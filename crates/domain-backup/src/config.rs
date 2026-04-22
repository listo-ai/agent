// STAGE-0 complete — sys.backup.config kind manifest + settings schema

//! `sys.backup.config` kind — per-device backup cadence and retention.
//!
//! This is a **config-only kind** (no `NodeBehavior`). The scheduler
//! that reads these settings and triggers snapshot creation lands in
//! Phase 1. For now, the kind exists so operators can write config
//! into the graph and tooling can render it.
//!
//! Cardinality: one per station (the root node). `isSystem` so it's
//! hidden from default listings.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use spi::{Cardinality, ContainmentSchema, Facet, FacetSet, ParentMatcher};
use spi::{KindManifest, Portability, SlotRole, SlotSchema, SlotValueKind};

pub const KIND_ID: &str = "sys.backup.config";

/// Settings shape for `sys.backup.config`. Written via
/// `PATCH /api/v1/nodes/<id>/settings`.
///
/// See BACKUP.md § 7.2 for operational defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfigSettings {
    /// Whether automatic snapshots are enabled. Defaults to `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Full-snapshot interval in seconds. Default: 86 400 (daily).
    #[serde(default = "default_full_interval_sec")]
    pub full_interval_sec: u64,

    /// Incremental (WAL archive) interval in seconds. Default: 3 600
    /// (hourly). Set to `0` to disable incremental snapshots.
    #[serde(default = "default_incremental_interval_sec")]
    pub incremental_interval_sec: u64,

    /// Minimum retention floor in seconds — never delete snapshots
    /// younger than this. Default: 86 400 (24 hours).
    #[serde(default = "default_retention_floor_sec")]
    pub retention_floor_sec: u64,

    /// Maximum number of local snapshots to keep before pruning the
    /// oldest. Default: 7.
    #[serde(default = "default_max_local_snapshots")]
    pub max_local_snapshots: u32,

    /// Whether the pre-apply hook triggers a snapshot before OTA.
    #[serde(default = "default_true")]
    pub snapshot_before_apply: bool,

    /// Whether the pre-reset hook triggers a snapshot before reset.
    #[serde(default = "default_true")]
    pub snapshot_before_reset: bool,
}

fn default_true() -> bool {
    true
}
fn default_full_interval_sec() -> u64 {
    86_400
}
fn default_incremental_interval_sec() -> u64 {
    3_600
}
fn default_retention_floor_sec() -> u64 {
    86_400
}
fn default_max_local_snapshots() -> u32 {
    7
}

impl Default for BackupConfigSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            full_interval_sec: default_full_interval_sec(),
            incremental_interval_sec: default_incremental_interval_sec(),
            retention_floor_sec: default_retention_floor_sec(),
            max_local_snapshots: default_max_local_snapshots(),
            snapshot_before_apply: true,
            snapshot_before_reset: true,
        }
    }
}

/// Build the JSON Schema for settings validation + UI rendering.
fn settings_schema() -> JsonValue {
    json!({
        "type": "object",
        "properties": {
            "enabled": {
                "type": "boolean",
                "default": true,
                "description": "Enable automatic snapshot scheduling."
            },
            "full_interval_sec": {
                "type": "integer",
                "minimum": 60,
                "default": 86400,
                "description": "Full-snapshot interval in seconds."
            },
            "incremental_interval_sec": {
                "type": "integer",
                "minimum": 0,
                "default": 3600,
                "description": "Incremental (WAL) snapshot interval. 0 = disabled."
            },
            "retention_floor_sec": {
                "type": "integer",
                "minimum": 0,
                "default": 86400,
                "description": "Never delete snapshots younger than this (seconds)."
            },
            "max_local_snapshots": {
                "type": "integer",
                "minimum": 1,
                "default": 7,
                "description": "Maximum local snapshots before pruning."
            },
            "snapshot_before_apply": {
                "type": "boolean",
                "default": true,
                "description": "Snapshot before OTA apply."
            },
            "snapshot_before_reset": {
                "type": "boolean",
                "default": true,
                "description": "Snapshot before factory/medium reset."
            }
        },
        "additionalProperties": false
    })
}

/// Build the `KindManifest` for `sys.backup.config`.
pub fn manifest() -> KindManifest {
    KindManifest::new(
        KIND_ID,
        ContainmentSchema {
            must_live_under: vec![ParentMatcher::Kind("sys.core.station".into())],
            may_contain: vec![],
            cardinality_per_parent: Cardinality::OnePerParent,
            cascade: Default::default(),
        },
    )
    .with_facets(FacetSet::of([Facet::IsSystem]))
    .with_display_name("Backup Configuration")
    .with_settings_schema(settings_schema())
    .with_slots(vec![
        // Status slot: last snapshot metadata (JSON).
        SlotSchema::new("last_snapshot", SlotRole::Status)
            .with_kind(SlotValueKind::Json)
            .with_portability(Portability::Device)
            .internal(),
        // Status slot: next scheduled snapshot time.
        SlotSchema::new("next_snapshot_ms", SlotRole::Status)
            .with_kind(SlotValueKind::Number)
            .with_portability(Portability::Derived)
            .internal(),
        // Status slot: total snapshot count on local disk.
        SlotSchema::new("local_count", SlotRole::Status)
            .with_kind(SlotValueKind::Number)
            .with_portability(Portability::Derived)
            .internal(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_valid() {
        let m = manifest();
        assert_eq!(m.id.as_str(), KIND_ID);
        assert!(m.facets.contains(Facet::IsSystem));
        assert_eq!(m.slots.len(), 3);
        assert!(!m.settings_schema.is_null());
    }

    #[test]
    fn settings_defaults_roundtrip() {
        let s = BackupConfigSettings::default();
        let json = serde_json::to_value(&s).unwrap();
        let back: BackupConfigSettings = serde_json::from_value(json).unwrap();
        assert!(back.enabled);
        assert_eq!(back.full_interval_sec, 86_400);
        assert_eq!(back.max_local_snapshots, 7);
    }

    #[test]
    fn slot_portability_is_correct() {
        let m = manifest();
        let last = m.slots.iter().find(|s| s.name == "last_snapshot").unwrap();
        assert_eq!(last.portability, Portability::Device);

        let next = m.slots.iter().find(|s| s.name == "next_snapshot_ms").unwrap();
        assert_eq!(next.portability, Portability::Derived);
    }
}
