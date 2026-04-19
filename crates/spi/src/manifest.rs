//! Kind manifest — declarative description of a node kind.
//!
//! `KindManifest` is the contract surface every extension author emits
//! (via `#[derive(NodeKind)]` in `extensions-sdk`) and every runtime
//! consumes (the graph registry, the placement validator, the Studio
//! palette, the process-plugin gRPC describer). Keep it here so the SDK
//! depends only on `spi`, never on `graph`.

use serde::{Deserialize, Serialize};

use crate::containment::ContainmentSchema;
use crate::facets::FacetSet;
use crate::ids::KindId;
use crate::slot_schema::SlotSchema;

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
