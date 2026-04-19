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

    let layout = page
        .slots
        .get("layout")
        .cloned()
        .filter(|v| !v.is_null())
        .ok_or_else(|| {
            TransportError::MalformedPage(page.id, "page has no `layout` slot".into())
        })?;

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

    let render: ComponentTree = serde_json::from_value(layout.clone()).map_err(|e| {
        TransportError::MalformedPage(
            page.id,
            format!("layout is not a valid ComponentTree: {e}"),
        )
    })?;

    enforce_render_tree_size(&render)?;
    enforce_tree_shape(&render)?;

    let subscriptions = crate::render::derive_subscriptions_for_layout(&layout, &*state.reader);
    enforce_subscription_cap(&subscriptions)?;

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
        meta,
    }))
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

    issues_cell.into_inner()
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
        Component::Wizard { .. } => "wizard",
        Component::Drawer { .. } => "drawer",
        Component::Button { .. } => "button",
        Component::Form { .. } => "form",
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
