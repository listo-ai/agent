#![allow(clippy::unwrap_used, clippy::panic)]
//! Stage 1 acceptance tests: the substrate works.
//!
//! Kinds register, bound nodes refuse wrong placement, free nodes drop
//! anywhere, deletes cascade correctly, events fire.

use std::sync::Arc;

use graph::{seed, GraphError, GraphEvent, GraphStore, KindRegistry, SlotRef, VecSink};
use serde_json::json;
use spi::{KindId, NodePath};

fn fresh() -> (Arc<VecSink>, GraphStore) {
    let sink = Arc::new(VecSink::new());
    let kinds = KindRegistry::new();
    seed::register_builtins(&kinds);
    let store = GraphStore::new(kinds, sink.clone());
    store
        .create_root(KindId::new("acme.core.station"))
        .expect("root station must create");
    (sink, store)
}

#[test]
fn free_node_drops_anywhere() {
    let (_sink, store) = fresh();
    store
        .create_child(
            &NodePath::root(),
            KindId::new("acme.compute.math.add"),
            "sum",
        )
        .expect("free compute node can live under the station");
}

#[test]
fn bound_node_rejects_wrong_parent() {
    let (_sink, store) = fresh();
    let err = store
        .create_child(
            &NodePath::root(),
            KindId::new("acme.driver.demo.point"),
            "p1",
        )
        .expect_err("point cannot live directly under station");
    assert!(matches!(err, GraphError::PlacementRejected { .. }));
}

#[test]
fn bound_node_accepts_right_parent() {
    let (sink, store) = fresh();
    store
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "demo")
        .unwrap();
    let demo = NodePath::root().child("demo");
    store
        .create_child(&demo, KindId::new("acme.driver.demo.device"), "d1")
        .unwrap();
    let device = demo.child("d1");
    store
        .create_child(&device, KindId::new("acme.driver.demo.point"), "p1")
        .unwrap();
    // Creation events were emitted in order.
    let events: Vec<&'static str> = sink
        .snapshot()
        .iter()
        .map(|e| match e {
            GraphEvent::NodeCreated { .. } => "created",
            _ => "other",
        })
        .collect();
    // Root + 3 children = 4 creation events.
    assert_eq!(events, vec!["created"; 4]);
}

#[test]
fn cascade_strict_removes_subtree() {
    let (sink, store) = fresh();
    store
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "demo")
        .unwrap();
    let demo = NodePath::root().child("demo");
    store
        .create_child(&demo, KindId::new("acme.driver.demo.device"), "d1")
        .unwrap();
    store
        .create_child(
            &demo.child("d1"),
            KindId::new("acme.driver.demo.point"),
            "p1",
        )
        .unwrap();

    assert_eq!(store.len(), 4);
    sink.take(); // drop creation events

    store.delete(&demo).expect("demo subtree is strict-cascade");
    assert_eq!(store.len(), 1, "only root remains");

    // 3 node-removed events in post-order: point, device, driver.
    let removed: Vec<_> = sink
        .snapshot()
        .iter()
        .filter_map(|e| match e {
            GraphEvent::NodeRemoved { path, .. } => Some(path.as_str().to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        removed,
        vec!["/demo/d1/p1", "/demo/d1", "/demo"],
        "children removed before parents"
    );
}

#[test]
fn cascade_deny_refuses_non_empty_delete() {
    let (_sink, store) = fresh();
    // Station has cascade=Deny + a child — refuses delete.
    store
        .create_child(&NodePath::root(), KindId::new("acme.core.folder"), "site")
        .unwrap();
    let err = store
        .delete(&NodePath::root())
        .expect_err("station denies when non-empty");
    assert!(matches!(err, GraphError::CascadeDenied { .. }));
}

#[test]
fn deleting_linked_node_emits_link_broken() {
    let (sink, store) = fresh();
    // Build a small tree with two driver subtrees, wire their two points.
    store
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "a")
        .unwrap();
    store
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "b")
        .unwrap();
    let a = NodePath::root().child("a");
    let b = NodePath::root().child("b");
    store
        .create_child(&a, KindId::new("acme.driver.demo.device"), "d")
        .unwrap();
    store
        .create_child(&b, KindId::new("acme.driver.demo.device"), "d")
        .unwrap();
    let pa_id = store
        .create_child(&a.child("d"), KindId::new("acme.driver.demo.point"), "p")
        .unwrap();
    let pb_id = store
        .create_child(&b.child("d"), KindId::new("acme.driver.demo.point"), "p")
        .unwrap();

    store
        .add_link(SlotRef::new(pa_id, "value"), SlotRef::new(pb_id, "value"))
        .unwrap();
    sink.take();

    store.delete(&a).expect("a subtree cascades");

    let snap = sink.snapshot();
    let broken: Vec<_> = snap
        .iter()
        .filter(|e| matches!(e, GraphEvent::LinkBroken { .. }))
        .collect();
    assert_eq!(broken.len(), 1, "exactly one link broke");
    let removed: Vec<_> = snap
        .iter()
        .filter(|e| matches!(e, GraphEvent::LinkRemoved { .. }))
        .collect();
    assert_eq!(removed.len(), 1);
}

#[test]
fn slot_write_emits_change_event() {
    let (sink, store) = fresh();
    store
        .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "demo")
        .unwrap();
    store
        .create_child(
            &NodePath::root().child("demo"),
            KindId::new("acme.driver.demo.device"),
            "d1",
        )
        .unwrap();
    let point_path = NodePath::root().child("demo").child("d1").child("p1");
    store
        .create_child(
            &NodePath::root().child("demo").child("d1"),
            KindId::new("acme.driver.demo.point"),
            "p1",
        )
        .unwrap();
    sink.take();

    let gen = store.write_slot(&point_path, "value", json!(42)).unwrap();
    assert_eq!(gen, 1);
    let changed: Vec<_> = sink
        .snapshot()
        .into_iter()
        .filter_map(|e| match e {
            GraphEvent::SlotChanged {
                value, generation, ..
            } => Some((value, generation)),
            _ => None,
        })
        .collect();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].0, json!(42));
    assert_eq!(changed[0].1, 1);
}

#[test]
fn unknown_kind_is_rejected() {
    let (_sink, store) = fresh();
    let err = store
        .create_child(
            &NodePath::root(),
            KindId::new("not.registered.anywhere"),
            "x",
        )
        .unwrap_err();
    assert!(matches!(err, GraphError::UnknownKind(_)));
}

#[test]
fn name_collision_is_rejected() {
    let (_sink, store) = fresh();
    store
        .create_child(&NodePath::root(), KindId::new("acme.core.folder"), "site")
        .unwrap();
    let err = store
        .create_child(&NodePath::root(), KindId::new("acme.core.folder"), "site")
        .unwrap_err();
    assert!(matches!(err, GraphError::NameCollision { .. }));
}
