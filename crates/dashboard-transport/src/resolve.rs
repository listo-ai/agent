//! `POST /ui/resolve` — resolve a `ui.page` into a typed SDUI
//! component tree.
//!
//! Request body:
//!
//! ```json
//! { "page_ref": "<uuid>", "stack": ["<nav-uuid>", ...],
//!   "page_state": { ... }, "dry_run": false }
//! ```
//!
//! The endpoint reads the page node's `layout` slot (a JSON
//! `ComponentTree`), derives a subscription plan so the client can
//! live-update via SSE, and returns `{render, subscriptions, meta}`.
//! With `dry_run: true` the handler validates the layout parses and
//! returns `{errors}` instead of a tree — used by AI authoring tools.

use axum::extract::State;
use axum::Json;
use dashboard_runtime::NodeSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::NodeId;
use ui_ir::{Component, ComponentTree};

use crate::error::TransportError;
use crate::limits;
use crate::state::DashboardState;

const PAGE_KIND: &str = "ui.page";

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub page_ref: NodeId,
    #[serde(default)]
    pub stack: Vec<NodeId>,
    #[serde(default = "empty_object")]
    pub page_state: JsonValue,
    #[serde(default)]
    pub dry_run: bool,
    /// Opaque auth subject identifier threaded into the cache key and
    /// audit events.
    #[serde(default)]
    pub auth_subject: Option<String>,
    /// User claims available as `$user.*` in bindings.
    #[serde(default)]
    pub user_claims: std::collections::HashMap<String, JsonValue>,
    /// Candidate layout to resolve in place of the node's persisted
    /// `layout` slot. Honoured on both `dry_run` (for validation) and
    /// live resolve (for the builder's live preview, which needs the
    /// subscription plan derived from the in-flight buffer rather
    /// than the last-saved slot).
    #[serde(default)]
    pub layout: Option<JsonValue>,
}

