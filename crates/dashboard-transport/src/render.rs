//! `GET /api/v1/ui/render?target=<id>[&view=<id>]` — render a node
//! using its kind's default SDUI view. See SDUI.md § S5.
//!
//! Zero authored pages required for the 90% case: a block author
//! declares `views: [{ id, title, template, priority }]` on their
//! `KindManifest`; this endpoint picks the highest-priority view (or
//! the one named by `view=`), substitutes `{{$target.*}}` bindings in
//! the template, and returns the same `ResolveResponse` shape
//! `/ui/resolve` emits — so clients consume it uniformly.
//!
//! Binding grammar supported for S5: `$target.<slot>` (slot read),
//! `$target.path`, `$target.id`, `$target.name`. The full `/` child
//! traversal from DASHBOARD.md composes with this once the template
//! tree needs it — out of scope for the S5 spine.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use dashboard_runtime::NodeReader;
use graph::KindRegistry;
use query::{execute, parse_only, QueryRequest};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use spi::{KindView, NodeId};
use ui_ir::ComponentTree;

use crate::error::TransportError;
use crate::resolve::{ResolveMeta, ResolveResponse, SubscriptionPlan};
use crate::state::DashboardState;

#[derive(Debug, Deserialize)]
pub struct RenderParams {
    /// Target node id (UUID).
    pub target: String,
    /// Optional view id. If absent, the highest-priority view wins.
    #[serde(default)]
    pub view: Option<String>,
}

pub async fn handler(
    State(state): State<DashboardState>,
    Query(params): Query<RenderParams>,
) -> Result<Json<ResolveResponse>, TransportError> {
    let kinds = state
        .kinds
        .clone()
        .ok_or_else(|| TransportError::Unavailable("kind registry not wired".into()))?;

    let target_id: NodeId = params
        .target
        .parse()
        .map(NodeId)
        .map_err(|e| TransportError::BadRequest(format!("target: {e}")))?;

    let target = state
        .reader
        .get(&target_id)
        .ok_or(TransportError::NotFound(target_id))?;

    // If the target is itself an authored `ui.page`, render its
    // `layout` slot directly — same fast-path `/ui/resolve` takes.
    // Saves the user from having to know which endpoint to call for
    // which kind of node.
    if target.kind.as_str() == "ui.page" {
        if let Some(layout) = target.slots.get("layout") {
            if !layout.is_null() {
                return render_layout_slot(layout.clone(), &target, &*state.reader);
            }
        }
    }

    let manifest = kinds.get(&target.kind).ok_or_else(|| {
        TransportError::BadRequest(format!(
            "no kind registered for `{}`",
            target.kind.as_str()
        ))
    })?;

    let view = pick_view(&manifest.views, params.view.as_deref()).ok_or_else(|| {
        TransportError::NoViewForKind {
            node: target_id,
            kind: target.kind.as_str().into(),
        }
    })?;

    let substituted = substitute_bindings(view.template.clone(), &target);
    let render: ComponentTree = serde_json::from_value(substituted).map_err(|e| {
        TransportError::MalformedView {
            kind: target.kind.as_str().into(),
            reason: format!("view template is not a ComponentTree: {e}"),
        }
    })?;

    let subscriptions =
        derive_subscriptions(&view.template, Some(&target), &*state.reader);

    let meta = ResolveMeta {
        cache_key: target.version,
        widget_count: 1,
        forbidden_count: 0,
        dangling_count: 0,
        stack_shadowed: vec![],
    };
    Ok(Json(ResolveResponse::Ok {
        render,
        subscriptions,
        meta,
    }))
}

