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
//! render tree plus `meta`. See DASHBOARD.md § M3-M5.
//!
//! Per-widget ACL redaction is applied (§ "ACL policy"): if any node
//! touched during a widget's binding evaluation is unreadable by the
//! caller, the widget becomes a [`RenderedWidget::Forbidden`] stub and
//! one audit event fires per redaction. Missing bound nodes surface as
//! [`RenderedWidget::Dangling`]. Unknown widget types are a dry-run
//! validation issue; in real resolve they also emit a forbidden stub
//! tagged `unknown_widget_type`.
//!
//! Subscription-plan emission is deferred to M5.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

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

use crate::acl::AclSubject;
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
        render: RenderTree,
        meta: ResolveMeta,
    },
    DryRun {
        errors: Vec<ResolveIssue>,
    },
}

#[derive(Debug, Serialize)]
pub struct RenderTree {
    pub page_id: NodeId,
    pub title: Option<String>,
    pub widgets: Vec<RenderedWidget>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
pub enum RenderedWidget {
    #[serde(rename = "ui.widget")]
    Rendered {
        id: NodeId,
        widget_type: String,
        values: HashMap<String, JsonValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        layout_hint: Option<JsonValue>,
    },
    #[serde(rename = "ui.widget.forbidden")]
    Forbidden { id: NodeId, reason: &'static str },
    #[serde(rename = "ui.widget.dangling")]
    Dangling { id: NodeId },
}

impl RenderedWidget {
    pub fn id(&self) -> NodeId {
        match self {
            Self::Rendered { id, .. } | Self::Forbidden { id, .. } | Self::Dangling { id } => *id,
        }
    }
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

    let reader = &*state.reader;
    let page = reader
        .get(&req.page_ref)
        .ok_or(TransportError::PageNotFound(req.page_ref))?;
    require_kind(&page, PAGE_KIND)?;

    let stack = ContextStack::build(reader, &req.stack, limits::MAX_NAV_DEPTH)?;
    let widgets = collect_widgets(reader, &page.id)?;
    let contract_errors = run_contract_validation(reader, &page)?;

    if req.dry_run {
        return Ok(Json(dry_run(&req, &state, &page, &widgets, &stack, contract_errors)));
    }

    if let Some(first) = contract_errors.into_iter().next() {
        return Err(first.into());
    }

    let mut rendered: Vec<RenderedWidget> = Vec::with_capacity(widgets.len());
    for w in &widgets {
        rendered.push(resolve_widget(reader, &stack, w, &req, &state));
    }

    let forbidden_count = rendered
        .iter()
        .filter(|w| matches!(w, RenderedWidget::Forbidden { .. }))
        .count();
    let dangling_count = rendered
        .iter()
        .filter(|w| matches!(w, RenderedWidget::Dangling { .. }))
        .count();

    let cache_key = build_cache_key(&req, &state, &page, &widgets, &stack);
    let meta = ResolveMeta {
        cache_key,
        widget_count: rendered.len(),
        forbidden_count,
        dangling_count,
        stack_shadowed: stack.shadowed().to_vec(),
    };
    let render = RenderTree {
        page_id: page.id,
        title: page.slots.get("title").and_then(|v| v.as_str()).map(String::from),
        widgets: rendered,
    };

    enforce_render_tree_size(&render)?;
    Ok(Json(ResolveResponse::Ok { render, meta }))
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
) -> RenderedWidget {
    let subject = AclSubject {
        subject: req.auth_subject.as_deref(),
    };

    // Unknown widget type → forbidden stub tagged unknown_widget_type.
    // Consistent with the "no dashboard-specific infrastructure beyond
    // resolution and validation" goal: the caller's authoring path will
    // catch it in dry-run; production just refuses to render.
    let wtype = match widget_type(w) {
        Ok(t) => t,
        Err(_) => {
            return RenderedWidget::Forbidden {
                id: w.id,
                reason: "malformed_widget",
            };
        }
    };
    if !state.widgets.contains(&wtype) {
        state.audit.emit(AuditEvent::UnknownWidgetType {
            widget: w.id,
            widget_type: &wtype,
            subject: subject.subject,
        });
        return RenderedWidget::Forbidden {
            id: w.id,
            reason: "unknown_widget_type",
        };
    }

    let bindings = match widget_bindings(w) {
        Ok(b) => b,
        Err(_) => {
            return RenderedWidget::Forbidden {
                id: w.id,
                reason: "malformed_widget",
            };
        }
    };

    let recorder = RecordingReader::new(reader);
    let mut values: HashMap<String, JsonValue> = HashMap::new();
    for (name, expr) in bindings {
        let binding = match Binding::parse(&expr) {
            Ok(b) => b,
            Err(_) => {
                return RenderedWidget::Forbidden {
                    id: w.id,
                    reason: "malformed_binding",
                };
            }
        };
        match binding.evaluate(&EvalContext {
            reader: &recorder,
            stack,
            self_id: w.id,
            user_claims: &req.user_claims,
            page_state: &req.page_state,
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
                return RenderedWidget::Dangling { id: w.id };
            }
            Err(_) => {
                // Any other binding error = malformed. Dry-run surfaces
                // the specific cause; in real resolve we refuse the
                // widget.
                return RenderedWidget::Forbidden {
                    id: w.id,
                    reason: "malformed_binding",
                };
            }
        }
    }

    // ACL check across every node we read. One audit event per denied
    // node — matches DASHBOARD.md "one audit event per redaction".
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
        return RenderedWidget::Forbidden {
            id: w.id,
            reason: "acl",
        };
    }

    RenderedWidget::Rendered {
        id: w.id,
        widget_type: wtype,
        values,
        layout_hint: w.slots.get("layout_hint").cloned().filter(|v| !v.is_null()),
    }
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

fn enforce_render_tree_size(tree: &RenderTree) -> Result<(), TransportError> {
    let len = serde_json::to_vec(tree).map(|b| b.len()).unwrap_or(usize::MAX);
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
    let template = reader
        .get(&tid)
        .ok_or_else(|| TransportError::MalformedPage(page.id, "template_ref points at missing node".into()))?;
    require_kind(&template, TEMPLATE_KIND)?;
    let requires = template.slots.get("requires").cloned().unwrap_or(JsonValue::Null);
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
    let obj = raw
        .as_object()
        .ok_or_else(|| TransportError::MalformedWidget(w.id, "bindings must be an object".into()))?;
    let mut out = Vec::with_capacity(obj.len());
    for (k, v) in obj {
        let s = v
            .as_str()
            .ok_or_else(|| TransportError::MalformedWidget(w.id, format!("binding `{k}` not a string")))?;
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
