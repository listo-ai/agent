//! `POST /ui/resolve` — the binding resolver transport.
//!
//! Body:
//!
//! ```json
//! {
//!   "page_ref": "<uuid>",
//!   "stack":    ["<nav-uuid>", ...],
//!   "page_state": { ... },
//!   "dry_run":  false
//! }
//! ```
//!
//! With `dry_run: true` the handler validates inputs and the
//! template parameter contract and returns `{ "errors": [...] }`
//! without producing a render tree. Otherwise it returns a resolved
//! [`ComponentTree`] plus `meta`. See DASHBOARD.md § M3-M5 and
//! SDUI.md § S1.
//!
//! Per-widget ACL redaction is applied (§ "ACL policy"): if any node
//! touched during a widget's binding evaluation is unreadable by the
//! caller, the widget becomes a [`Component::Forbidden`] stub and
//! one audit event fires per redaction. Missing bound nodes surface as
//! [`Component::Dangling`]. Unknown widget types are a dry-run
//! validation issue; in real resolve they also emit a forbidden stub
//! tagged `unknown_widget_type`.
//!
//! Subscription-plan emission is deferred to M5.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use dashboard_nodes::{validate_bound_args, ContractError};
use dashboard_runtime::{
    hash_page_state, Binding, BindingError, CacheKeyInputs, ContextStack, EvalContext, NodeReader,
    NodeSnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::{KindId, NodeId};
use ui_ir::{Component, ComponentTree};

use crate::acl::{AclCheck, AclSubject};
use crate::audit::AuditEvent;
use crate::error::TransportError;
use crate::limits;
use crate::state::DashboardState;

const PAGE_KIND: &str = "ui.page";
const TEMPLATE_KIND: &str = "ui.template";
const WIDGET_KIND: &str = "ui.widget";

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub page_ref: NodeId,
    #[serde(default)]
    pub stack: Vec<NodeId>,
    #[serde(default = "empty_object")]
    pub page_state: JsonValue,
    #[serde(default)]
    pub dry_run: bool,
    /// Optional — the auth subject identifier. Threaded into the cache
    /// key + ACL check + audit events. Real auth plumbing lands with
    /// the AuthContext integration.
    #[serde(default)]
    pub auth_subject: Option<String>,
    /// Optional user claims available as `$user.*` in bindings.
    #[serde(default)]
    pub user_claims: HashMap<String, JsonValue>,
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

/// Subscription plan for a single widget — mechanically derived from
/// the slots its bindings touch during evaluation. Subjects follow the
/// `node.<id>.slot.<name>` convention the messaging crate expects.
///
/// ACL-denied subjects are dropped before emission; a widget whose
/// bindings touch any denied node is already redacted as a
/// `ui.widget.forbidden` stub and gets no subscription plan at all.
#[derive(Debug, Serialize)]
pub struct SubscriptionPlan {
    pub widget_id: NodeId,
    pub subjects: Vec<String>,
    pub debounce_ms: u32,
}

/// Per-widget debounce default. The client can override if it cares;
/// the value feeds the cache-key indirectly via subscription churn.
const DEFAULT_DEBOUNCE_MS: u32 = 250;

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

    let reader = &*state.reader;
    let page = reader
        .get(&req.page_ref)
        .ok_or(TransportError::PageNotFound(req.page_ref))?;
    require_kind(&page, PAGE_KIND)?;

    // SDUI fast-path: if the page has a `layout` slot, parse it directly as a
    // ComponentTree and return it, bypassing the legacy widget resolver.
    if let Some(layout_val) = page.slots.get("layout") {
        if !layout_val.is_null() {
            return sdui_layout_path(layout_val.clone(), &req, &page, &*state.reader);
        }
    }

    let stack = ContextStack::build(reader, &req.stack, limits::MAX_NAV_DEPTH)?;
    let widgets = collect_widgets(reader, &page.id)?;
    let contract_errors = run_contract_validation(reader, &page)?;

    if req.dry_run {
        return Ok(Json(dry_run(
            &req,
            &state,
            &page,
            &widgets,
            &stack,
            contract_errors,
        )));
    }

    if let Some(first) = contract_errors.into_iter().next() {
        return Err(first.into());
    }

    let mut rendered: Vec<Component> = Vec::with_capacity(widgets.len());
    let mut subscriptions: Vec<SubscriptionPlan> = Vec::with_capacity(widgets.len());
    for w in &widgets {
        let (r, subs) = resolve_widget(reader, &stack, w, &req, &state);
        if let Some(s) = subs {
            subscriptions.push(s);
        }
        rendered.push(r);
    }

    enforce_subscription_cap(&subscriptions)?;

    let forbidden_count = rendered
        .iter()
        .filter(|w| matches!(w, Component::Forbidden { .. }))
        .count();
    let dangling_count = rendered
        .iter()
        .filter(|w| matches!(w, Component::Dangling { .. }))
        .count();

    let cache_key = build_cache_key(&req, &state, &page, &widgets, &stack);
    let meta = ResolveMeta {
        cache_key,
        widget_count: rendered.len(),
        forbidden_count,
        dangling_count,
        stack_shadowed: stack.shadowed().to_vec(),
    };
    let title = page
        .slots
        .get("title")
        .and_then(|v| v.as_str())
        .map(String::from);
    let render = ComponentTree::new(Component::Page {
        id: page.id.0.to_string(),
        title,
        children: rendered,
    });

    enforce_render_tree_size(&render)?;
    Ok(Json(ResolveResponse::Ok {
        render,
        subscriptions,
        meta,
    }))
}