fn render_layout_slot(
    mut layout: JsonValue,
    target: &dashboard_runtime::NodeSnapshot,
    reader: &(dyn NodeReader + Send + Sync),
) -> Result<Json<ResolveResponse>, TransportError> {
    assign_synthetic_ids(&mut layout);
    let render: ComponentTree = serde_json::from_value(layout.clone()).map_err(|e| {
        TransportError::MalformedPage(
            target.id,
            format!("layout is not a valid ComponentTree: {e}"),
        )
    })?;
    let subscriptions = derive_subscriptions(&layout, Some(target), reader);
    let meta = ResolveMeta {
        cache_key: target.version,
        widget_count: 1,
        forbidden_count: 0,
        dangling_count: 0,
        stack_shadowed: vec![],
    };
    Ok(Json(ResolveResponse::Ok {
        render,
        subscriptions,
        meta,
    }))
}

fn pick_view<'a>(views: &'a [KindView], requested: Option<&str>) -> Option<&'a KindView> {
    if let Some(id) = requested {
        return views.iter().find(|v| v.id == id);
    }
    views.iter().max_by_key(|v| v.priority)
}

/// Walks a JSON value, substituting `{{$target.<path>}}` expressions in
/// string leaves. Whole-string expressions are replaced with the raw
/// JSON value (preserving number / bool / object types); interpolated
/// expressions (embedded in larger strings) are stringified.
fn substitute_bindings(value: JsonValue, target: &dashboard_runtime::NodeSnapshot) -> JsonValue {
    match value {
        JsonValue::String(s) => substitute_string(s, target),
        JsonValue::Array(a) => {
            JsonValue::Array(a.into_iter().map(|v| substitute_bindings(v, target)).collect())
        }
        JsonValue::Object(m) => JsonValue::Object(
            m.into_iter()
                .map(|(k, v)| (k, substitute_bindings(v, target)))
                .collect(),
        ),
        other => other,
    }
}

fn substitute_string(s: String, target: &dashboard_runtime::NodeSnapshot) -> JsonValue {
    let trimmed = s.trim();
    if let Some(expr) = whole_binding(trimmed) {
        match eval_target_expr(expr, target) {
            Some(JsonValue::Null) | None => return JsonValue::String(String::new()),
            Some(val) => return val,
        }
    }
    // Inline interpolation: replace every `{{...}}` occurrence.
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = s[i + 2..].find("}}") {
                let expr = &s[i + 2..i + 2 + end];
                let replacement = eval_target_expr(expr.trim(), target)
                    .map(|v| match v {
                        JsonValue::String(x) => x,
                        other => other.to_string(),
                    })
                    .unwrap_or_else(|| format!("{{{{{expr}}}}}"));
                out.push_str(&replacement);
                i += 2 + end + 2;
                continue;
            }
        }
        out.push(s.as_bytes()[i] as char);
        i += 1;
    }
    JsonValue::String(out)
}

fn whole_binding(s: &str) -> Option<&str> {
    let inner = s.strip_prefix("{{")?.strip_suffix("}}")?;
    // Reject strings with any extra `{{` or `}}` in the middle.
    if inner.contains("{{") || inner.contains("}}") {
        return None;
    }
    Some(inner.trim())
}

fn eval_target_expr(expr: &str, target: &dashboard_runtime::NodeSnapshot) -> Option<JsonValue> {
    let rest = expr.strip_prefix("$target")?;
    if rest.is_empty() {
        return Some(JsonValue::String(target.id.to_string()));
    }
    let rest = rest.strip_prefix('.')?;
    match rest {
        "id" => Some(JsonValue::String(target.id.to_string())),
        "path" => Some(JsonValue::String(target.path.clone().unwrap_or_default())),
        "name" => {
            let p = target.path.clone().unwrap_or_default();
            let name = p.rsplit('/').next().unwrap_or("").to_string();
            Some(JsonValue::String(name))
        }
        "kind" => Some(JsonValue::String(target.kind.as_str().into())),
        slot => target.slots.get(slot).cloned(),
    }
}

