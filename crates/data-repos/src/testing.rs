#![allow(clippy::unwrap_used, clippy::panic)]
//! Shared [`GraphRepo`] trait-test harness.
//!
//! Every backend implementation calls into this module from its own
//! integration tests. A query that works on one impl and fails on
//! another is caught here at the seam, not downstream in the graph
//! crate. See CODE-LAYOUT.md \u{00a7} "Testing parity".

use serde_json::json;
use uuid::Uuid;

use crate::{GraphRepo, PersistedLink, PersistedNode, PersistedSlot};

/// Run the full matrix of repo tests against `make`, which produces a
/// fresh, empty repo each call (so impls can clear tables or hand out
/// new temp files between scenarios).
pub fn run_all<R, F>(mut make: F)
where
    R: GraphRepo,
    F: FnMut() -> R,
{
    empty_repo_loads_empty_snapshot(&make());
    save_then_load_roundtrip(&make());
    delete_nodes_and_links_reflects_in_snapshot(&make());
    slot_upsert_bumps_generation(&make());
}

fn empty_repo_loads_empty_snapshot<R: GraphRepo>(r: &R) {
    let snap = r.load().expect("load");
    assert!(snap.nodes.is_empty());
    assert!(snap.slots.is_empty());
    assert!(snap.links.is_empty());
}

fn save_then_load_roundtrip<R: GraphRepo>(r: &R) {
    let root_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    r.save_node(&PersistedNode {
        id: root_id,
        parent_id: None,
        kind_id: "acme.core.station".into(),
        path: "/".into(),
        name: "/".into(),
        lifecycle: "created".into(),
    })
    .unwrap();
    r.save_node(&PersistedNode {
        id: child_id,
        parent_id: Some(root_id),
        kind_id: "acme.core.folder".into(),
        path: "/site".into(),
        name: "site".into(),
        lifecycle: "created".into(),
    })
    .unwrap();
    r.upsert_slot(&PersistedSlot {
        node_id: child_id,
        name: "value".into(),
        role: "output".into(),
        value: json!(42),
        generation: 1,
        kind: None,
    })
    .unwrap();
    let link_id = Uuid::new_v4();
    r.save_link(&PersistedLink {
        id: link_id,
        source_node: root_id,
        source_slot: "out".into(),
        target_node: child_id,
        target_slot: "value".into(),
    })
    .unwrap();

    let snap = r.load().expect("load");
    assert_eq!(snap.nodes.len(), 2);
    assert_eq!(snap.slots.len(), 1);
    assert_eq!(snap.slots[0].value, json!(42));
    assert_eq!(snap.links.len(), 1);
    assert_eq!(snap.links[0].id, link_id);

    // Parents are emitted before children in the returned order.
    let root_ix = snap.nodes.iter().position(|n| n.id == root_id).unwrap();
    let child_ix = snap.nodes.iter().position(|n| n.id == child_id).unwrap();
    assert!(
        root_ix < child_ix,
        "parent must precede child in load order"
    );
}

fn delete_nodes_and_links_reflects_in_snapshot<R: GraphRepo>(r: &R) {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    for (id, path) in [(a, "/a"), (b, "/b")] {
        r.save_node(&PersistedNode {
            id,
            parent_id: None,
            kind_id: "acme.core.folder".into(),
            path: path.into(),
            name: path.trim_start_matches('/').into(),
            lifecycle: "created".into(),
        })
        .unwrap();
    }
    let link_id = Uuid::new_v4();
    r.save_link(&PersistedLink {
        id: link_id,
        source_node: a,
        source_slot: "out".into(),
        target_node: b,
        target_slot: "in".into(),
    })
    .unwrap();

    r.delete_links(&[link_id]).unwrap();
    r.delete_nodes(&[a]).unwrap();

    let snap = r.load().expect("load");
    assert!(snap.links.is_empty());
    assert_eq!(snap.nodes.len(), 1);
    assert_eq!(snap.nodes[0].id, b);
}

fn slot_upsert_bumps_generation<R: GraphRepo>(r: &R) {
    let id = Uuid::new_v4();
    r.save_node(&PersistedNode {
        id,
        parent_id: None,
        kind_id: "acme.core.folder".into(),
        path: "/n".into(),
        name: "n".into(),
        lifecycle: "active".into(),
    })
    .unwrap();
    for gen in 1..=3 {
        r.upsert_slot(&PersistedSlot {
            node_id: id,
            name: "v".into(),
            role: "output".into(),
            value: json!(gen),
            generation: gen,
            kind: None,
        })
        .unwrap();
    }
    let snap = r.load().expect("load");
    let slot = snap
        .slots
        .iter()
        .find(|s| s.node_id == id && s.name == "v")
        .unwrap();
    assert_eq!(slot.generation, 3);
    assert_eq!(slot.value, json!(3));
}