/// SDUI layout fast-path — parses the `layout` slot value directly as a
/// `ComponentTree`, bypassing the legacy widget resolver.
///
/// Returned for any `ui.page` whose `layout` slot is non-null.  The dry-run
/// variant validates the JSON is parseable; the normal variant returns the tree
/// verbatim.  Subscriptions and cache-key are stubbed for now (S5 will wire
/// proper subscription derivation for SDUI trees).
fn sdui_layout_path(
    layout_val: serde_json::Value,
    req: &ResolveRequest,
    page: &NodeSnapshot,
    reader: &(dyn NodeReader + Send + Sync),
) -> Result<Json<ResolveResponse>, TransportError> {
    let render: ComponentTree = serde_json::from_value(layout_val.clone()).map_err(|e| {
        TransportError::MalformedPage(page.id.clone(), format!("layout is not a valid ComponentTree: {e}"))
    })?;

    if req.dry_run {
        return Ok(Json(ResolveResponse::DryRun { errors: vec![] }));
    }

    enforce_render_tree_size(&render)?;

    let subscriptions = crate::render::derive_subscriptions_for_layout(&layout_val, reader);

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

/// Recursively count all nodes in a component tree.
fn count_components(c: &Component) -> usize {
    let children: &[Component] = match c {
        Component::Page { children, .. } => children,
        Component::Row { children, .. } => children,
        Component::Col { children, .. } => children,
        Component::Grid { children, .. } => children,
        Component::Tabs { tabs, .. } => {
            return 1 + tabs.iter().map(|t| t.children.iter().map(count_components).sum::<usize>()).sum::<usize>();
        }
        _ => &[],
    };
    1 + children.iter().map(count_components).sum::<usize>()
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

fn dry_run(
    req: &ResolveRequest,
    state: &DashboardState,
    page: &NodeSnapshot,
    widgets: &[NodeSnapshot],
    stack: &ContextStack,
    contract_errors: Vec<ContractError>,
) -> ResolveResponse {
    let mut issues: Vec<ResolveIssue> = contract_errors
        .into_iter()
        .map(|e| ResolveIssue {
            location: format!("page/{}/bound_args", page.id),
            message: e.to_string(),
        })
        .collect();
    for w in widgets {
        if let Ok(wtype) = widget_type(w) {
            if !state.widgets.contains(&wtype) {
                issues.push(ResolveIssue {
                    location: format!("widget/{}/widget_type", w.id),
                    message: format!("unknown widget type `{wtype}`"),
                });
            }
        }
        let bindings = match widget_bindings(w) {
            Ok(b) => b,
            Err(e) => {
                issues.push(ResolveIssue {
                    location: format!("widget/{}/bindings", w.id),
                    message: e.to_string(),
                });
                continue;
            }
        };
        for (name, expr) in bindings {
            if let Err(err) = Binding::parse(&expr).and_then(|b| {
                b.evaluate(&EvalContext {
                    reader: &*state.reader,
                    stack,
                    self_id: w.id,
                    user_claims: &req.user_claims,
                    page_state: &req.page_state,
                    access_log: None,
                })
            }) {
                issues.push(ResolveIssue {
                    location: format!("widget/{}/bindings/{name}", w.id),
                    message: err.to_string(),
                });
            }
        }
    }
    ResolveResponse::DryRun { errors: issues }
}

fn resolve_widget<R: NodeReader + ?Sized>(
    reader: &R,
    stack: &ContextStack,
    w: &NodeSnapshot,
    req: &ResolveRequest,
    state: &DashboardState,
) -> (Component, Option<SubscriptionPlan>) {
    let wid = w.id.0.to_string();
    let subject = AclSubject {
        subject: req.auth_subject.as_deref(),
    };

    let wtype = match widget_type(w) {
        Ok(t) => t,
        Err(_) => {
            return (
                Component::Forbidden {
                    id: wid,
                    reason: "malformed_widget".into(),
                },
                None,
            );
        }
    };
    if !state.widgets.contains(&wtype) {
        state.audit.emit(AuditEvent::UnknownWidgetType {
            widget: w.id,
            widget_type: &wtype,
            subject: subject.subject,
        });
        return (
            Component::Forbidden {
                id: wid,
                reason: "unknown_widget_type".into(),
            },
            None,
        );
    }

    let bindings = match widget_bindings(w) {
        Ok(b) => b,
        Err(_) => {
            return (
                Component::Forbidden {
                    id: wid,
                    reason: "malformed_widget".into(),
                },
                None,
            );
        }
    };

    let recorder = RecordingReader::new(reader);
    let access_log: RefCell<Vec<(NodeId, String)>> = RefCell::new(Vec::new());
    let mut values: HashMap<String, JsonValue> = HashMap::new();
    for (name, expr) in bindings {
        let binding = match Binding::parse(&expr) {
            Ok(b) => b,
            Err(_) => {
                return (
                    Component::Forbidden {
                        id: wid,
                        reason: "malformed_binding".into(),
                    },
                    None,
                );
            }
        };
        match binding.evaluate(&EvalContext {
            reader: &recorder,
            stack,
            self_id: w.id,
            user_claims: &req.user_claims,
            page_state: &req.page_state,
            access_log: Some(&access_log),
        }) {
            Ok(v) => {
                values.insert(name, v);
            }
            Err(BindingError::RefNodeMissing(id)) => {
                state.audit.emit(AuditEvent::WidgetDangling {
                    widget: w.id,
                    missing_node: id,
                    subject: subject.subject,
                });
                return (Component::Dangling { id: wid }, None);
            }
            Err(_) => {
                return (
                    Component::Forbidden {
                        id: wid,
                        reason: "malformed_binding".into(),
                    },
                    None,
                );
            }
        }
    }

    let touched = recorder.touched.into_inner();
    let mut denied = false;
    for nid in &touched {
        if !state.acl.can_read(subject, nid) {
            state.audit.emit(AuditEvent::WidgetRedacted {
                widget: w.id,
                bound_node: *nid,
                subject: subject.subject,
            });
            denied = true;
        }
    }
    if denied {
        return (
            Component::Forbidden {
                id: wid,
                reason: "acl".into(),
            },
            None,
        );
    }

    // Build subscription plan. ACL-filter subjects: drop any subject
    // whose node the caller cannot read (defensive — in practice if
    // any node were denied we'd have short-circuited above, but the
    // check keeps the contract explicit).
    let subjects = derive_subjects(access_log.into_inner(), &state.acl, subject);

    // Map the resolved ui.widget to a Text component showing the
    // widget type + values. A richer mapping (per-widget-type IR
    // emission) comes in later milestones. For S1 this preserves the
    // resolved data in the tree.
    (
        Component::Text {
            id: Some(wid),
            content: format!("[{wtype}]"),
            intent: None,
        },
        Some(SubscriptionPlan {
            widget_id: w.id,
            subjects,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
        }),
    )
}

fn derive_subjects(
    log: Vec<(NodeId, String)>,
    acl: &Arc<dyn AclCheck>,
    subject: AclSubject<'_>,
) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for (node, slot) in log {
        if !acl.can_read(subject, &node) {
            continue;
        }
        seen.insert(format!("node.{node}.slot.{slot}"));
    }
    seen.into_iter().collect()
}

/// Reader wrapper that records every `get` call so the caller can ACL
/// every node visited during a widget's binding evaluation.
struct RecordingReader<'a, R: NodeReader + ?Sized> {
    inner: &'a R,
    touched: RefCell<HashSet<NodeId>>,
}