/// Derive subscription subjects from a template.
///
/// Two sources contribute:
///
/// 1. `{{$target.<slot>}}` references anywhere in the template → subject
///    `node.<target-id>.slot.<slot>` (only when a `target` is supplied).
/// 2. `{"type": "table", "source": {"query": "...", "subscribe": true}}`
///    nodes → the query is executed against the reader and a subject is
///    emitted for every slot of every matching node. The client
///    invalidates on any of them → the table re-fetches its page →
///    rows stay in sync with the tick stream.
///
/// All subjects land in a single `SubscriptionPlan` keyed off the target
/// (or a nil id when rendering a bare page). The React hook in
/// `useSubscriptions.ts` does the exact-subject lookup.
/// Public entry point reused by `resolve.rs`'s SDUI fast-path so the
/// authored-`ui.page` case gets the same subscription derivation as
/// `/ui/render`.
pub(crate) fn derive_subscriptions_for_layout(
    layout: &JsonValue,
    reader: &(dyn NodeReader + Send + Sync),
) -> Vec<SubscriptionPlan> {
    derive_subscriptions(layout, None, reader)
}

/// Walk the layout tree and inject deterministic ids on `chart`,
/// `table`, `sparkline`, and `timeline` components missing one. The
/// IR makes `id` optional on these variants, but every subscription
/// plan needs an id to key the client's per-widget patch path. Without
/// this pass an authored layout that omits `id` silently produces no
/// subscription plan and the widget never live-updates.
///
/// IDs are assigned by depth-first encounter order, separately per
/// component type: `auto:chart:0`, `auto:table:0`, etc. Stable as long
/// as the tree shape doesn't change, which is sufficient — both the
/// derived plan and the rendered tree are regenerated together on
/// every resolve, so the client sees matching ids in both.
pub(crate) fn assign_synthetic_ids(layout: &mut JsonValue) {
    let mut counters = SyntheticIdCounters::default();
    assign_ids_walk(layout, &mut counters);
}

#[derive(Default)]
struct SyntheticIdCounters {
    chart: usize,
    table: usize,
    sparkline: usize,
    timeline: usize,
}

fn assign_ids_walk(v: &mut JsonValue, counters: &mut SyntheticIdCounters) {
    match v {
        JsonValue::Array(a) => {
            for item in a.iter_mut() {
                assign_ids_walk(item, counters);
            }
        }
        JsonValue::Object(m) => {
            let kind = m.get("type").and_then(|v| v.as_str()).map(str::to_owned);
            let needs_id = m
                .get("id")
                .map(|v| v.as_str().map(str::is_empty).unwrap_or(true))
                .unwrap_or(true);
            if needs_id {
                if let Some(prefix_and_n) = match kind.as_deref() {
                    Some("chart") => {
                        let n = counters.chart;
                        counters.chart += 1;
                        Some(("chart", n))
                    }
                    Some("table") => {
                        let n = counters.table;
                        counters.table += 1;
                        Some(("table", n))
                    }
                    Some("sparkline") => {
                        let n = counters.sparkline;
                        counters.sparkline += 1;
                        Some(("sparkline", n))
                    }
                    Some("timeline") => {
                        let n = counters.timeline;
                        counters.timeline += 1;
                        Some(("timeline", n))
                    }
                    _ => None,
                } {
                    let (prefix, n) = prefix_and_n;
                    m.insert(
                        "id".to_string(),
                        JsonValue::String(format!("auto:{prefix}:{n}")),
                    );
                }
            }
            for val in m.values_mut() {
                assign_ids_walk(val, counters);
            }
        }
        _ => {}
    }
}

