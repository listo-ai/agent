#![allow(clippy::unwrap_used, clippy::panic)]
//! Stage 5 acceptance: state survives close + reopen through
//! `GraphStore::with_repo`, write-through is rejected atomically
//! when the backend refuses, and restore rebuilds the in-memory
//! structure in parent-before-child order.

use std::sync::Arc;

use data_repos::{
    GraphRepo, GraphSnapshot, PersistedLink, PersistedNode, PersistedSlot, RepoError,
};
use data_sqlite::SqliteGraphRepo;
use graph::{seed, GraphStore, KindRegistry, NullSink, SlotRef};
use serde_json::json;
use spi::{KindId, NodePath};
use tempfile::NamedTempFile;
use uuid::Uuid;

fn fresh_kinds() -> KindRegistry {
    let kinds = KindRegistry::new();
    seed::register_builtins(&kinds);
    kinds
}

#[test]
fn state_survives_close_and_reopen() {
    let db = NamedTempFile::new().unwrap();

    // First boot: build a small tree, write a slot, add a link.
    {
        let repo = Arc::new(SqliteGraphRepo::open_file(db.path()).unwrap());
        let store = GraphStore::with_repo(fresh_kinds(), Arc::new(NullSink), repo.clone()).unwrap();
        store.create_root(KindId::new("acme.core.station")).unwrap();
        store
            .create_child(&NodePath::root(), KindId::new("acme.driver.demo"), "d")
            .unwrap();
        let d = NodePath::root().child("d");
        store
            .create_child(&d, KindId::new("acme.driver.demo.device"), "dev")
            .unwrap();
        let dev = d.child("dev");
        let p_id = store
            .create_child(&dev, KindId::new("acme.driver.demo.point"), "p")
            .unwrap();
        let q_id = store
            .create_child(&dev, KindId::new("acme.driver.demo.point"), "q")
            .unwrap();
        store
            .write_slot(&dev.child("p"), "value", json!(42))
            .unwrap();
        store
            .add_link(SlotRef::new(p_id, "value"), SlotRef::new(q_id, "value"))
            .unwrap();
    }

    // Second boot: same DB file, fresh store. State must match.
    let repo = Arc::new(SqliteGraphRepo::open_file(db.path()).unwrap());
    let store = GraphStore::with_repo(fresh_kinds(), Arc::new(NullSink), repo).unwrap();
    assert_eq!(store.len(), 5, "root + driver + device + 2 points restored");
    let p = store
        .get(&NodePath::root().child("d").child("dev").child("p"))
        .unwrap();
    let (_, sv) = p.slot_values.iter().find(|(n, _)| n == "value").unwrap();
    assert_eq!(sv.value, json!(42), "slot value restored");
    assert_eq!(sv.generation, 1, "generation restored");
    assert_eq!(store.links().len(), 1, "link survived reopen");
}

#[test]
fn restore_rejects_unknown_kind() {
    let repo = Arc::new(SqliteGraphRepo::open_memory().unwrap());
    repo.save_node(&PersistedNode {
        id: Uuid::new_v4(),
        parent_id: None,
        kind_id: "acme.unknown.kind".into(),
        path: "/".into(),
        name: "/".into(),
        lifecycle: "created".into(),
    })
    .unwrap();
    let result = GraphStore::with_repo(fresh_kinds(), Arc::new(NullSink), repo);
    match result {
        Err(graph::GraphError::UnknownKind(_)) => {}
        Ok(_) => panic!("expected UnknownKind"),
        Err(other) => panic!("wrong error: {other:?}"),
    }
}

#[test]
fn write_through_failure_leaves_memory_clean() {
    struct FlakyRepo;
    impl GraphRepo for FlakyRepo {
        fn load(&self) -> Result<GraphSnapshot, RepoError> {
            Ok(GraphSnapshot::default())
        }
        fn save_node(&self, _n: &PersistedNode) -> Result<(), RepoError> {
            Err(RepoError::Backend("nope".into()))
        }
        fn delete_nodes(&self, _ids: &[Uuid]) -> Result<(), RepoError> {
            Ok(())
        }
        fn upsert_slot(&self, _s: &PersistedSlot) -> Result<(), RepoError> {
            Ok(())
        }
        fn save_link(&self, _l: &PersistedLink) -> Result<(), RepoError> {
            Ok(())
        }
        fn delete_links(&self, _ids: &[Uuid]) -> Result<(), RepoError> {
            Ok(())
        }
    }

    let store =
        GraphStore::with_repo(fresh_kinds(), Arc::new(NullSink), Arc::new(FlakyRepo)).unwrap();
    let err = store
        .create_root(KindId::new("acme.core.station"))
        .expect_err("backend refused");
    assert!(matches!(err, graph::GraphError::Backend(_)));
    assert_eq!(store.len(), 0, "memory untouched after backend refusal");
}
