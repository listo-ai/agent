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
    /// Materialised parent path (`"/"` for depth-1, `null` for root).
    /// Lets tree UIs fetch direct children via
    /// `filter=parent_path==/station/floor1`.
    #[serde(default)]
    pub parent_path: Option<String>,
    pub parent_id: Option<String>,
    /// Whether the node has at least one child in the store.
    /// Computed server-side so tree UIs can render expand chevrons
    /// without issuing a speculative child query.
    #[serde(default)]
    pub has_children: bool,
    pub lifecycle: String,
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PageMeta {
    pub total: u64,
    pub page: u64,
    pub size: u64,
    pub pages: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeListResponse {
    pub data: Vec<NodeSnapshot>,
    pub meta: PageMeta,
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
    /// Kind identifier (e.g. `sys.core.station`).
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

// ---- auth -----------------------------------------------------------------

// ---- ui (dashboard) -------------------------------------------------------

/// Node in a `GET /api/v1/ui/nav` tree slice. Field order mirrors the
/// server's `dashboard_transport::nav::NavNode`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiNavNode {
    pub id: String,
    pub title: Option<String>,
    pub path: Option<String>,
    pub icon: Option<String>,
    pub order: Option<i64>,
    pub frame_alias: Option<String>,
    pub frame_ref: Option<JsonValue>,
    pub children: Vec<UiNavNode>,
}

/// Request body for `POST /api/v1/ui/resolve`. Mirrors
/// `dashboard_transport::resolve::ResolveRequest` exactly.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiResolveRequest {
    pub page_ref: String,
    #[serde(default)]
    pub stack: Vec<String>,
    #[serde(default)]
    pub page_state: JsonValue,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub auth_subject: Option<String>,
    #[serde(default)]
    pub user_claims: std::collections::HashMap<String, JsonValue>,
}

/// Response envelope for `POST /api/v1/ui/resolve`. Serialised
/// untagged — a successful resolve carries `{render, subscriptions,
/// meta}`; a dry-run carries `{errors}`. Mirrors the server's
/// `ResolveResponse`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum UiResolveResponse {
    Ok {
        render: UiRenderTree,
        subscriptions: Vec<UiSubscriptionPlan>,
        meta: UiResolveMeta,
    },
    DryRun {
        errors: Vec<UiResolveIssue>,
    },
}

/// Per-widget subscription plan — mirrors the server's `SubscriptionPlan`.
/// Subjects follow the `node.<id>.slot.<name>` convention.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiSubscriptionPlan {
    pub widget_id: String,
    pub subjects: Vec<String>,
    pub debounce_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiRenderTree {
    pub page_id: String,
    pub title: Option<String>,
    pub widgets: Vec<UiRenderedWidget>,
}

/// Tagged by `"kind"` — `"ui.widget"` (rendered), `"ui.widget.forbidden"`,
/// or `"ui.widget.dangling"`. Mirrors the server's `RenderedWidget`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum UiRenderedWidget {
    #[serde(rename = "ui.widget")]
    Rendered {
        id: String,
        widget_type: String,
        values: std::collections::HashMap<String, JsonValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout_hint: Option<JsonValue>,
    },
    #[serde(rename = "ui.widget.forbidden")]
    Forbidden { id: String, reason: String },
    #[serde(rename = "ui.widget.dangling")]
    Dangling { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiResolveMeta {
    pub cache_key: u64,
    pub widget_count: usize,
    pub forbidden_count: usize,
    pub dangling_count: usize,
    pub stack_shadowed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiResolveIssue {
    pub location: String,
    pub message: String,
}

// ---- auth -----------------------------------------------------------------

/// Mirror of `GET /api/v1/auth/whoami`. Field order must match the
/// server-side DTO in `crates/transport-rest/src/auth_routes.rs`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WhoAmIDto {
    /// `"user" | "machine" | "dev_null"`.
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub actor_display: String,
    pub tenant: String,
    /// Scopes the actor holds — snake_case strings matching
    /// `spi::Scope`'s `rename_all = "snake_case"`.
    pub scopes: Vec<String>,
    /// Provider id, e.g. `"dev_null"`, `"static_token"`.
    pub provider: String,
}

// ---- flows ----------------------------------------------------------------

/// Mirror of `GET /api/v1/flows` and `GET /api/v1/flows/:id`.
///
/// `document` is the raw JSON flow payload — opaque to the client.
/// `head_revision_id` is `null` only on a brand-new flow before any
/// edit has been committed (race-safe: it is set by the first
/// `append_revision` call inside the same transaction as `save_flow`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FlowDto {
    pub id: String,
    pub name: String,
    pub document: JsonValue,
    pub head_revision_id: Option<String>,
    pub head_seq: i64,
}

/// Mirror of `GET /api/v1/flows/:id/revisions` entries.
///
/// `op` is one of `create | edit | undo | redo | revert | import |
/// duplicate | paste` — stable strings defined in `RevisionOp`.
/// `target_rev_id` is non-null only for `undo`, `redo`, and `revert`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FlowRevisionDto {
    pub id: String,
    pub flow_id: String,
    pub parent_id: Option<String>,
    pub seq: i64,
    pub author: String,
    pub op: String,
    pub target_rev_id: Option<String>,
    pub summary: String,
    pub created_at: String,
}

/// Returned by every mutating flow endpoint (`edit`, `undo`, `redo`,
/// `revert`). Contains the new `head_revision_id` after the operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FlowMutationResult {
    pub head_revision_id: String,
}
