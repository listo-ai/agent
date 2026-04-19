//! Kind registry. Kinds are reverse-DNS-identified types; the registry
//! maps `KindId` to `KindManifest`. Domain crates and extensions
//! register their kinds here — this is the substrate extension
//! contribution lands in.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::containment::ContainmentSchema;
use crate::facets::FacetSet;
use crate::ids::KindId;
use crate::slot::SlotSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindManifest {
    pub id: KindId,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub facets: FacetSet,
    pub containment: ContainmentSchema,
    #[serde(default)]
    pub slots: Vec<SlotSchema>,
    /// Manifest schema version — bumps per VERSIONING.md.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_schema_version() -> u32 {
    1
}

impl KindManifest {
    pub fn new(id: impl Into<KindId>, containment: ContainmentSchema) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            facets: FacetSet::default(),
            containment,
            slots: Vec::new(),
            schema_version: 1,
        }
    }

    pub fn with_facets(mut self, facets: FacetSet) -> Self {
        self.facets = facets;
        self
    }

    pub fn with_slots(mut self, slots: Vec<SlotSchema>) -> Self {
        self.slots = slots;
        self
    }

    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }
}

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
}
