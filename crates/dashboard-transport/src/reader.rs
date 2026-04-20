//! `GraphReader` — production [`NodeReader`] backed by `graph::GraphStore`.
//!
//! Node version is derived as the max slot generation across all slots.
//! That matches what the doc's cache-key specifies: the "node version"
//! changes whenever any slot mutates. It's an over-estimate for the
//! binding resolver's purposes (a widget cares only about the slots it
//! reads) but cheap and monotonic.

use std::collections::HashMap;
use std::sync::Arc;

use dashboard_runtime::{NodeReader, NodeSnapshot as RuntimeSnapshot};
use graph::GraphStore;
use spi::NodeId;

pub struct GraphReader {
    store: Arc<GraphStore>,
}

impl GraphReader {
    pub fn new(store: Arc<GraphStore>) -> Self {
        Self { store }
    }
}

impl NodeReader for GraphReader {
    fn get(&self, id: &NodeId) -> Option<RuntimeSnapshot> {
        let snap = self.store.get_by_id(*id)?;
        let mut slots = HashMap::new();
        let mut version: u64 = 0;
        for (name, sv) in &snap.slot_values {
            slots.insert(name.clone(), sv.value.clone());
            if sv.generation > version {
                version = sv.generation;
            }
        }
        Some(RuntimeSnapshot {
            id: snap.id,
            kind: snap.kind,
            version,
            slots,
            path: Some(snap.path.to_string()),
            parent_id: snap.parent.map(|p| p.to_string()),
        })
    }

    fn children(&self, parent: &NodeId) -> Vec<NodeId> {
        // Linear scan — acceptable while resolve is bounded by
        // MAX_WIDGETS_PER_PAGE. If profiles point here, add a
        // parent-index to `GraphStore` rather than caching upstream.
        self.store
            .snapshots()
            .into_iter()
            .filter(|s| s.parent == Some(*parent))
            .map(|s| s.id)
            .collect()
    }

    fn list_all(&self) -> Vec<RuntimeSnapshot> {
        self.store
            .snapshots()
            .into_iter()
            .map(|snap| {
                let mut slots = HashMap::new();
                let mut version: u64 = 0;
                for (name, sv) in snap.slot_values {
                    if sv.generation > version {
                        version = sv.generation;
                    }
                    slots.insert(name, sv.value.clone());
                }
                RuntimeSnapshot {
                    id: snap.id,
                    kind: snap.kind,
                    version,
                    slots,
                    path: Some(snap.path.to_string()),
                    parent_id: snap.parent.map(|p| p.to_string()),
                }
            })
            .collect()
    }
}