fn empty_object() -> JsonValue {
    JsonValue::Object(serde_json::Map::new())
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ResolveResponse {
    Ok {
        render: ComponentTree,
        subscriptions: Vec<SubscriptionPlan>,
        /// Write plan — one entry per two-way bound control (`toggle`,
        /// `slider`) in the resolved tree. Keyed on `component_id`.
        /// Absent entries mean the control is read-only for this caller
        /// (ACL-denied write or binding error).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writes: Vec<WritePlanEntry>,
        meta: ResolveMeta,
    },
    DryRun {
        errors: Vec<ResolveIssue>,
    },
}

/// Subscription plan emitted alongside a resolve/render response.
///
/// Two plan shapes flow to the client:
///
/// * **Tree-binding plan** — `widget_id` is the target node's id (as a
///   string). The template contains `{{$target.<slot>}}` references
///   whose values are baked into the tree; when any listed slot
///   changes, the *whole* resolve / render query must be invalidated
///   so the new tree is served.
/// * **Table plan** — `widget_id` is the authored `table` component's
///   id from the IR tree (e.g. `"alarms"`, `"t"`). `subscribe: true`
///   on the table's source drives the plan's subjects from whichever
///   nodes the RSQL query currently matches. The client invalidates
///   just that table's React-Query key — rows refetch without
///   re-resolving the tree.
#[derive(Debug, Serialize)]
pub struct SubscriptionPlan {
    pub widget_id: String,
    pub subjects: Vec<String>,
    pub debounce_ms: u32,
    /// Optional dot-path into the slot value the widget wants to
    /// extract (e.g. `payload.count` for a Msg envelope). Populated
    /// for chart / kpi plans that set `source.field`; omitted for
    /// table plans and for widgets without `field`. The client uses
    /// this in the live-tick patch path so it can apply the same
    /// extraction the initial fetch applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// One entry in the write plan emitted alongside a resolved tree.
///
/// Clients look up the entry by `component_id` when a bound control
/// changes and POST `{path, slot, value[, expected_generation]}` to
/// `POST /api/v1/slots`. A missing entry (ACL-denied, binding error)
/// means the control renders disabled.
///
/// **LWW** entries omit `generation` (send write without
/// `expected_generation`). **OCC** entries carry `generation` baked
/// from the current slot at resolve time; the client updates it on
/// every `slot_changed` SSE echo before the next write.
///
/// NOTE: per-slot generation baking (OCC, S5) requires `NodeSnapshot`
/// to expose `slot_generations: HashMap<String, u64>`. That field is
/// not yet present — OCC entries carry `generation: None` until the
/// graph reader is extended (see SDUI-WRITE-PATH.md § S5 flag).
#[derive(Debug, Clone, Serialize)]
pub struct WritePlanEntry {
    /// The IR component's `id` field — used by the client as the lookup key.
    pub component_id: String,
    /// Absolute node path (e.g. `/buildings/building-1`).
    pub path: String,
    /// Slot name (e.g. `"enabled"`, `"brightness"`).
    pub slot: String,
    /// Concurrency mode. `"lww"` omits `expected_generation` on
    /// writes; `"occ"` requires it.
    pub concurrency: ui_ir::Concurrency,
    /// For `"occ"` entries: current slot generation baked at resolve
    /// time. Client updates this on every `slot_changed` SSE echo.
    /// `None` for `"lww"` entries and until per-slot generation is
    /// available in `NodeSnapshot`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ResolveMeta {
    pub cache_key: u64,
    pub widget_count: usize,
    pub forbidden_count: usize,
    pub dangling_count: usize,
    pub stack_shadowed: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolveIssue {
    pub location: String,
    pub message: String,
}

pub async fn handler(
    State(state): State<DashboardState>,
    Json(req): Json<ResolveRequest>,
) -> Result<Json<ResolveResponse>, TransportError> {
    enforce_page_state_size(&req.page_state)?;

    let page = state
        .reader
        .get(&req.page_ref)
        .ok_or(TransportError::PageNotFound(req.page_ref))?;
    require_kind(&page, PAGE_KIND)?;

    // Layout override (inline candidate) takes precedence over the
    // persisted slot for both dry-run and live resolve. The builder
    // uses this on every keystroke so its preview + subscription plan
    // reflect the in-flight buffer instead of the last-saved tree.
    let persisted = page.slots.get("layout").cloned().filter(|v| !v.is_null());
    let mut layout = req.layout.clone().or(persisted).ok_or_else(|| {
        TransportError::MalformedPage(page.id, "page has no `layout` slot".into())
    })?;
    // Inject synthetic ids for chart/table/sparkline/timeline that
    // omitted them, so the subscription plan and the rendered tree
    // both carry the same key for the client to patch against.
    crate::render::assign_synthetic_ids(&mut layout);

    // Substitute `{{$page.*}} / {{$stack.*}} / {{$user.*}} / {{$self.*}}`
    // in fields where bindings gate data fetching (today: a table's
    // `source.query`). Done on the live path; dry-run validates the
    // raw template so authors see their un-substituted bindings when
    // they browse errors.
    if !req.dry_run {
        substitute_query_bindings(&mut layout, &page, &req, &*state.reader);
    }

    if req.dry_run {
        if let Err(e) = serde_json::from_value::<ComponentTree>(layout.clone()) {
            return Ok(Json(ResolveResponse::DryRun {
                errors: vec![ResolveIssue {
                    location: format!("page/{}/layout", page.id),
                    message: format!("layout is not a valid ComponentTree: {e}"),
                }],
            }));
        }
        let errors = collect_binding_issues(&page, &layout, &req, &*state.reader);
        return Ok(Json(ResolveResponse::DryRun { errors }));
    }

    // Shape errors on the live path return a structured DryRun-style
    // response instead of a 422 — clients render `errors[]` cleanly
    // via their existing dry-run branch and the preview surface
    // doesn't need a separate error pane.
    let render: ComponentTree = match serde_json::from_value(layout.clone()) {
        Ok(t) => t,
        Err(e) => {
            return Ok(Json(ResolveResponse::DryRun {
                errors: vec![ResolveIssue {
                    location: format!("page/{}/layout", page.id),
                    message: format!("layout is not a valid ComponentTree: {e}"),
                }],
            }));
        }
    };

    enforce_render_tree_size(&render)?;
    enforce_tree_shape(&render)?;

    let subscriptions = crate::render::derive_subscriptions_for_layout(&layout, &*state.reader);
    enforce_subscription_cap(&subscriptions)?;

    // Resolve the target node for write-plan derivation. In an authored
    // page, $target is the first stack frame when one is provided.
    let target_for_writes = req.stack.first().and_then(|id| state.reader.get(id));
    let writes = crate::render::derive_write_plan_for_layout(
        &layout,
        target_for_writes.as_ref(),
        &*state.reader,
        &*state.acl,
        req.auth_subject.as_deref(),
        &*state.audit,
    );

    let meta = ResolveMeta {
        cache_key: page.version,
        widget_count: count_components(&render.root),
        forbidden_count: 0,
        dangling_count: 0,
        stack_shadowed: vec![],
    };

    Ok(Json(ResolveResponse::Ok {
        render,
        subscriptions,
        writes,
        meta,
    }))
}

/// Pull the `vars` map off the layout JSON. Missing / non-object →
/// empty map. Kept outside the ComponentTree parse path because
/// query-templating runs before we know the tree is structurally
/// valid; a partial or still-invalid tree can still have a vars
/// block we want to honour.
fn extract_tree_vars(layout: &JsonValue) -> std::collections::HashMap<String, JsonValue> {
    layout
        .as_object()
        .and_then(|m| m.get("vars"))
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

fn substitute_query_bindings(
    layout: &mut JsonValue,
    page: &NodeSnapshot,
    req: &ResolveRequest,
    reader: &(dyn dashboard_runtime::NodeReader + Send + Sync),
) {
    use dashboard_runtime::{Binding, ContextStack, EvalContext};
    let stack = ContextStack::build(reader, &req.stack, 128).unwrap_or_default();
    let vars = extract_tree_vars(layout);
    let ctx = EvalContext {
        reader,
        stack: &stack,
        self_id: page.id,
        user_claims: &req.user_claims,
        page_state: &req.page_state,
        vars: &vars,
        access_log: None,
    };
    walk_string_leaves_mut(layout, &|s| {
        if !s.contains("{{") {
            return None;
        }
        Some(crate::binding_walk::substitute_bindings(s, |expr| {
            Binding::parse(expr)
                .ok()
                .and_then(|b| b.evaluate(&ctx).ok())
        }))
    });
}

/// Walk every JSON string leaf and apply `rewrite`. `None` means
/// "leave unchanged"; `Some(s)` replaces the string.
///
/// Generic coverage is intentional — authors write `{{$vars.*}}` and
/// friends in any field (node ids, slot names, labels, queries). The
/// server substitutes at resolve time so downstream consumers (chart
/// fetch, table query, subscription-plan derivation) all see
/// concrete values. Strings with no `{{` short-circuit.
fn walk_string_leaves_mut<F: Fn(&str) -> Option<String>>(v: &mut JsonValue, rewrite: &F) {
    match v {
        JsonValue::String(s) => {
            if let Some(next) = rewrite(s) {
                *s = next;
            }
        }
        JsonValue::Array(arr) => {
            for x in arr {
                walk_string_leaves_mut(x, rewrite);
            }
        }
        JsonValue::Object(m) => {
            // Skip the `vars` subtree — vars *are* the substitution
            // source, substituting into them would recurse.
            for (k, val) in m.iter_mut() {
                if k == "vars" {
                    continue;
                }
                walk_string_leaves_mut(val, rewrite);
            }
        }
        _ => {}
    }
}

fn collect_binding_issues(
    page: &NodeSnapshot,
    layout: &JsonValue,
    req: &ResolveRequest,
    reader: &(dyn dashboard_runtime::NodeReader + Send + Sync),
) -> Vec<ResolveIssue> {
    use dashboard_runtime::{Binding, ContextStack, EvalContext};

    let issues_cell: std::cell::RefCell<Vec<ResolveIssue>> = std::cell::RefCell::new(Vec::new());

    // Build the stack once — stack-build failures are themselves
    // issues (missing aliased frames, for example).
    let stack = match ContextStack::build(reader, &req.stack, 128) {
        Ok(s) => s,
        Err(e) => {
            issues_cell.borrow_mut().push(ResolveIssue {
                location: "root".into(),
                message: format!("stack build failed: {e}"),
            });
            ContextStack::empty()
        }
    };

    // Declared `$page.*` fields, if the page carries a schema. A
    // `null`/missing schema skips the check entirely.
    let declared_page_fields: Option<std::collections::HashSet<String>> = page
        .slots
        .get("page_state_schema")
        .and_then(|v| v.as_object())
        .and_then(|m| m.get("properties"))
        .and_then(|p| p.as_object())
        .map(|m| m.keys().cloned().collect());

    // ComponentTree-scoped vars — pulled from the layout itself so
    // $vars bindings resolve against the candidate tree, not the
    // persisted one.
    let tree_vars = extract_tree_vars(layout);

    crate::binding_walk::walk_string_leaves(layout, "root", &mut |loc, s| {
        crate::binding_walk::for_each_binding_expr(
            s,
            &mut |expr| {
                let parsed = match Binding::parse(expr) {
                    Ok(b) => b,
                    Err(err) => {
                        issues_cell.borrow_mut().push(ResolveIssue {
                            location: loc.into(),
                            message: err.to_string(),
                        });
                        return;
                    }
                };
                if let (Some(declared), dashboard_runtime::Source::PageField(field)) =
                    (declared_page_fields.as_ref(), &parsed.source)
                {
                    if !declared.contains(field) {
                        issues_cell.borrow_mut().push(ResolveIssue {
                            location: loc.into(),
                            message: format!("unresolved $page.{field} — not declared in page_state_schema.properties"),
                        });
                        return;
                    }
                }
                let ctx = EvalContext {
                    reader,
                    stack: &stack,
                    self_id: page.id,
                    user_claims: &req.user_claims,
                    page_state: &req.page_state,
                    vars: &tree_vars,
                    access_log: None,
                };
                if let Err(err) = parsed.evaluate(&ctx) {
                    issues_cell.borrow_mut().push(ResolveIssue {
                        location: loc.into(),
                        message: err.to_string(),
                    });
                }
            },
            &mut || {
                issues_cell.borrow_mut().push(ResolveIssue {
                    location: loc.into(),
                    message: "unterminated `{{` binding expression".into(),
                });
            },
        );
    });

    // Validate `bind` fields on two-way bound controls (toggle, slider).
    // These carry bare binding expressions (not wrapped in `{{...}}`),
    // so the general walk above doesn't reach them. We walk the tree
    // separately and validate that:
    //   1. The expression is syntactically valid.
    //   2. For `$target.*` expressions: a target frame is available and
    //      the slot name is not a meta field (id, path, name, kind).
    //   3. The expression resolves to *something* — not a missing slot.
    let target_node = req.stack.first().and_then(|id| reader.get(id));
    collect_write_binding_issues(layout, target_node.as_ref(), &issues_cell);

    issues_cell.into_inner()
}

/// Walk the JSON tree and validate `bind` fields on `toggle` / `slider`
/// components. Issues are pushed into `cells`.
fn collect_write_binding_issues(
    v: &JsonValue,
    target: Option<&dashboard_runtime::NodeSnapshot>,
    issues: &std::cell::RefCell<Vec<ResolveIssue>>,
) {
    match v {
        JsonValue::Array(a) => {
            for item in a {
                collect_write_binding_issues(item, target, issues);
            }
        }
        JsonValue::Object(m) => {
            let kind = m.get("type").and_then(|v| v.as_str());
            if matches!(kind, Some("toggle") | Some("slider")) {
                let component_id = m.get("id").and_then(|v| v.as_str()).unwrap_or("<unnamed>");
                let loc = format!("{kind}.{component_id}.bind", kind = kind.unwrap_or("?"));

                if let Some(bind_val) = m.get("bind") {
                    // Extract the raw slot expression.
                    let expr: std::borrow::Cow<str> = match bind_val {
                        JsonValue::String(s) => std::borrow::Cow::Borrowed(s.as_str()),
                        JsonValue::Object(obj) => obj
                            .get("slot")
                            .and_then(|v| v.as_str())
                            .map(std::borrow::Cow::Borrowed)
                            .unwrap_or(std::borrow::Cow::Borrowed("")),
                        _ => std::borrow::Cow::Borrowed(""),
                    };

                    if expr.is_empty() {
                        issues.borrow_mut().push(ResolveIssue {
                            location: loc.clone(),
                            message: "bind.slot is empty or missing".into(),
                        });
                    } else if let Some(tail) = expr.strip_prefix("$target.") {
                        // $target.<slot> — most common case.
                        let slot = tail.split('.').next().unwrap_or(tail);
                        if matches!(slot, "id" | "path" | "name" | "kind") {
                            issues.borrow_mut().push(ResolveIssue {
                                location: loc.clone(),
                                message: format!(
                                    "bind `{expr}` resolves to a meta field `{slot}`, \
                                     not a writable slot"
                                ),
                            });
                        } else if target.is_none() {
                            issues.borrow_mut().push(ResolveIssue {
                                location: loc.clone(),
                                message: format!(
                                    "bind `{expr}` requires a $target context \
                                     but no stack frame was provided"
                                ),
                            });
                        } else if let Some(t) = target {
                            // Slot exists check — warn if the slot is absent
                            // in the current snapshot (may be a new slot not
                            // yet written, so this is a warning, not an error).
                            if !t.slots.contains_key(slot) {
                                issues.borrow_mut().push(ResolveIssue {
                                    location: loc.clone(),
                                    message: format!(
                                        "bind `{expr}`: slot `{slot}` not found on \
                                         target node (node may not have written it yet)"
                                    ),
                                });
                            }
                        }
                    } else {
                        // Not a recognised write-binding pattern.
                        issues.borrow_mut().push(ResolveIssue {
                            location: loc.clone(),
                            message: format!(
                                "bind `{expr}` is not a supported write-binding \
                                 expression (expected `$target.<slot>` form)"
                            ),
                        });
                    }
                } else {
                    issues.borrow_mut().push(ResolveIssue {
                        location: loc.clone(),
                        message: "bound control missing required `bind` field".into(),
                    });
                }
            }
            // Always recurse.
            for val in m.values() {
                collect_write_binding_issues(val, target, issues);
            }
        }
        _ => {}
    }
}

fn enforce_subscription_cap(subs: &[SubscriptionPlan]) -> Result<(), TransportError> {
    let total: usize = subs.iter().map(|p| p.subjects.len()).sum();
    if total > limits::MAX_SUBSCRIPTIONS_PER_PAGE {
        return Err(TransportError::LimitExceeded {
            what: "subscriptions_per_page",
            value: total,
            max: limits::MAX_SUBSCRIPTIONS_PER_PAGE,
        });
    }
    Ok(())
}

fn enforce_page_state_size(v: &JsonValue) -> Result<(), TransportError> {
    let len = serde_json::to_vec(v).map(|b| b.len()).unwrap_or(usize::MAX);
    if len > limits::MAX_PAGE_STATE_BYTES {
        return Err(TransportError::LimitExceeded {
            what: "page_state_bytes",
            value: len,
            max: limits::MAX_PAGE_STATE_BYTES,
        });
    }
    Ok(())
}

/// Walks the tree and enforces per-tree shape limits: node count,
/// nesting depth, distinct component types. Each violation becomes a
/// 413 with a distinct `what` tag so clients can branch cleanly.
pub fn enforce_tree_shape(tree: &ComponentTree) -> Result<(), TransportError> {
    use std::collections::BTreeSet;
    let mut count = 0usize;
    let mut max_depth = 0usize;
    let mut types: BTreeSet<&'static str> = BTreeSet::new();
    walk(&tree.root, 0, &mut count, &mut max_depth, &mut types);
    if count > limits::MAX_TREE_NODES {
        return Err(TransportError::LimitExceeded {
            what: "tree_nodes",
            value: count,
            max: limits::MAX_TREE_NODES,
        });
    }
    if max_depth > limits::MAX_TREE_DEPTH {
        return Err(TransportError::LimitExceeded {
            what: "tree_depth",
            value: max_depth,
            max: limits::MAX_TREE_DEPTH,
        });
    }
    if types.len() > limits::MAX_COMPONENT_TYPES {
        return Err(TransportError::LimitExceeded {
            what: "component_types",
            value: types.len(),
            max: limits::MAX_COMPONENT_TYPES,
        });
    }
    Ok(())
}

fn walk<'a>(
    c: &'a Component,
    depth: usize,
    count: &mut usize,
    max_depth: &mut usize,
    types: &mut std::collections::BTreeSet<&'static str>,
) {
    *count += 1;
    if depth > *max_depth {
        *max_depth = depth;
    }
    types.insert(variant_name(c));
    match c {
        Component::Page { children, .. }
        | Component::Row { children, .. }
        | Component::Col { children, .. }
        | Component::Grid { children, .. }
        | Component::Drawer { children, .. } => {
            for ch in children {
                walk(ch, depth + 1, count, max_depth, types);
            }
        }
        Component::Tabs { tabs, .. } => {
            for t in tabs {
                for ch in &t.children {
                    walk(ch, depth + 1, count, max_depth, types);
                }
            }
        }
        Component::Wizard { steps, .. } => {
            for s in steps {
                for ch in &s.children {
                    walk(ch, depth + 1, count, max_depth, types);
                }
            }
        }
        _ => {}
    }
}

fn variant_name(c: &Component) -> &'static str {
    match c {
        Component::Page { .. } => "page",
        Component::Row { .. } => "row",
        Component::Col { .. } => "col",
        Component::Grid { .. } => "grid",
        Component::Tabs { .. } => "tabs",
        Component::Text { .. } => "text",
        Component::Heading { .. } => "heading",
        Component::Badge { .. } => "badge",
        Component::Diff { .. } => "diff",
        Component::Chart { .. } => "chart",
        Component::Sparkline { .. } => "sparkline",
        Component::Table { .. } => "table",
        Component::Tree { .. } => "tree",
        Component::Timeline { .. } => "timeline",
        Component::Markdown { .. } => "markdown",
        Component::RichText { .. } => "rich_text",
        Component::RefPicker { .. } => "ref_picker",
        Component::DateRange { .. } => "date_range",
        Component::Select { .. } => "select",
        Component::Kpi { .. } => "kpi",
        Component::Wizard { .. } => "wizard",
        Component::Drawer { .. } => "drawer",
        Component::Button { .. } => "button",
        Component::Form { .. } => "form",
        Component::Toggle { .. } => "toggle",
        Component::Slider { .. } => "slider",
        Component::Forbidden { .. } => "forbidden",
        Component::Dangling { .. } => "dangling",
        Component::Custom { .. } => "custom",
    }
}

pub(crate) fn enforce_render_tree_size(tree: &ComponentTree) -> Result<(), TransportError> {
    let len = serde_json::to_vec(tree)
        .map(|b| b.len())
        .unwrap_or(usize::MAX);
    if len > limits::MAX_RENDER_TREE_BYTES {
        return Err(TransportError::LimitExceeded {
            what: "render_tree_bytes",
            value: len,
            max: limits::MAX_RENDER_TREE_BYTES,
        });
    }
    Ok(())
}

fn require_kind(snap: &NodeSnapshot, expected: &str) -> Result<(), TransportError> {
    if snap.kind.as_str() != expected {
        return Err(TransportError::KindMismatch {
            node: snap.id,
            expected: expected.into(),
            found: snap.kind.as_str().into(),
        });
    }
    Ok(())
}

fn count_components(c: &Component) -> usize {
    let children: &[Component] = match c {
        Component::Page { children, .. } => children,
        Component::Row { children, .. } => children,
        Component::Col { children, .. } => children,
        Component::Grid { children, .. } => children,
        Component::Tabs { tabs, .. } => {
            return 1 + tabs
                .iter()
                .map(|t| t.children.iter().map(count_components).sum::<usize>())
                .sum::<usize>();
        }
        _ => &[],
    };
    1 + children.iter().map(count_components).sum::<usize>()
}