fn derive_subscriptions(
    template: &JsonValue,
    target: Option<&dashboard_runtime::NodeSnapshot>,
    reader: &(dyn NodeReader + Send + Sync),
) -> Vec<SubscriptionPlan> {
    let mut plans: Vec<SubscriptionPlan> = Vec::new();

    // (1) $target.<slot> bindings baked into the tree (e.g. a Badge
    // label `{{$target.current_state}}`). When any of these slots
    // change, the tree itself needs to be re-resolved — so the plan is
    // keyed off the target node's id. The client treats `widget_id ==
    // target.id` as "invalidate the render/resolve query".
    if let Some(t) = target {
        let mut slots: BTreeSet<String> = BTreeSet::new();
        collect_target_slots(template, &mut slots);
        if !slots.is_empty() {
            let subjects: Vec<String> = slots
                .iter()
                .map(|s| format!("node.{}.slot.{}", t.id, s))
                .collect();
            plans.push(SubscriptionPlan {
                widget_id: t.id.to_string(),
                subjects,
                debounce_ms: 250,
            });
        }
    }

    // (2) One plan per `table` with `subscribe: true`. The plan's
    // `widget_id` is the table component's id — the client uses that
    // to invalidate just the sdui-table query, not the whole tree.
    collect_table_plans(template, reader, &mut plans);

    // (3) Charts + sparklines — each carries its own node+slot
    // subscription target, no RSQL query involved. Plan widget_id is
    // the component's authored id; the hook routes invalidations
    // per-widget.
    collect_chart_plans(template, &mut plans);

    plans
}

fn collect_target_slots(v: &JsonValue, acc: &mut BTreeSet<String>) {
    match v {
        JsonValue::String(s) => scan_bindings(s, acc),
        JsonValue::Array(a) => a.iter().for_each(|x| collect_target_slots(x, acc)),
        JsonValue::Object(m) => m.values().for_each(|x| collect_target_slots(x, acc)),
        _ => {}
    }
}

fn scan_bindings(s: &str, acc: &mut BTreeSet<String>) {
    crate::binding_walk::for_each_binding_expr(
        s,
        &mut |expr| {
            if let Some(tail) = expr.strip_prefix("$target.") {
                if !matches!(tail, "id" | "path" | "name" | "kind") {
                    acc.insert(tail.to_string());
                }
            }
        },
        &mut || {},
    );
}

/// Walk the tree, find every `chart` component with a `source.{node_id,
/// slot}` and emit a subscription subject for live ticks. Complements
/// the table-plan collector; the two are composable.
fn collect_chart_plans(v: &JsonValue, plans: &mut Vec<SubscriptionPlan>) {
    match v {
        JsonValue::Array(a) => a.iter().for_each(|x| collect_chart_plans(x, plans)),
        JsonValue::Object(m) => {
            let is_chart = m.get("type").and_then(|v| v.as_str()) == Some("chart");
            let is_spark = m.get("type").and_then(|v| v.as_str()) == Some("sparkline");
            if is_chart {
                let src = m.get("source").and_then(|v| v.as_object());
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if let (Some(src), true) = (src, !id.is_empty()) {
                    let node = src.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
                    let slot = src.get("slot").and_then(|v| v.as_str()).unwrap_or("");
                    if !node.is_empty() && !slot.is_empty() {
                        plans.push(SubscriptionPlan {
                            widget_id: id.to_string(),
                            subjects: vec![format!("node.{node}.slot.{slot}")],
                            debounce_ms: 250,
                        });
                    }
                }
            }
            if is_spark {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let subj = m.get("subscribe").and_then(|v| v.as_str()).unwrap_or("");
                if !id.is_empty() && !subj.is_empty() {
                    plans.push(SubscriptionPlan {
                        widget_id: id.to_string(),
                        subjects: vec![subj.to_string()],
                        debounce_ms: 250,
                    });
                }
            }
            for val in m.values() {
                collect_chart_plans(val, plans);
            }
        }
        _ => {}
    }
}

