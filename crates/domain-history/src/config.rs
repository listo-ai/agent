//! `sys.core.history.config` kind — declaration and settings types.
//!
//! **Cardinality rule (one per node):** The placement validator enforces
//! that at most one `HistoryConfig` exists per parent node by default.
//! Multiple configs require `history.allow_multiple_configs_per_node = true`
//! in the agent config (Stage 8 hardening).
//!
//! **`isSystem` facet:** HistoryConfig nodes are tagged `isSystem = true`
//! so they are hidden from default `list_children` responses.  Only
//! callers that pass `include_system = true` see them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use spi::{Cardinality, ContainmentSchema, Facet, FacetSet};
use spi::{KindManifest, SlotRole, SlotSchema, SlotValueKind};

pub const KIND_ID: &str = "sys.core.history.config";

/// Per-slot policy variant declared in `HistoryConfigSettings::slots`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum SlotPolicy {
    /// Record when the value changes beyond the deadband (Number) or
    /// changes at all (Bool / String / Json / Binary).
    Cov {
        /// For Number slots only.  `0.0` means any change.
        #[serde(default)]
        deadband: f64,
        /// Minimum milliseconds between consecutive records (rate floor).
        #[serde(default = "default_min_interval_ms")]
        min_interval_ms: u64,
        /// Maximum milliseconds between records (heartbeat ceiling).
        #[serde(default = "default_max_gap_ms")]
        max_gap_ms: u64,
        /// Per-slot sample cap override.  `None` → inherits config/platform.
        #[serde(default)]
        max_samples: Option<u64>,
    },
    /// Record on a fixed wall- or monotonic-clock interval.
    Interval {
        period_ms: u64,
        /// If true, align each sample to the nearest multiple of
        /// `period_ms` from Unix epoch (wall-clock alignment).
        #[serde(default)]
        align_to_wall: bool,
        /// Per-slot sample cap override.
        #[serde(default)]
        max_samples: Option<u64>,
    },
    /// Record only when `history.record` REST / flow node fires.
    OnDemand,
}

fn default_min_interval_ms() -> u64 {
    0
}
fn default_max_gap_ms() -> u64 {
    900_000
}

/// Retention settings on a `HistoryConfig` node (wraps both table shapes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionSettings {
    /// Drop records older than this many days.  `None` = no time-based expiry.
    #[serde(default)]
    pub keep_for_days: Option<u32>,
    /// Per-config row-cap override.  `None` = inherit platform default.
    #[serde(default)]
    pub max_samples_per_slot: Option<u64>,
}

impl Default for RetentionSettings {
    fn default() -> Self {
        Self {
            keep_for_days: None,
            max_samples_per_slot: None,
        }
    }
}

/// Full deserialized settings blob for an `sys.core.history.config` node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfigSettings {
    /// Map of slot_name → policy.
    #[serde(default)]
    pub slots: BTreeMap<String, SlotPolicy>,
    #[serde(default)]
    pub retention: RetentionSettings,
    /// Publish `graph.<tenant>.<path>.slot.<slot>.historized` events on
    /// the fleet bus.  Off by default — per-record publish on chatty
    /// slots is expensive; the slot-change event is already on the bus.
    #[serde(default)]
    pub publish_historized_events: bool,
    /// Critical-tier config: bypasses the in-memory buffer and commits
    /// per-write.  Refused rather than dropped when back-pressured.
    #[serde(default)]
    pub critical: bool,
}

impl Default for HistoryConfigSettings {
    fn default() -> Self {
        Self {
            slots: BTreeMap::new(),
            retention: RetentionSettings::default(),
            publish_historized_events: false,
            critical: false,
        }
    }
}

/// Opaque in-memory handle to a resolved `HistoryConfig` node.
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    pub config_node_id: uuid::Uuid,
    pub parent_node_id: uuid::Uuid,
    pub settings: HistoryConfigSettings,
}

/// Return the [`KindManifest`] for `sys.core.history.config`.
///
/// Registered at startup alongside other first-party kinds.
pub fn manifest() -> KindManifest {
    KindManifest::new(
        KIND_ID,
        ContainmentSchema {
            // can be attached as a child of any node kind
            must_live_under: vec![],
            // leaf — holds no children of its own
            may_contain: vec![],
            // one per parent node (default; operator can relax via platform setting)
            cardinality_per_parent: Cardinality::OnePerParent,
            cascade: Default::default(),
        },
    )
    .with_facets(FacetSet::of([Facet::IsSystem]))
    .with_settings_schema(settings_schema())
    .with_slots(vec![
        SlotSchema::new("status", SlotRole::Status).with_kind(SlotValueKind::Json)
    ])
}

fn settings_schema() -> JsonValue {
    json!({
        "type": "object",
        "properties": {
            "slots": {
                "type": "object",
                "description": "Map of slot_name → recording policy",
                "additionalProperties": {
                    "type": "object",
                    "oneOf": [
                        {
                            "properties": {
                                "policy": { "const": "cov" },
                                "deadband": { "type": "number", "minimum": 0, "default": 0 },
                                "min_interval_ms": { "type": "integer", "minimum": 0, "default": 0 },
                                "max_gap_ms": { "type": "integer", "minimum": 0, "default": 900000 },
                                "max_samples": { "type": "integer", "minimum": 1 }
                            },
                            "required": ["policy"]
                        },
                        {
                            "properties": {
                                "policy": { "const": "interval" },
                                "period_ms": { "type": "integer", "minimum": 1 },
                                "align_to_wall": { "type": "boolean", "default": false },
                                "max_samples": { "type": "integer", "minimum": 1 }
                            },
                            "required": ["policy", "period_ms"]
                        },
                        {
                            "properties": {
                                "policy": { "const": "on_demand" }
                            },
                            "required": ["policy"]
                        }
                    ]
                }
            },
            "retention": {
                "type": "object",
                "properties": {
                    "keep_for_days": { "type": "integer", "minimum": 1 },
                    "max_samples_per_slot": { "type": "integer", "minimum": 1 }
                }
            },
            "publish_historized_events": { "type": "boolean", "default": false },
            "critical": { "type": "boolean", "default": false }
        },
        "additionalProperties": false
    })
}
