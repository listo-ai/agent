//! `NodeReader` — the seam between the runtime and the graph.
//!
//! The binding resolver and context-stack builder read nodes through
//! this trait. The real implementation (M3+) is backed by
//! `graph::GraphStore`; tests use [`InMemoryReader`].

use std::collections::HashMap;

use serde_json::Value as JsonValue;
use spi::{KindId, NodeId};

/// A single node snapshot as seen by the runtime. Versions drive cache
/// invalidation; slots are opaque JSON.
#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    pub id: NodeId,
    pub kind: KindId,
    pub version: u64,
    pub slots: HashMap<String, JsonValue>,
}

impl NodeSnapshot {
    pub fn new(id: NodeId, kind: impl Into<KindId>) -> Self {
        Self {
            id,
            kind: kind.into(),
            version: 1,
            slots: HashMap::new(),
        }
    }

    pub fn with_slot(mut self, name: impl Into<String>, value: JsonValue) -> Self {
        self.slots.insert(name.into(), value);
        self
    }

    pub fn with_version(mut self, v: u64) -> Self {
        self.version = v;
        self
    }
}

pub trait NodeReader {
    fn get(&self, id: &NodeId) -> Option<NodeSnapshot>;

    /// Children of `parent` in graph order. Empty if the parent has no
    /// children or does not exist.
    fn children(&self, parent: &NodeId) -> Vec<NodeId>;
}

/// Test-only reader backed by a `HashMap`. Not exported from the crate
/// prelude as a production dependency — useful for unit tests and
/// dashboard fixtures.
#[derive(Debug, Default, Clone)]
pub struct InMemoryReader {
    nodes: HashMap<NodeId, NodeSnapshot>,
    children: HashMap<NodeId, Vec<NodeId>>,
}

impl InMemoryReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, snap: NodeSnapshot) {
        self.nodes.insert(snap.id, snap);
    }

    pub fn with(mut self, snap: NodeSnapshot) -> Self {
        self.insert(snap);
        self
    }

    /// Record a parent → child edge for tests that exercise
    /// [`NodeReader::children`].
    pub fn with_child(mut self, parent: NodeId, child: NodeId) -> Self {
        self.children.entry(parent).or_default().push(child);
        self
    }
}

impl NodeReader for InMemoryReader {
    fn get(&self, id: &NodeId) -> Option<NodeSnapshot> {
        self.nodes.get(id).cloned()
    }

    fn children(&self, parent: &NodeId) -> Vec<NodeId> {
        self.children.get(parent).cloned().unwrap_or_default()
    }
}