fn collect_table_plans(
    v: &JsonValue,
    reader: &(dyn NodeReader + Send + Sync),
    plans: &mut Vec<SubscriptionPlan>,
) {
    match v {
        JsonValue::Array(a) => a.iter().for_each(|x| collect_table_plans(x, reader, plans)),
        JsonValue::Object(m) => {
            let is_table = m.get("type").and_then(|v| v.as_str()) == Some("table");
            if is_table {
                if let Some(source) = m.get("source").and_then(|v| v.as_object()) {
                    let subscribe = source
                        .get("subscribe")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let query = source.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    let table_id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if subscribe && !query.is_empty() && !table_id.is_empty() {
                        let mut subjects: BTreeSet<String> = BTreeSet::new();
                        emit_query_subjects(query, reader, &mut subjects);
                        if !subjects.is_empty() {
                            plans.push(SubscriptionPlan {
                                widget_id: table_id.to_string(),
                                subjects: subjects.into_iter().collect(),
                                debounce_ms: 250,
                            });
                        }
                    }
                }
            }
            // Recurse — tables can be nested under rows / cols / tabs.
            for val in m.values() {
                collect_table_plans(val, reader, plans);
            }
        }
        _ => {}
    }
}

fn emit_query_subjects(
    query: &str,
    reader: &(dyn NodeReader + Send + Sync),
    acc: &mut BTreeSet<String>,
) {
    // Reuse the same query engine the table endpoint uses. A failed
    // parse is non-fatal — we just skip subscription derivation for
    // this table; the client still renders, just without live updates.
    let req = QueryRequest {
        filter: Some(query.to_string()),
        sort: None,
        page: Some(1),
        size: Some(500),
    };
    let Ok(validated) = parse_only(req, 500) else { return };
    let rows: Vec<TableRow> = reader
        .list_all()
        .into_iter()
        .map(|snap| TableRow {
            id: snap.id.0.to_string(),
            kind: snap.kind.as_str().to_string(),
            path: snap.path.unwrap_or_default(),
            parent_id: snap.parent_id,
            slots: snap.slots,
        })
        .collect();
    let Ok(page) = execute(rows, &validated) else { return };
    for row in &page.data {
        for slot_name in row.slots.keys() {
            acc.insert(format!("node.{}.slot.{}", row.id, slot_name));
        }
    }
}

// Mirror of `TableRow` from `table.rs` — duplicated here to avoid a
// circular use. Private.
#[derive(serde::Serialize)]
struct TableRow {
    id: String,
    kind: String,
    path: String,
    parent_id: Option<String>,
    slots: std::collections::HashMap<String, JsonValue>,
}

// ---- helpers --------------------------------------------------------------

// Force `Arc<KindRegistry>` to be referenced so clippy doesn't flag the
// import as unused in minimal builds.
#[allow(dead_code)]
fn _type_anchor(_r: Arc<KindRegistry>) {}

#[cfg(test)]
mod synthetic_id_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn assigns_ids_to_chart_and_table_when_missing() {
        let mut layout = json!({
            "root": {
                "type": "page",
                "id": "p",
                "children": [
                    {
                        "type": "chart",
                        "source": {"node_id": "n1", "slot": "value"}
                    },
                    {
                        "type": "table",
                        "source": {"query": "kind==\"x\"", "subscribe": true},
                        "columns": []
                    }
                ]
            }
        });
        assign_synthetic_ids(&mut layout);
        let kids = &layout["root"]["children"];
        assert_eq!(kids[0]["id"], "auto:chart:0");
        assert_eq!(kids[1]["id"], "auto:table:0");
    }

    #[test]
    fn preserves_authored_ids() {
        let mut layout = json!({
            "type": "chart",
            "id": "my-chart",
            "source": {"node_id": "n", "slot": "v"}
        });
        assign_synthetic_ids(&mut layout);
        assert_eq!(layout["id"], "my-chart");
    }

    #[test]
    fn synthetic_id_drives_chart_plan() {
        let mut layout = json!({
            "type": "chart",
            "source": {"node_id": "11111111-1111-1111-1111-111111111111", "slot": "value"}
        });
        assign_synthetic_ids(&mut layout);
        let mut plans = Vec::new();
        collect_chart_plans(&layout, &mut plans);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].widget_id, "auto:chart:0");
        assert_eq!(
            plans[0].subjects,
            vec!["node.11111111-1111-1111-1111-111111111111.slot.value"]
        );
    }
}
