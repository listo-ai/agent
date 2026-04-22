#![allow(clippy::unwrap_used, clippy::panic)]
//! Tenant-surface writability guard.
//!
//! `SlotSchema::writable = false` on a kind manifest means operators,
//! flow authors, MCP tools, and fleet peers cannot PATCH the slot.
//! Bootstrappers and engine writes legitimately populate `writable:
//! false` status slots (e.g. `/agent/setup.status`,
//! `/agent/fleet.connection`) — they must continue to succeed via the
//! default [`graph::GraphStore::write_slot`] path.
//!
//! See `docs/design/SYSTEM-BOOTSTRAP.md` § `sys.auth.setup` node kind.

use std::sync::Arc;

use graph::{GraphError, GraphStore, KindRegistry, NullSink, WriteSlotOpts};
use serde_json::json;
use spi::{
    Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest, NodePath,
    SlotRole, SlotSchema, SlotValueKind,
};

fn kind_with_status_and_value() -> KindManifest {
    KindManifest {
        id: KindId::new("test.status_holder"),
        display_name: None,
        facets: FacetSet::of([Facet::IsAnywhere]),
        containment: ContainmentSchema {
            must_live_under: vec![],
            may_contain: vec![],
            cardinality_per_parent: Cardinality::ManyPerParent,
            cascade: CascadePolicy::Strict,
        },
        slots: vec![
            // Status slot — bootstrapper-writable, NOT tenant-writable.
            SlotSchema::new("status", SlotRole::Status).with_kind(SlotValueKind::String),
            // Plain writable input slot for control tests.
            SlotSchema::new("value", SlotRole::Input)
                .with_kind(SlotValueKind::Number)
                .writable(),
        ],
        settings_schema: serde_json::Value::Null,
        msg_overrides: Default::default(),
        trigger_policy: Default::default(),
        schema_version: 1,
        views: Vec::new(),
    }
}

fn fresh_store() -> Arc<GraphStore> {
    let kinds = KindRegistry::new();
    graph::seed::register_builtins(&kinds);
    kinds.register(kind_with_status_and_value());
    let store = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
    store.create_root(KindId::new("sys.core.station")).unwrap();
    store
        .create_child(&NodePath::root(), KindId::new("test.status_holder"), "n")
        .unwrap();
    store
}

#[test]
fn internal_write_slot_permits_writable_false_slot() {
    let store = fresh_store();
    let path = NodePath::root().child("n");
    // Bootstrapper path: must succeed, no surprises.
    store
        .write_slot(&path, "status", json!("unconfigured"))
        .unwrap();
    let snap = store.get(&path).unwrap();
    let v = snap
        .slot_values
        .iter()
        .find(|(n, _)| n == "status")
        .unwrap()
        .1
        .value
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(v, "unconfigured");
}

#[test]
fn tenant_write_rejects_writable_false_slot() {
    let store = fresh_store();
    let path = NodePath::root().child("n");
    let err = store
        .write_slot_with(&path, "status", json!("hacked"), WriteSlotOpts::tenant())
        .unwrap_err();
    match err {
        GraphError::SlotNotWritable { path: p, slot } => {
            assert_eq!(slot, "status");
            assert_eq!(p, NodePath::root().child("n"));
        }
        other => panic!("expected SlotNotWritable, got {other:?}"),
    }
}

#[test]
fn tenant_write_allows_writable_true_slot() {
    let store = fresh_store();
    let path = NodePath::root().child("n");
    store
        .write_slot_with(&path, "value", json!(42), WriteSlotOpts::tenant())
        .unwrap();
}

#[test]
fn tenant_cas_write_rejects_writable_false_slot_before_cas_check() {
    // Order matters: writability guard runs before CAS, so a caller
    // testing a non-writable slot gets SlotNotWritable, not
    // GenerationMismatch (which would leak the slot's existence and
    // current generation).
    let store = fresh_store();
    let path = NodePath::root().child("n");
    let err = store
        .write_slot_with(
            &path,
            "status",
            json!("hacked"),
            WriteSlotOpts::tenant_expected(999),
        )
        .unwrap_err();
    assert!(matches!(err, GraphError::SlotNotWritable { .. }));
}

#[test]
fn tenant_cas_write_propagates_generation_mismatch_on_writable_slot() {
    let store = fresh_store();
    let path = NodePath::root().child("n");
    let err = store
        .write_slot_with(
            &path,
            "value",
            json!(1),
            WriteSlotOpts::tenant_expected(99),
        )
        .unwrap_err();
    assert!(matches!(err, GraphError::GenerationMismatch { .. }));
}
