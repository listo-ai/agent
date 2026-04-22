//! Wire shape for node snapshots.
//!
//! Mirrors [`crate::NodeSnapshot`] with the fields the API needs:
//! stringified ids/paths, parent metadata, a `has_children` rollup the
//! UI uses to draw expand chevrons without a speculative child query,
//! and a flattened slot list. Everything here is serialisable — scopes
//! never hand an internal type to a transport.

use serde::Serialize;
use serde_json::Value as JsonValue;
use spi::{KindManifest, Quantity, Unit};

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
    /// Physical quantity this slot measures, if declared. Consumed by
    /// clients to decide which user-preference (`temperature_unit`,
    /// `pressure_unit`, …) governs display conversion. Absent for
    /// dimensionless slots. See `docs/design/USER-PREFERENCES.md` §
    /// "Slot units".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<Quantity>,
    /// Unit the stored `value` is expressed in. Typically the
    /// quantity's canonical unit; set explicitly when the slot
    /// opted out of ingest-time conversion. Paired with `quantity`
    /// to drive client-side formatting: `"22.4 °C" → "72.3 °F"`.
    /// Absent when `quantity` is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
}

impl From<NodeSnapshot> for NodeDto {
    fn from(s: NodeSnapshot) -> Self {
        Self::from_snapshot(s, None)
    }
}

impl NodeDto {
    /// Build a DTO from a snapshot, enriching each slot with
    /// `quantity`/`unit` metadata from the kind manifest. Pass
    /// `Some(manifest)` on every read path that has the registry
    /// handy — the `From<NodeSnapshot>` variant is only used in
    /// tests / debug paths where unit annotations aren't needed.
    pub fn from_snapshot(s: NodeSnapshot, manifest: Option<&KindManifest>) -> Self {
        let slot_meta = |name: &str| -> (Option<Quantity>, Option<Unit>) {
            let Some(m) = manifest else {
                return (None, None);
            };
            let Some(schema) = m.slots.iter().find(|slot| slot.name == name) else {
                return (None, None);
            };
            // Stored unit = declared `unit` if present, else the
            // quantity's canonical from the registry. We cannot resolve
            // canonical here without the registry, so expose the
            // `unit` as-declared; `None` here means "canonical for
            // quantity" and the client resolves via `/api/v1/units`.
            (schema.quantity, schema.unit)
        };
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
                .map(|(name, sv)| {
                    let (quantity, unit) = slot_meta(&name);
                    SlotDto {
                        name,
                        value: sv.value,
                        generation: sv.generation,
                        quantity,
                        unit,
                    }
                })
                .collect(),
        }
    }
}
