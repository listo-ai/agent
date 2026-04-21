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
    /// Per-slot generation counters. Populated by [`GraphReader`] from
    /// `SlotValue.generation`. Used by the write-plan builder to bake
    /// OCC `generation` into resolved write plan entries.
    ///
    /// `InMemoryReader` fixtures default to empty (all generations `0`
    /// unless set via [`NodeSnapshot::with_slot_generation`]).
    pub slot_generations: HashMap<String, u64>,
    /// Absolute node path — populated by [`GraphReader`]. `None` for
    /// test fixtures created without an explicit path.
    pub path: Option<String>,
    /// Parent node id — populated by [`GraphReader`]. `None` for root
    /// nodes or test fixtures.
    pub parent_id: Option<String>,
}

impl NodeSnapshot {
    pub fn new(id: NodeId, kind: impl Into<KindId>) -> Self {
        Self {
            id,
            kind: kind.into(),
            version: 1,
            slots: HashMap::new(),
            slot_generations: HashMap::new(),
            path: None,
            parent_id: None,
        }
    }

    pub fn with_slot(mut self, name: impl Into<String>, value: JsonValue) -> Self {
        self.slots.insert(name.into(), value);
        self
    }

    /// Set the generation for a named slot. Use in tests that exercise
    /// OCC write-plan baking so the resolved entry carries a non-`None`
    /// `generation`.
    pub fn with_slot_generation(mut self, name: impl Into<String>, generation: u64) -> Self {
        self.slot_generations.insert(name.into(), generation);
        self
    }

    pub fn with_version(mut self, v: u64) -> Self {
        self.version = v;
        self
    }
}

pub trait NodeReader {
    fn get(&self, id: &NodeId) -> Option<NodeSnapshot>;

    /// Children of `parent` in graph order.
    fn children(&self, parent: &NodeId) -> Vec<NodeId>;

    /// Enumerate all nodes. Used by the table endpoint (S3). Default
    /// returns empty — production `GraphReader` overrides this.
    fn list_all(&self) -> Vec<NodeSnapshot> {
        Vec::new()
    }
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

    fn list_all(&self) -> Vec<NodeSnapshot> {
        self.nodes.values().cloned().collect()
    }
}
