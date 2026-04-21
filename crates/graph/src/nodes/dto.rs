//! Wire shape for node snapshots.
//!
//! Mirrors [`crate::NodeSnapshot`] with the fields the API needs:
//! stringified ids/paths, parent metadata, a `has_children` rollup the
//! UI uses to draw expand chevrons without a speculative child query,
//! and a flattened slot list. Everything here is serialisable — scopes
//! never hand an internal type to a transport.

use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::lifecycle::Lifecycle;
use crate::node::NodeSnapshot;

#[derive(Clone, Debug, Serialize)]
pub struct NodeDto {
    pub id: String,
    pub kind: String,
    pub path: String,
    /// Materialised parent path (`"/"` for depth-1 nodes, `null` for
    /// the root). Exposed so tree UIs can filter direct children with
    /// `filter=parent_path==/station/floor1` in a single query.
    pub parent_path: Option<String>,
    pub parent_id: Option<String>,
    /// Whether the node has at least one child. Computed server-side
    /// so tree UIs can show expand chevrons without a speculative
    /// child query.
    pub has_children: bool,
    pub lifecycle: Lifecycle,
    pub slots: Vec<SlotDto>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SlotDto {
    pub name: String,
    pub value: JsonValue,
    pub generation: u64,
}

impl From<NodeSnapshot> for NodeDto {
    fn from(s: NodeSnapshot) -> Self {
        Self {
            id: s.id.to_string(),
            kind: s.kind.as_str().to_string(),
            parent_path: s.path.parent().map(|p| p.to_string()),
            path: s.path.to_string(),
            parent_id: s.parent.map(|p| p.to_string()),
            has_children: s.has_children,
            lifecycle: s.lifecycle,
            slots: s
                .slot_values
                .into_iter()
                .map(|(name, sv)| SlotDto {
                    name,
                    value: sv.value,
                    generation: sv.generation,
                })
                .collect(),
        }
    }
}