impl<'a, R: NodeReader + ?Sized> RecordingReader<'a, R> {
    fn new(inner: &'a R) -> Self {
        Self {
            inner,
            touched: RefCell::new(HashSet::new()),
        }
    }
}

impl<R: NodeReader + ?Sized> NodeReader for RecordingReader<'_, R> {
    fn get(&self, id: &NodeId) -> Option<NodeSnapshot> {
        self.touched.borrow_mut().insert(*id);
        self.inner.get(id)
    }
    fn children(&self, parent: &NodeId) -> Vec<NodeId> {
        self.inner.children(parent)
    }
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

fn enforce_render_tree_size(tree: &ComponentTree) -> Result<(), TransportError> {
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

fn collect_widgets<R: NodeReader + ?Sized>(
    reader: &R,
    page_id: &NodeId,
) -> Result<Vec<NodeSnapshot>, TransportError> {
    let mut out: Vec<NodeSnapshot> = Vec::new();
    for cid in reader.children(page_id) {
        if let Some(snap) = reader.get(&cid) {
            if snap.kind == KindId::new(WIDGET_KIND) {
                out.push(snap);
            }
        }
    }
    if out.len() > limits::MAX_WIDGETS_PER_PAGE {
        return Err(TransportError::LimitExceeded {
            what: "widgets_per_page",
            value: out.len(),
            max: limits::MAX_WIDGETS_PER_PAGE,
        });
    }
    Ok(out)
}

fn run_contract_validation<R: NodeReader + ?Sized>(
    reader: &R,
    page: &NodeSnapshot,
) -> Result<Vec<ContractError>, TransportError> {
    let tref = match page.slots.get("template_ref") {
        Some(v) if !v.is_null() => v,
        _ => return Ok(Vec::new()),
    };
    let id_str = tref
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TransportError::MalformedPage(page.id, "template_ref.id missing".into()))?;
    let tid: NodeId = id_str
        .parse()
        .map(NodeId)
        .map_err(|e| TransportError::MalformedPage(page.id, format!("template_ref.id: {e}")))?;
    let template = reader.get(&tid).ok_or_else(|| {
        TransportError::MalformedPage(page.id, "template_ref points at missing node".into())
    })?;
    require_kind(&template, TEMPLATE_KIND)?;
    let requires = template
        .slots
        .get("requires")
        .cloned()
        .unwrap_or(JsonValue::Null);
    let bound_args = page
        .slots
        .get("bound_args")
        .cloned()
        .unwrap_or_else(|| JsonValue::Object(Default::default()));
    validate_bound_args(&requires, &bound_args).map_err(TransportError::Contract)
}

