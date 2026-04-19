//! Kind registry — runtime lookup of `KindManifest` values registered by
//! domain crates and extensions. The manifest type itself lives in
//! [`spi::KindManifest`] so the SDK never has to depend on this crate.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde_json::json;
use spi::{KindId, KindManifest};
use spi::{SlotRole, SlotSchema};

/// Thread-safe kind registry.
///
/// Registrations are cheap and typically happen once at agent startup.
/// Lookups are frequent — placement enforcement reads from here on every
/// mutation. The `RwLock` makes reads cheap and writes exclusive.
#[derive(Debug, Default, Clone)]
pub struct KindRegistry {
    inner: Arc<RwLock<HashMap<KindId, KindManifest>>>,
}

impl KindRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a kind. Replaces any existing manifest with the same id.
    pub fn register(&self, manifest: KindManifest) {
        let mut map = self
            .inner
            .write()
            .expect("KindRegistry lock poisoned — programmer error");
        let manifest = with_synthesised_slots(manifest);
        tracing::debug!(kind = %manifest.id, "registering kind");
        map.insert(manifest.id.clone(), manifest);
    }

    pub fn get(&self, id: &KindId) -> Option<KindManifest> {
        let map = self.inner.read().ok()?;
        map.get(id).cloned()
    }

    pub fn contains(&self, id: &KindId) -> bool {
        self.inner
            .read()
            .map(|m| m.contains_key(id))
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.inner.read().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot of every registered manifest. Used by the REST palette
    /// endpoint and the CLI. Order is unspecified — callers sort for
    /// display.
    pub fn all(&self) -> Vec<KindManifest> {
        self.inner
            .read()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }
}

/// Inject synthesised config slots that every kind gets for free:
///
/// * `position` — canvas x/y (editor metadata, not behaviour).
/// * `notes`    — free-form annotation.
/// * `settings` — behaviour settings blob, injected only for kinds
///   whose manifest declares a non-null `settings_schema`. This is
///   what makes settings first-class graph state: persisted, subject
///   to the same write-through / event / subscription machinery as
///   every other slot, and therefore free of the
///   `BehaviorRegistry::configs` parallel-state antipattern (see
///   [`docs/design/EVERYTHING-AS-NODE.md`] § "The agent itself is a
///   node too — no parallel state").
fn with_synthesised_slots(mut manifest: KindManifest) -> KindManifest {
    ensure_slot(
        &mut manifest,
        SlotSchema::new("position", SlotRole::Config)
            .writable()
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                },
                "required": ["x", "y"],
                "additionalProperties": false
            })),
    );

    ensure_slot(
        &mut manifest,
        SlotSchema::new("notes", SlotRole::Config)
            .writable()
            .with_schema(json!({
                "type": ["string", "null"]
            })),
    );

    if !manifest.settings_schema.is_null() {
        let schema = manifest.settings_schema.clone();
        ensure_slot(
            &mut manifest,
            SlotSchema::new("settings", SlotRole::Config)
                .writable()
                .with_schema(schema),
        );
    }

    manifest
}

fn ensure_slot(manifest: &mut KindManifest, slot: SlotSchema) {
    if manifest
        .slots
        .iter()
        .all(|existing| existing.name != slot.name)
    {
        manifest.slots.push(slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::{ContainmentSchema, FacetSet};

    #[test]
    fn register_injects_canvas_slots() {
        let kinds = KindRegistry::new();
        let manifest = KindManifest {
            id: KindId::new("sys.test.kind"),
            display_name: Some("Test".to_string()),
            facets: FacetSet::default(),
            containment: ContainmentSchema::default(),
            slots: Vec::new(),
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        };

        kinds.register(manifest);
        let stored = kinds
            .get(&KindId::new("sys.test.kind"))
            .expect("kind should be registered");

        assert!(stored.slots.iter().any(|slot| slot.name == "position"));
        assert!(stored.slots.iter().any(|slot| slot.name == "notes"));
    }
}
