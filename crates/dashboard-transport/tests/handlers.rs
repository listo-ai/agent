//! Integration tests for /ui/nav and /ui/resolve.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use dashboard_runtime::{InMemoryReader, NodeReader, NodeSnapshot};
use dashboard_transport::acl::{AclCheck, DenyNodes};
use dashboard_transport::audit::{OwnedAuditEvent, RecordingAudit};
use dashboard_transport::nav::{NavNode, NavQuery};
use dashboard_transport::resolve::{ResolveRequest, ResolveResponse};
use dashboard_transport::{DashboardState, TransportError};
use serde_json::{json, Value as JsonValue};
use spi::NodeId;
use ui_ir::Component;

const CARD: &str = "sys.card";

fn page(id: NodeId, title: &str) -> NodeSnapshot {
    NodeSnapshot::new(id, "ui.page").with_slot("title", json!(title))
}

fn widget(id: NodeId, wtype: &str, bindings: JsonValue) -> NodeSnapshot {
    NodeSnapshot::new(id, "ui.widget")
        .with_slot("widget_type", json!(wtype))
        .with_slot("bindings", bindings)
}

fn nav(id: NodeId, title: &str, alias: Option<&str>, frame: Option<NodeId>) -> NodeSnapshot {
    let mut s = NodeSnapshot::new(id, "ui.nav").with_slot("title", json!(title));
    if let Some(a) = alias {
        s = s.with_slot("frame_alias", json!(a));
    }
    if let Some(f) = frame {
        s = s.with_slot("frame_ref", json!({ "id": f.0.to_string() }));
    }
    s
}

fn state<R: NodeReader + Send + Sync + 'static>(r: R) -> DashboardState {
    let s = DashboardState::new(Arc::new(r));
    s.widgets.register(CARD);
    s
}

fn state_with_acl<R: NodeReader + Send + Sync + 'static>(
    r: R,
    acl: Arc<dyn AclCheck>,
) -> (DashboardState, Arc<RecordingAudit>) {
    let audit = Arc::new(RecordingAudit::new());
    let s = DashboardState::new(Arc::new(r))
        .with_acl(acl)
        .with_audit(audit.clone());
    s.widgets.register(CARD);
    (s, audit)
}

async fn resolve_ok(s: DashboardState, req: ResolveRequest) -> ResolveResponse {
    match dashboard_transport::resolve::handler(State(s), Json(req)).await {
        Ok(Json(r)) => r,
        Err(e) => panic!("resolve failed: {e}"),
    }
}

fn req(page_ref: NodeId, stack: Vec<NodeId>) -> ResolveRequest {
    ResolveRequest {
        page_ref,
        stack,
        page_state: json!({}),
        dry_run: false,
        auth_subject: None,
        user_claims: HashMap::new(),
    }
}

#[tokio::test]
async fn resolve_renders_widget_with_bindings() {
    let page_id = NodeId::new();
    let w_id = NodeId::new();
    let site_id = NodeId::new();
    let nav_id = NodeId::new();

    let reader = InMemoryReader::new()
        .with(page(page_id, "Dashboard"))
        .with(widget(
            w_id,
            CARD,
            json!({
                "label": "$stack.target.name",
                "heading": "$self.widget_type",
            }),
        ))
        .with(NodeSnapshot::new(site_id, "sys.site").with_slot("name", json!("HQ")))
        .with(nav(nav_id, "Site", Some("target"), Some(site_id)))
        .with_child(page_id, w_id);

    let resp = resolve_ok(state(reader), req(page_id, vec![nav_id])).await;
    let (render, meta) = match resp {
        ResolveResponse::Ok {
            render,
            meta,
            subscriptions: _,
        } => (render, meta),
        other => panic!("expected Ok variant, got {other:?}"),
    };
    let children = match &render.root {
        Component::Page { children, .. } => children,
        other => panic!("expected Page root, got {other:?}"),
    };
    assert_eq!(children.len(), 1);
    // In S1, resolved widgets map to Text components showing the
    // widget type. Verify the component is present.
    match &children[0] {
        Component::Text { content, .. } => {
            assert!(content.contains(CARD));
        }
        other => panic!("expected Text, got {other:?}"),
    }
    assert_eq!(meta.widget_count, 1);
    assert_eq!(meta.forbidden_count, 0);
    assert_eq!(meta.dangling_count, 0);
    assert!(meta.cache_key != 0);
}