fn widget_bindings(w: &NodeSnapshot) -> Result<Vec<(String, String)>, TransportError> {
    let raw = match w.slots.get("bindings") {
        Some(v) => v,
        None => return Ok(Vec::new()),
    };
    let obj = raw.as_object().ok_or_else(|| {
        TransportError::MalformedWidget(w.id, "bindings must be an object".into())
    })?;
    let mut out = Vec::with_capacity(obj.len());
    for (k, v) in obj {
        let s = v.as_str().ok_or_else(|| {
            TransportError::MalformedWidget(w.id, format!("binding `{k}` not a string"))
        })?;
        out.push((k.clone(), s.to_string()));
    }
    Ok(out)
}

fn widget_type(w: &NodeSnapshot) -> Result<String, TransportError> {
    w.slots
        .get("widget_type")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| TransportError::MalformedWidget(w.id, "widget_type missing".into()))
}

fn build_cache_key(
    req: &ResolveRequest,
    state: &DashboardState,
    page: &NodeSnapshot,
    widgets: &[NodeSnapshot],
    stack: &ContextStack,
) -> u64 {
    let widget_versions: Vec<(NodeId, u64)> = widgets.iter().map(|w| (w.id, w.version)).collect();
    let bound_versions: Vec<(NodeId, u64)> = Vec::new();
    let inputs = CacheKeyInputs {
        page_ref: page.id,
        page_node_version: page.version,
        template_node_version: None,
        widget_node_versions: &widget_versions,
        bound_node_versions: &bound_versions,
        auth_subject: req.auth_subject.as_deref().unwrap_or(""),
        auth_role_epoch: 0,
        stack,
        page_state_hash: hash_page_state(&req.page_state),
        widget_registry_version: state.widgets.version(),
    };
    inputs.derive().0
}
