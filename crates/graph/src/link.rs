//! Links between slots — the "wires" shown in the Studio canvas.
//!
//! A link connects a source slot on one node to a target slot on
//! another (or the same) node. When either endpoint is deleted the link
//! is removed and a `LinkBroken` event fires on the surviving end —
//! never a silent disconnect. See EVERYTHING-AS-NODE.md § "Cascading
//! delete".

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LinkId(pub Uuid);

impl LinkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for LinkId {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference to a specific slot on a specific node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotRef {
    pub node: NodeId,
    pub slot: String,
}

impl SlotRef {
    pub fn new(node: NodeId, slot: impl Into<String>) -> Self {
        Self { node, slot: slot.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: LinkId,
    pub source: SlotRef,
    pub target: SlotRef,
}

impl Link {
    pub fn new(source: SlotRef, target: SlotRef) -> Self {
        Self { id: LinkId::new(), source, target }
    }
}
