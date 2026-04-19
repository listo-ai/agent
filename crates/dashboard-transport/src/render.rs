//! `GET /api/v1/ui/render?target=<id>[&view=<id>]` — render a node
//! using its kind's default SDUI view. See SDUI.md § S5.
//!
//! Zero authored pages required for the 90% case: a plugin author
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
    layout: JsonValue,
    target: &dashboard_runtime::NodeSnapshot,
    reader: &(dyn NodeReader + Send + Sync),
) -> Result<Json<ResolveResponse>, TransportError> {
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

fn derive_subscriptions(
    template: &JsonValue,
    target: Option<&dashboard_runtime::NodeSnapshot>,
    reader: &(dyn NodeReader + Send + Sync),
) -> Vec<SubscriptionPlan> {
    let mut subjects: BTreeSet<String> = BTreeSet::new();

    // (1) $target.<slot> bindings.
    if let Some(t) = target {
        let mut slots: BTreeSet<String> = BTreeSet::new();
        collect_target_slots(template, &mut slots);
        for s in slots {
            subjects.insert(format!("node.{}.slot.{}", t.id, s));
        }
    }

    // (2) Tables with subscribe:true — run the query, emit per-slot
    // subjects for every matched node. This makes live-update work for
    // tables whose rows are bound purely by RSQL (no $target threading).
    collect_table_subjects(template, reader, &mut subjects);

    if subjects.is_empty() {
        return vec![];
    }
    let widget_id = target.map(|t| t.id).unwrap_or_default();
    vec![SubscriptionPlan {
        widget_id,
        subjects: subjects.into_iter().collect(),
        debounce_ms: 250,
    }]
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
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let expr = after[..end].trim();
        if let Some(tail) = expr.strip_prefix("$target.") {
            if !matches!(tail, "id" | "path" | "name" | "kind") {
                acc.insert(tail.to_string());
            }
        }
        rest = &after[end + 2..];
    }
}

fn collect_table_subjects(
    v: &JsonValue,
    reader: &(dyn NodeReader + Send + Sync),
    acc: &mut BTreeSet<String>,
) {
    match v {
        JsonValue::Array(a) => a.iter().for_each(|x| collect_table_subjects(x, reader, acc)),
        JsonValue::Object(m) => {
            let is_table = m.get("type").and_then(|v| v.as_str()) == Some("table");
            if is_table {
                if let Some(source) = m.get("source").and_then(|v| v.as_object()) {
                    let subscribe = source
                        .get("subscribe")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let query = source.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    if subscribe && !query.is_empty() {
                        emit_query_subjects(query, reader, acc);
                    }
                }
            }
            // Recurse into every field — tables can be nested under
            // rows/cols/tabs.
            for val in m.values() {
                collect_table_subjects(val, reader, acc);
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