#[tokio::test]
async fn resolve_emits_subscription_plan_for_each_widget() {
    let page_id = NodeId::new();
    let w_id = NodeId::new();
    let site_id = NodeId::new();
    let nav_id = NodeId::new();

    let reader = InMemoryReader::new()
        .with(page(page_id, "P"))
        .with(widget(w_id, CARD, json!({ "label": "$stack.target.name" })))
        .with(NodeSnapshot::new(site_id, "sys.site").with_slot("name", json!("HQ")))
        .with(nav(nav_id, "Site", Some("target"), Some(site_id)))
        .with_child(page_id, w_id);

    let resp = resolve_ok(state(reader), req(page_id, vec![nav_id])).await;
    let subs = match resp {
        ResolveResponse::Ok { subscriptions, .. } => subscriptions,
        other => panic!("expected Ok, got {other:?}"),
    };
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].widget_id, w_id);
    assert_eq!(subs[0].debounce_ms, 250);
    // The only slot read is site.name.
    let expected = format!("node.{site_id}.slot.name");
    assert_eq!(subs[0].subjects, vec![expected]);
}

#[tokio::test]
async fn resolve_subscription_plan_deduplicates_slot_reads() {
    let page_id = NodeId::new();
    let w_id = NodeId::new();
    let site_id = NodeId::new();
    let nav_id = NodeId::new();

    let reader = InMemoryReader::new()
        .with(page(page_id, "P"))
        .with(widget(
            w_id,
            CARD,
            // Two bindings both read `name` on the same target.
            json!({ "a": "$stack.target.name", "b": "$stack.target.name" }),
        ))
        .with(NodeSnapshot::new(site_id, "sys.site").with_slot("name", json!("HQ")))
        .with(nav(nav_id, "Site", Some("target"), Some(site_id)))
        .with_child(page_id, w_id);

    let resp = resolve_ok(state(reader), req(page_id, vec![nav_id])).await;
    let subs = match resp {
        ResolveResponse::Ok { subscriptions, .. } => subscriptions,
        other => panic!("expected Ok, got {other:?}"),
    };
    assert_eq!(subs[0].subjects.len(), 1);
}

#[tokio::test]
async fn resolve_rejects_unknown_page() {
    let missing = NodeId::new();
    let err = dashboard_transport::resolve::handler(
        State(state(InMemoryReader::new())),
        Json(req(missing, vec![])),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, TransportError::PageNotFound(id) if id == missing));
}

