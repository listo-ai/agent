#![allow(clippy::unwrap_used, clippy::panic)]
//! Falsification tests — the three acceptance scenarios from
//! [`docs/design/SDUI.md`] § "Acceptance criteria".
//!
//! Each scenario ships as:
//!
//! 1. A fixture `layout` — the JSON a block author would write once.
//! 2. A handler registration for each interactive action.
//! 3. A resolve + action round-trip that proves the page renders and
//!    its buttons fire the right handler.
//!
//! The point is *negative*: there are zero BACnet-, GitHub-, or
//! scope-specific keywords in the IR or React renderer. If any of
//! these tests regress, a new domain-concept has leaked into the
//! platform — fix at the source, don't paper over here.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use dashboard_runtime::{InMemoryReader, NodeSnapshot};
use dashboard_transport::{
    action::ActionContext, resolve, ActionResponse, DashboardState, HandlerRegistry, ToastIntent,
};
use serde_json::{json, Value};
use spi::NodeId;
use ui_ir::ComponentTree;

fn state_with(layout: Value, handlers: HandlerRegistry) -> (DashboardState, NodeId) {
    let page = NodeId::default();
    let reader = InMemoryReader::new().with(
        NodeSnapshot::new(page, "ui.page").with_slot("layout", layout),
    );
    let state = DashboardState::new(Arc::new(reader) as Arc<_>)
        .with_handlers(Arc::new(handlers));
    (state, page)
}

async fn resolve_ok(state: DashboardState, page_ref: NodeId) -> ComponentTree {
    let req: resolve::ResolveRequest = serde_json::from_value(json!({
        "page_ref": page_ref
    }))
    .unwrap();
    match resolve::handler(State(state), Json(req)).await.unwrap().0 {
        resolve::ResolveResponse::Ok { render, .. } => render,
        resolve::ResolveResponse::DryRun { errors } => {
            panic!("expected Ok, got DryRun with errors: {errors:?}")
        }
    }
}

async fn dispatch_action(state: DashboardState, handler: &str, args: Value) -> ActionResponse {
    let fut = state
        .handlers
        .dispatch(handler, args, ActionContext::default())
        .unwrap_or_else(|| panic!("handler `{handler}` not registered"));
    fut.await.unwrap()
}

/// Count every component matching a predicate, recursively.
fn count_where(tree: &ComponentTree, mut pred: impl FnMut(&ui_ir::Component) -> bool) -> usize {
    let mut n = 0;
    fn walk(
        c: &ui_ir::Component,
        pred: &mut impl FnMut(&ui_ir::Component) -> bool,
        n: &mut usize,
    ) {
        if pred(c) {
            *n += 1;
        }
        match c {
            ui_ir::Component::Page { children, .. }
            | ui_ir::Component::Row { children, .. }
            | ui_ir::Component::Col { children, .. }
            | ui_ir::Component::Grid { children, .. }
            | ui_ir::Component::Drawer { children, .. } => {
                for c in children {
                    walk(c, pred, n);
                }
            }
            _ => {}
        }
    }
    walk(&tree.root, &mut pred, &mut n);
    n
}

// =========================================================================
// UC1 — BACnet discovery
// =========================================================================
//
// "A React app with zero BACnet-specific code renders a working
// BACnet discovery page — list devices, click scan, add a discovered
// device to the graph, see it live-update — driven entirely by IR the
// backend emits."
//
// The falsification: a block ships a `ui.page.layout` with a
// heading, a `scan` button (action → `bacnet.scan`), and a table of
// nodes matching `kind==sys.driver.demo.device`. Clicking the button
// fires a handler that returns a toast. The renderer sees only IR.
#[tokio::test]
async fn uc1_bacnet_discovery_page() {
    let layout = json!({
        "ir_version": 1,
        "root": {
            "type": "page", "id": "root", "title": "Discovery",
            "children": [
                {"type": "heading", "id": "h", "content": "Devices", "level": 1},
                {"type": "button",  "id": "scan", "label": "Scan",
                 "action": { "handler": "bacnet.scan", "args": { "network": "net-1" } }},
                {"type": "table",   "id": "devices",
                 "source": { "query": "kind==sys.driver.demo.device", "subscribe": true },
                 "columns": [
                    {"title":"Path","field":"path"},
                    {"title":"Kind","field":"kind"}
                 ]}
            ]
        }
    });

    let handlers = HandlerRegistry::new();
    handlers.register("bacnet.scan", |_args: Value, _ctx: ActionContext| {
        Box::pin(async {
            Ok(ActionResponse::Toast {
                intent: ToastIntent::Ok,
                message: "scan started".into(),
            })
        })
    });
    let (state, page) = state_with(layout, handlers);

    // 1. Resolve: tree comes back with a button + a table.
    let tree = resolve_ok(state.clone(), page).await;
    assert_eq!(
        count_where(&tree, |c| matches!(c, ui_ir::Component::Button { .. })),
        1,
    );
    assert_eq!(
        count_where(&tree, |c| matches!(c, ui_ir::Component::Table { .. })),
        1,
    );

    // 2. Action: button fires the registered handler.
    let resp = dispatch_action(state, "bacnet.scan", json!({"network": "net-1"})).await;
    match resp {
        ActionResponse::Toast { message, .. } => assert_eq!(message, "scan started"),
        other => panic!("expected toast, got {other:?}"),
    }
}

