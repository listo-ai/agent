#![allow(clippy::unwrap_used, clippy::panic)]
//! Acceptance tests for structural node mutation (`rename_node`,
//! `move_node`, `patch_node`).
//!
//! See `docs/design/NODE-MUTATION.md` for the contract these enforce.

use std::sync::Arc;

use graph::{seed, GraphError, GraphEvent, GraphStore, KindRegistry, NodePatch, VecSink};
use spi::{KindId, NodePath};

fn fresh() -> (Arc<VecSink>, GraphStore) {
    let sink = Arc::new(VecSink::new());
    let kinds = KindRegistry::new();
    seed::register_builtins(&kinds);
    let store = GraphStore::new(kinds, sink.clone());
    store
        .create_root(KindId::new("sys.core.station"))
        .expect("root station must create");
    (sink, store)
}

fn mk_free(store: &GraphStore, parent: &NodePath, name: &str) {
    store
        .create_child(parent, KindId::new("sys.compute.math.add"), name)
        .expect("free compute node creates");
}

#[test]
fn rename_renames_leaf_and_emits_event() {
    let (sink, store) = fresh();
    mk_free(&store, &NodePath::root(), "n1");

    let new_path = store
        .rename_node(&"/n1".parse().unwrap(), "n2")
        .expect("rename");
    assert_eq!(new_path.as_str(), "/n2");
    assert!(store.get(&"/n2".parse().unwrap()).is_some());
    assert!(store.get(&"/n1".parse().unwrap()).is_none());

    let events = sink.snapshot();
    assert!(events.iter().any(|e| matches!(
        e,
        GraphEvent::NodeRenamed { new_path, .. } if new_path.as_str() == "/n2"
    )));
}

#[test]
fn rename_repaths_whole_subtree() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "parent");
    mk_free(&store, &"/parent".parse().unwrap(), "child");
    mk_free(&store, &"/parent/child".parse().unwrap(), "leaf");

    store
        .rename_node(&"/parent".parse().unwrap(), "renamed")
        .expect("rename");

    assert!(store.get(&"/renamed".parse().unwrap()).is_some());
    assert!(store.get(&"/renamed/child".parse().unwrap()).is_some());
    assert!(store.get(&"/renamed/child/leaf".parse().unwrap()).is_some());
    assert!(store.get(&"/parent".parse().unwrap()).is_none());
    assert!(store.get(&"/parent/child".parse().unwrap()).is_none());
}

#[test]
fn rename_rejects_sibling_collision() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "a");
    mk_free(&store, &NodePath::root(), "b");

    let err = store
        .rename_node(&"/a".parse().unwrap(), "b")
        .expect_err("collision");
    assert!(matches!(err, GraphError::NameCollision { .. }));
}

#[test]
fn rename_rejects_invalid_name() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "a");

    assert!(matches!(
        store.rename_node(&"/a".parse().unwrap(), ""),
        Err(GraphError::InvalidNodeName(_))
    ));
    assert!(matches!(
        store.rename_node(&"/a".parse().unwrap(), "has/slash"),
        Err(GraphError::InvalidNodeName(_))
    ));
}

#[test]
fn rename_rejects_root() {
    let (_, store) = fresh();
    assert!(matches!(
        store.rename_node(&NodePath::root(), "whatever"),
        Err(GraphError::InvalidNodeName(_))
    ));
}

#[test]
fn move_relocates_under_new_parent() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "src");
    mk_free(&store, &NodePath::root(), "dst");

    let new_path = store
        .move_node(&"/src".parse().unwrap(), &"/dst".parse().unwrap())
        .expect("move");
    assert_eq!(new_path.as_str(), "/dst/src");
    assert!(store.get(&"/dst/src".parse().unwrap()).is_some());
    assert!(store.get(&"/src".parse().unwrap()).is_none());
}

#[test]
fn move_takes_subtree_along() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "src");
    mk_free(&store, &"/src".parse().unwrap(), "child");
    mk_free(&store, &NodePath::root(), "dst");

    store
        .move_node(&"/src".parse().unwrap(), &"/dst".parse().unwrap())
        .unwrap();
    assert!(store.get(&"/dst/src/child".parse().unwrap()).is_some());
}

#[test]
fn patch_empty_rejected() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "a");
    assert!(matches!(
        store.patch_node(&"/a".parse().unwrap(), NodePatch::default()),
        Err(GraphError::InvalidNodeName(_))
    ));
}

#[test]
fn patch_name_only_equals_rename() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "a");
    let new_path = store
        .patch_node(
            &"/a".parse().unwrap(),
            NodePatch {
                name: Some("b".into()),
                parent: None,
            },
        )
        .unwrap();
    assert_eq!(new_path.as_str(), "/b");
}

#[test]
fn patch_parent_only_equals_move() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "a");
    mk_free(&store, &NodePath::root(), "dst");
    let new_path = store
        .patch_node(
            &"/a".parse().unwrap(),
            NodePatch {
                name: None,
                parent: Some("/dst".parse().unwrap()),
            },
        )
        .unwrap();
    assert_eq!(new_path.as_str(), "/dst/a");
}

#[test]
fn patch_name_and_parent_applied_together() {
    let (_, store) = fresh();
    mk_free(&store, &NodePath::root(), "a");
    mk_free(&store, &NodePath::root(), "dst");
    let new_path = store
        .patch_node(
            &"/a".parse().unwrap(),
            NodePatch {
                name: Some("renamed".into()),
                parent: Some("/dst".parse().unwrap()),
            },
        )
        .unwrap();
    assert_eq!(new_path.as_str(), "/dst/renamed");
}