#[tokio::test]
async fn resolve_enforces_page_state_size_cap() {
    let page_id = NodeId::new();
    let reader = InMemoryReader::new().with(page(page_id, "P"));
    let big = "x".repeat(70 * 1024);
    let mut r = req(page_id, vec![]);
    r.page_state = json!({ "blob": big });
    let err = dashboard_transport::resolve::handler(State(state(reader)), Json(r))
        .await
        .unwrap_err();
    match err {
        TransportError::LimitExceeded { what, .. } => assert_eq!(what, "page_state_bytes"),
        other => panic!("expected LimitExceeded, got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_dry_run_collects_binding_errors() {
    let page_id = NodeId::new();
    let w_id = NodeId::new();
    let reader = InMemoryReader::new()
        .with(page(page_id, "P"))
        .with(widget(w_id, CARD, json!({ "x": "$stack.nonexistent" })))
        .with_child(page_id, w_id);

    let mut r = req(page_id, vec![]);
    r.dry_run = true;
    let resp = resolve_ok(state(reader), r).await;
    match resp {
        ResolveResponse::DryRun { errors } => {
            assert!(errors.iter().any(|e| e.message.contains("nonexistent")));
        }
        other => panic!("expected DryRun, got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_dry_run_flags_unknown_widget_type() {
    let page_id = NodeId::new();
    let w_id = NodeId::new();
    let reader = InMemoryReader::new()
        .with(page(page_id, "P"))
        .with(widget(w_id, "unregistered.type", json!({})))
        .with_child(page_id, w_id);

    let mut r = req(page_id, vec![]);
    r.dry_run = true;
    // Use plain state (CARD registered, not `unregistered.type`).
    let resp = resolve_ok(state(reader), r).await;
    match resp {
        ResolveResponse::DryRun { errors } => {
            assert!(errors
                .iter()
                .any(|e| e.message.contains("unknown widget type")));
        }
        other => panic!("expected DryRun, got {other:?}"),
    }
}

#[tokio::test]
async fn unknown_widget_type_becomes_forbidden_placeholder_and_audits() {
    let page_id = NodeId::new();
    let w_id = NodeId::new();
    let reader = InMemoryReader::new()
        .with(page(page_id, "P"))
        .with(widget(w_id, "unregistered.type", json!({})))
        .with_child(page_id, w_id);

    let audit = Arc::new(RecordingAudit::new());
    let s = DashboardState::new(Arc::new(reader)).with_audit(audit.clone());
    // Deliberately don't register the widget type.
    let resp = resolve_ok(s, req(page_id, vec![])).await;
    let (render, meta) = match resp {
        ResolveResponse::Ok {
            render,
            meta,
            subscriptions: _,
        } => (render, meta),
        other => panic!("expected Ok, got {other:?}"),
    };
    assert_eq!(meta.forbidden_count, 1);
    let children = match &render.root {
        Component::Page { children, .. } => children,
        other => panic!("expected Page root, got {other:?}"),
    };
    assert!(matches!(
        &children[0],
        Component::Forbidden { reason, .. } if reason == "unknown_widget_type"
    ));
    let events = audit.events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        OwnedAuditEvent::UnknownWidgetType { widget_type, .. } if widget_type == "unregistered.type"
    ));
}

#[tokio::test]
async fn acl_denied_bound_node_redacts_widget_and_audits() {
    let page_id = NodeId::new();
    let w_id = NodeId::new();
    let site_id = NodeId::new();
    let nav_id = NodeId::new();

    let reader = InMemoryReader::new()
        .with(page(page_id, "P"))
        .with(widget(w_id, CARD, json!({ "label": "$stack.target.name" })))
        .with(NodeSnapshot::new(site_id, "sys.site").with_slot("name", json!("HQ")))
        .with(nav(nav_id, "Site", Some("target"), Some(site_id)))
        .with_child(page_id, w_id);

    let acl: Arc<dyn AclCheck> = Arc::new(DenyNodes::new().deny(site_id));
    let (state, audit) = state_with_acl(reader, acl);

    let mut r = req(page_id, vec![nav_id]);
    r.auth_subject = Some("user:ada".to_string());
    let resp = resolve_ok(state, r).await;
    let (render, meta) = match resp {
        ResolveResponse::Ok {
            render,
            meta,
            subscriptions: _,
        } => (render, meta),
        other => panic!("expected Ok, got {other:?}"),
    };
    assert_eq!(meta.forbidden_count, 1);
    let children = match &render.root {
        Component::Page { children, .. } => children,
        other => panic!("expected Page root, got {other:?}"),
    };
    assert!(matches!(
        &children[0],
        Component::Forbidden { reason, .. } if reason == "acl"
    ));

    let events = audit.events();
    assert!(events.iter().any(|e| matches!(
        e,
        OwnedAuditEvent::WidgetRedacted { bound_node, subject, .. }
            if *bound_node == site_id && subject.as_deref() == Some("user:ada")
    )));
}

#[tokio::test]
async fn acl_page_with_mixed_allowed_denied_widgets() {
    let page_id = NodeId::new();
    let w_ok_id = NodeId::new();
    let w_bad_id = NodeId::new();
    let allowed_site = NodeId::new();
    let denied_site = NodeId::new();

    let reader = InMemoryReader::new()
        .with(page(page_id, "P"))
        .with(widget(
            w_ok_id,
            CARD,
            json!({ "label": "$self.widget_type" }),
        ))
        .with(widget(
            w_bad_id,
            CARD,
            json!({ "label": format!("$user.denied_lookup") }),
        ))
        .with(NodeSnapshot::new(allowed_site, "sys.site").with_slot("name", json!("A")))
        .with(NodeSnapshot::new(denied_site, "sys.site").with_slot("name", json!("B")))
        .with_child(page_id, w_ok_id)
        .with_child(page_id, w_bad_id);

    // Widget-level ACL: only deny the `denied_site`. Bad widget reads
    // self slot only — so it'll actually render. To truly force a
    // redaction on only the "bad" widget, point it at the denied site.
    let reader = reader.with(
        NodeSnapshot::new(w_bad_id, "ui.widget")
            .with_slot("widget_type", json!(CARD))
            .with_slot("bindings", json!({ "label": format!("$stack.s.name") })),
    );
    let nav_id = NodeId::new();
    let reader = reader.with(nav(nav_id, "Site", Some("s"), Some(denied_site)));

    let acl: Arc<dyn AclCheck> = Arc::new(DenyNodes::new().deny(denied_site));
    let (state, _audit) = state_with_acl(reader, acl);

    let resp = resolve_ok(state, req(page_id, vec![nav_id])).await;
    let (render, meta) = match resp {
        ResolveResponse::Ok {
            render,
            meta,
            subscriptions: _,
        } => (render, meta),
        other => panic!("expected Ok, got {other:?}"),
    };
    let children = match &render.root {
        Component::Page { children, .. } => children,
        other => panic!("expected Page root, got {other:?}"),
    };
    assert_eq!(children.len(), 2);
    assert_eq!(meta.forbidden_count, 1);
    // The "ok" widget only reads $self, which the recorder logs, but
    // its own node isn't denied — it should render.
    let rendered_count = children
        .iter()
        .filter(|w| matches!(w, Component::Text { .. }))
        .count();
    assert_eq!(rendered_count, 1);
}

#[tokio::test]
async fn missing_bound_node_becomes_dangling_placeholder() {
    let page_id = NodeId::new();
    let w_id = NodeId::new();
    let ghost = NodeId::new(); // never inserted
    let nav_id = NodeId::new();

    let reader = InMemoryReader::new()
        .with(page(page_id, "P"))
        .with(widget(w_id, CARD, json!({ "label": "$stack.gone.name" })))
        .with(nav(nav_id, "Ghost", Some("gone"), Some(ghost)))
        .with_child(page_id, w_id);

    let audit = Arc::new(RecordingAudit::new());
    let s = DashboardState::new(Arc::new(reader)).with_audit(audit.clone());
    s.widgets.register(CARD);

    let resp = resolve_ok(s, req(page_id, vec![nav_id])).await;
    let (render, meta) = match resp {
        ResolveResponse::Ok {
            render,
            meta,
            subscriptions: _,
        } => (render, meta),
        other => panic!("expected Ok, got {other:?}"),
    };
    assert_eq!(meta.dangling_count, 1);
    let children = match &render.root {
        Component::Page { children, .. } => children,
        other => panic!("expected Page root, got {other:?}"),
    };
    assert!(matches!(&children[0], Component::Dangling { .. }));
    assert!(audit.events().iter().any(|e| matches!(
        e,
        OwnedAuditEvent::WidgetDangling { missing_node, .. } if *missing_node == ghost
    )));
}

#[tokio::test]
async fn widget_registry_version_affects_cache_key() {
    let page_id = NodeId::new();
    let reader = InMemoryReader::new().with(page(page_id, "P"));
    let s1 = state(reader.clone());
    let before = match resolve_ok(s1.clone(), req(page_id, vec![])).await {
        ResolveResponse::Ok { meta, .. } => meta.cache_key,
        other => panic!("{other:?}"),
    };
    s1.widgets.register("another.type");
    let after = match resolve_ok(s1, req(page_id, vec![])).await {
        ResolveResponse::Ok { meta, .. } => meta.cache_key,
        other => panic!("{other:?}"),
    };
    assert_ne!(before, after);
}

#[tokio::test]
async fn nav_returns_nested_subtree() {
    let root = NodeId::new();
    let child = NodeId::new();
    let grand = NodeId::new();
    let reader = InMemoryReader::new()
        .with(nav(root, "Root", None, None))
        .with(nav(child, "Child", None, None))
        .with(nav(grand, "Grand", None, None))
        .with_child(root, child)
        .with_child(child, grand);

    let Json(tree): Json<NavNode> =
        dashboard_transport::nav::handler(State(state(reader)), Query(NavQuery { root }))
            .await
            .unwrap();
    assert_eq!(tree.title.as_deref(), Some("Root"));
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].children.len(), 1);
    assert_eq!(tree.children[0].children[0].title.as_deref(), Some("Grand"));
}

#[tokio::test]
async fn nav_rejects_wrong_kind() {
    let id = NodeId::new();
    let reader = InMemoryReader::new().with(NodeSnapshot::new(id, "ui.page"));
    let err = dashboard_transport::nav::handler(State(state(reader)), Query(NavQuery { root: id }))
        .await
        .unwrap_err();
    assert!(matches!(err, TransportError::KindMismatch { .. }));
}