// =========================================================================
// UC2 — PR review card
// =========================================================================
//
// "The same bundle, zero GitHub-specific code, renders a per-user PR
// review card … with a `diff` component showing the PR changes,
// inline `button`s for approve / request-changes / reject, and the
// action round-trips to the GitHub extension's handlers."
#[tokio::test]
async fn uc2_pr_review_card() {
    let layout = json!({
        "ir_version": 1,
        "root": {
            "type": "page", "id": "root", "title": "PR #42",
            "children": [
                {"type": "diff", "id": "d",
                 "old_text": "fn main() {}",
                 "new_text": "fn main() { println!(\"hi\"); }",
                 "language": "rust"},
                {"type": "row", "id": "actions", "children": [
                    {"type": "button", "id": "approve",   "label": "Approve",
                     "action": {"handler": "github.approve",        "args": {"pr": 42}}},
                    {"type": "button", "id": "request",   "label": "Request changes",
                     "action": {"handler": "github.request_changes","args": {"pr": 42}}},
                    {"type": "button", "id": "reject",    "label": "Reject",
                     "action": {"handler": "github.reject",         "args": {"pr": 42}}}
                ]}
            ]
        }
    });

    let handlers = HandlerRegistry::new();
    for name in ["github.approve", "github.request_changes", "github.reject"] {
        let msg = name.to_string();
        handlers.register(name, move |_args: Value, _ctx: ActionContext| {
            let msg = msg.clone();
            Box::pin(async move {
                Ok(ActionResponse::Toast {
                    intent: ToastIntent::Ok,
                    message: msg,
                })
            })
        });
    }
    let (state, page) = state_with(layout, handlers);

    let tree = resolve_ok(state.clone(), page).await;
    assert_eq!(
        count_where(&tree, |c| matches!(c, ui_ir::Component::Diff { .. })),
        1,
    );
    assert_eq!(
        count_where(&tree, |c| matches!(c, ui_ir::Component::Button { .. })),
        3,
    );

    // Each button reaches its handler.
    for h in ["github.approve", "github.request_changes", "github.reject"] {
        let resp = dispatch_action(state.clone(), h, json!({"pr": 42})).await;
        match resp {
            ActionResponse::Toast { message, .. } => assert_eq!(message, h),
            other => panic!("expected toast for {h}, got {other:?}"),
        }
    }
}

// =========================================================================
// UC3 — scope plan board
// =========================================================================
//
// "The same bundle, zero scope-specific code, renders UC3's scope-plan
// daily board — rows of scopes with state badges, per-row approve /
// reject buttons, live updates as the flow advances stages via
// subscriptions."
#[tokio::test]
async fn uc3_scope_plan_board() {
    let layout = json!({
        "ir_version": 1,
        "root": {
            "type": "page", "id": "root", "title": "Scope plan",
            "children": [
                {"type": "table", "id": "scopes",
                 "source": {"query": "kind==sys.core.folder", "subscribe": true},
                 "columns": [
                    {"title":"Name","field":"path"},
                    {"title":"Status","field":"lifecycle"}
                 ]},
                {"type": "row", "id": "controls", "children": [
                    {"type":"button","id":"approve","label":"Approve selected",
                     "action":{"handler":"scope.approve","args":{}},
                     "intent":"ok"},
                    {"type":"button","id":"reject","label":"Reject selected",
                     "action":{"handler":"scope.reject","args":{}},
                     "intent":"danger"}
                ]}
            ]
        }
    });

    let handlers = HandlerRegistry::new();
    handlers.register("scope.approve", |_args: Value, _ctx: ActionContext| {
        Box::pin(async {
            Ok(ActionResponse::Toast {
                intent: ToastIntent::Ok,
                message: "approved".into(),
            })
        })
    });
    handlers.register("scope.reject", |_args: Value, _ctx: ActionContext| {
        Box::pin(async {
            Ok(ActionResponse::Toast {
                intent: ToastIntent::Danger,
                message: "rejected".into(),
            })
        })
    });
    let (state, page) = state_with(layout, handlers);

    let tree = resolve_ok(state.clone(), page).await;
    assert_eq!(
        count_where(&tree, |c| matches!(c, ui_ir::Component::Table { .. })),
        1,
    );
    assert_eq!(
        count_where(&tree, |c| matches!(c, ui_ir::Component::Button { .. })),
        2,
    );

    let resp = dispatch_action(state.clone(), "scope.approve", json!({})).await;
    assert!(matches!(resp, ActionResponse::Toast { .. }));
    let resp = dispatch_action(state, "scope.reject", json!({})).await;
    assert!(matches!(resp, ActionResponse::Toast { .. }));
}
