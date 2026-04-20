//! Kind manifest — declarative description of a node kind.
//!
//! `KindManifest` is the contract surface every block author emits
//! (via `#[derive(NodeKind)]` in `blocks-sdk`) and every runtime
//! consumes (the graph registry, the placement validator, the Studio
//! palette, the process-block gRPC describer). Keep it here so the SDK
//! depends only on `spi`, never on `graph`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::containment::ContainmentSchema;
use crate::facets::FacetSet;
use crate::ids::KindId;
use crate::slot_schema::SlotSchema;

/// When a behaviour's `on_message` should fire relative to inbound
/// trigger-slot writes.
///
/// Stage 3a-2 ships only `OnAny` (each trigger slot write fires the
/// behaviour exactly once). `OnAll` (await every trigger slot first)
/// arrives with `sys.logic.gate` in a later stage — it's modelled here
/// so manifests can declare it without a schema bump.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerPolicy {
    #[default]
    OnAny,
    OnAll,
}

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
    /// JSON Schema describing the behaviour's settings object. Optional
    /// — manifest-only kinds have no settings. The runtime feeds this
    /// to `ResolvedSettings::resolve` to fill defaults and validate
    /// merged settings.
    #[serde(default)]
    pub settings_schema: JsonValue,
    /// Map from settings field name → message metadata key. If the
    /// inbound `Msg` carries the named metadata key, its value wins
    /// over the persisted config for that one resolution.
    #[serde(default)]
    pub msg_overrides: BTreeMap<String, String>,
    /// How `on_message` fires across multiple trigger inputs.
    #[serde(default)]
    pub trigger_policy: TriggerPolicy,
    /// Manifest schema version — bumps per VERSIONING.md.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// SDUI default views for this kind. Each view is a reusable
    /// component tree that `GET /api/v1/ui/render?target=<id>` resolves
    /// with `$target` pointing at the node being rendered. Empty by
    /// default — kinds without any view fall through to whatever UI the
    /// caller authors explicitly.
    #[serde(default)]
    pub views: Vec<KindView>,
}

/// A default SDUI view for a kind. See SDUI.md § S5.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindView {
    /// Stable view identifier within the kind's manifest
    /// (e.g. `"overview"`, `"settings"`).
    pub id: String,
    /// Human-readable title shown in pickers / Studio view menus.
    pub title: String,
    /// Component-tree template. Wire shape matches `ui_ir::ComponentTree`
    /// but is stored here as opaque JSON so `spi` stays free of
    /// `ui-ir` as a dependency.
    pub template: JsonValue,
    /// Higher priority wins when no `view` query param is supplied.
    /// Ties break by array order.
    #[serde(default)]
    pub priority: i32,
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
            settings_schema: JsonValue::Null,
            msg_overrides: BTreeMap::new(),
            trigger_policy: TriggerPolicy::default(),
            schema_version: 1,
            views: Vec::new(),
        }
    }

    pub fn with_settings_schema(mut self, schema: JsonValue) -> Self {
        self.settings_schema = schema;
        self
    }

    pub fn with_msg_overrides<I, K, V>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.msg_overrides = items
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        self
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
