//! Wire-shape DTOs matching the REST API JSON.
//!
//! These are the client's own types — they don't depend on any server
//! crate. They match the JSON the agent sends/receives, analogous to
//! the TS client's Zod schemas.
//!
//! `schemars::JsonSchema` is derived on output types so that
//! `agent schema` and (Stage 9) OpenAPI can generate JSON Schema from
//! one source of truth.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---- nodes ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeSnapshot {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub parent_id: Option<String>,
    pub lifecycle: String,
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Slot {
    pub name: String,
    pub value: JsonValue,
    pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatedNode {
    pub id: String,
    pub path: String,
}

// ---- slots ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteSlotResponse {
    pub generation: u64,
}

// ---- links ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Link {
    pub id: String,
    pub source: LinkEndpoint,
    pub target: LinkEndpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LinkEndpoint {
    pub node_id: String,
    pub path: Option<String>,
    pub slot: String,
}

/// Reference used when creating a link — either by node path or node ID.
///
/// `skip_serializing_if` is intentional here — this is a *request*
/// body, not CLI output. CLI output contracts (explicit nulls) do not
/// apply to outgoing HTTP request payloads.
#[derive(Debug, Clone, Serialize)]
pub struct LinkEndpointRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub slot: String,
}

impl LinkEndpointRef {
    pub fn by_path(path: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            path: Some(path.into()),
            node_id: None,
            slot: slot.into(),
        }
    }

    pub fn by_id(node_id: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            path: None,
            node_id: Some(node_id.into()),
            slot: slot.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatedLink {
    pub id: String,
}

// ---- lifecycle ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleResponse {
    pub path: String,
    pub to: String,
}

// ---- seed -----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeedResult {
    pub folder: String,
    pub nodes: Vec<SeededNode>,
    pub links: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeededNode {
    pub path: String,
    pub kind: String,
}

// ---- capabilities ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityManifest {
    pub platform: PlatformInfo,
    pub api: ApiInfo,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlatformInfo {
    pub version: String,
    pub flow_schema: u32,
    pub node_schema: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiInfo {
    pub rest: String,
}

/// Explicit nulls for `deprecated_since` / `removal_planned` per
/// CLI.md § "Deterministic JSON output" — `null` means "no value",
/// a missing key means "field doesn't exist at this API version".
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capability {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub deprecated_since: Option<String>,
    #[serde(default)]
    pub removal_planned: Option<String>,
}

// ---- events ---------------------------------------------------------------

/// Graph event from the SSE stream. Internally tagged by `"event"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GraphEvent {
    NodeCreated {
        id: String,
        kind: String,
        path: String,
    },
    NodeRemoved {
        id: String,
        kind: String,
        path: String,
    },
    NodeRenamed {
        id: String,
        old_path: String,
        new_path: String,
    },
    SlotChanged {
        id: String,
        path: String,
        slot: String,
        value: JsonValue,
        generation: u64,
    },
    LifecycleTransition {
        id: String,
        path: String,
        from: String,
        to: String,
    },
    LinkAdded(Link),
    LinkRemoved {
        id: String,
        source: LinkEventEndpoint,
        target: LinkEventEndpoint,
    },
    LinkBroken {
        id: String,
        broken_end: LinkEventEndpoint,
        surviving_end: LinkEventEndpoint,
    },
}

/// Endpoint in a link event — uses `node` (not `node_id`) to match
/// the server's `SlotRef` serde shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkEventEndpoint {
    pub node: String,
    pub slot: String,
}

// ---- kinds ----------------------------------------------------------------

/// Facet classification flag on a kind.
/// Mirrors `spi::Facet` — `camelCase` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Facet {
    IsProtocol,
    IsDriver,
    IsDevice,
    IsPoint,
    IsCompute,
    IsContainer,
    IsSystem,
    IsIdentity,
    IsEphemeral,
    IsWritable,
    IsFlow,
    #[serde(rename = "isIO")]
    IsIo,
}

/// `snake_case` on the wire — mirrors `spi::TriggerPolicy`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriggerPolicy {
    #[default]
    OnAny,
    OnAll,
}

/// `snake_case` on the wire — mirrors `spi::Cardinality`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    #[default]
    ManyPerParent,
    OnePerParent,
    ExactlyOne,
}

/// `snake_case` on the wire — mirrors `spi::CascadePolicy`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CascadePolicy {
    #[default]
    Strict,
    Deny,
    Orphan,
}

/// Parent-matcher in a containment rule — serialised as a one-key map
/// (`{"kind": "..."}` or `{"facet": "isContainer"}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ParentMatcher {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet: Option<Facet>,
}

/// Containment rules — mirrors `spi::ContainmentSchema`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ContainmentSchema {
    #[serde(default)]
    pub must_live_under: Vec<ParentMatcher>,
    #[serde(default)]
    pub may_contain: Vec<ParentMatcher>,
    #[serde(default)]
    pub cardinality_per_parent: Cardinality,
    #[serde(default)]
    pub cascade: CascadePolicy,
}

/// Slot role — mirrors `spi::SlotRole`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    Config,
    Input,
    Output,
    Status,
}

/// Slot shape declared by a kind — mirrors `spi::SlotSchema`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SlotSchema {
    pub name: String,
    pub role: SlotRole,
    #[serde(default)]
    pub value_schema: JsonValue,
    #[serde(default)]
    pub writable: bool,
    #[serde(default)]
    pub trigger: bool,
}

/// Wire shape for `GET /api/v1/kinds`.
///
/// The server flattens `KindManifest` so all its fields appear at the
/// top level alongside `placement_class`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KindDto {
    /// Kind identifier (e.g. `acme.core.station`).
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Set of orthogonal facets (e.g. `["isContainer", "isSystem"]`).
    #[serde(default)]
    pub facets: Vec<Facet>,
    pub containment: ContainmentSchema,
    #[serde(default)]
    pub slots: Vec<SlotSchema>,
    #[serde(default)]
    pub settings_schema: JsonValue,
    #[serde(default)]
    pub msg_overrides: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub trigger_policy: TriggerPolicy,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// `"free"` or `"bound"` based on containment rules.
    pub placement_class: String,
}

fn default_schema_version() -> u32 {
    1
}

// ---- plugins --------------------------------------------------------------

/// Plugin lifecycle state — `snake_case` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginLifecycle {
    Discovered,
    Validated,
    Enabled,
    Disabled,
    Failed,
}

impl std::fmt::Display for PluginLifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Discovered => "discovered",
            Self::Validated => "validated",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Failed => "failed",
        };
        f.write_str(s)
    }
}

/// Summary of a loaded plugin — mirrors `extensions_host::LoadedPluginSummary`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginSummary {
    /// Plugin identifier (transparent string).
    pub id: String,
    pub version: String,
    pub lifecycle: PluginLifecycle,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub has_ui: bool,
    #[serde(default)]
    pub ui_entry: Option<String>,
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub load_errors: Vec<String>,
}

/// Response from plugin enable / disable / reload actions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginActionResponse {
    pub id: String,
    pub lifecycle: PluginLifecycle,
}
