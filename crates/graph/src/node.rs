//! Internal node record plus a public read-only snapshot.
//!
//! Stage 1 stores nodes in an in-memory map owned by the store. A
//! persistent backing via `data-repos` lands in Stage 5 behind the same
//! public surface.

use crate::ids::{KindId, NodeId, NodePath};
use crate::lifecycle::Lifecycle;
use crate::slot::{SlotMap, SlotValue};

#[derive(Debug, Clone)]
pub(crate) struct NodeRecord {
    pub id: NodeId,
    pub kind: KindId,
    pub path: NodePath,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub slots: SlotMap,
    pub lifecycle: Lifecycle,
}

impl NodeRecord {
    pub(crate) fn new(id: NodeId, kind: KindId, path: NodePath, parent: Option<NodeId>) -> Self {
        Self {
            id,
            kind,
            path,
            parent,
            children: Vec::new(),
            slots: SlotMap::new(),
            lifecycle: Lifecycle::Created,
        }
    }
}

/// Read-only snapshot of a node — what the public API returns.
#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    pub id: NodeId,
    pub kind: KindId,
    pub path: NodePath,
    pub parent: Option<NodeId>,
    pub lifecycle: Lifecycle,
    pub slot_values: Vec<(String, SlotValue)>,
}

impl NodeSnapshot {
    pub(crate) fn from_record(r: &NodeRecord) -> Self {
        Self {
            id: r.id,
            kind: r.kind.clone(),
            path: r.path.clone(),
            parent: r.parent,
            lifecycle: r.lifecycle,
            slot_values: r
                .slots
                .iter()
                .map(|(name, v)| (name.clone(), v.clone()))
                .collect(),
        }
    }
}
