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

// ---- slot history (structured: String / Json / Binary) --------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HistoryRecord {
    pub id: i64,
    pub node_id: String,
    pub slot_name: String,
    pub slot_kind: String,
    pub ts_ms: i64,
    /// Decoded value for String/Json records; `null` for Binary.
    pub value: Option<JsonValue>,
    pub byte_size: i64,
    pub ntp_synced: bool,
    #[serde(default)]
    pub last_sync_age_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HistoryResponse {
    pub data: Vec<HistoryRecord>,
}

// ---- telemetry (scalar: Bool / Number) ------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScalarRecord {
    pub node_id: String,
    pub slot_name: String,
    pub ts_ms: i64,
    pub value: JsonValue,
    pub ntp_synced: bool,
    #[serde(default)]
    pub last_sync_age_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TelemetryResponse {
    pub data: Vec<ScalarRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecordedResponse {
    pub recorded: bool,
    pub kind: String,
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
    /// SDUI component IR version supported by the agent.
    #[serde(default)]
    pub ir_version: u32,
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

/// Primitive value kind — mirrors `spi::SlotValueKind`. Drives
/// historizer table routing: `Bool`/`Number` → time-series tables,
/// others → `slot_history`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SlotValueKind {
    #[default]
    Null,
    Bool,
    Number,
    String,
    Json,
    Binary,
}

/// Slot shape declared by a kind — mirrors `spi::SlotSchema`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SlotSchema {
    pub name: String,
    pub role: SlotRole,
    #[serde(default)]
    pub value_kind: SlotValueKind,
    #[serde(default)]
    pub value_schema: JsonValue,
    #[serde(default)]
    pub writable: bool,
    #[serde(default)]
    pub trigger: bool,
    #[serde(default)]
    pub is_internal: bool,
    #[serde(default)]
    pub emit_on_init: bool,
}

/// Wire shape for `GET /api/v1/node/schema` — a single node's kind-
/// declared slots. Answers "what slots does this node have?" in one
/// call, without cross-referencing `/kinds`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeSchema {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub slots: Vec<SlotSchema>,
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

// ---- blocks --------------------------------------------------------------

/// Block lifecycle state — `snake_case` on the wire.
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

/// Summary of a loaded block — mirrors `blocks_host::LoadedPluginSummary`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginSummary {
    /// Block identifier (transparent string).
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

/// Response from block enable / disable / reload actions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginActionResponse {
    pub id: String,
    pub lifecycle: PluginLifecycle,
}

/// Runtime state of a process block — mirrors
/// `blocks_host::PluginRuntimeState`. Internally tagged on
/// `status` (server-side `#[serde(tag = "status", rename_all =
/// "snake_case")]`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PluginRuntimeState {
    /// Supervisor hasn't spawned the child yet (disabled or just
    /// attached).
    Idle,
    /// Spawning child / awaiting UDS / performing `Describe`.
    Starting,
    /// `Describe` succeeded; last `Health` reported `READY`.
    Ready,
    /// Last `Health` returned `DEGRADED` with a detail string.
    Degraded { detail: String },
    /// Crashed or health failed — sleeping before the next spawn.
    Restarting {
        attempt: u32,
        backoff_ms: u64,
        reason: String,
    },
    /// Circuit-broken after repeated failures in the window. Needs
    /// operator re-enable.
    Failed { reason: String },
    /// Shut down cleanly.
    Stopped,
}

/// One entry of `GET /api/v1/blocks/runtime`. The `state` is
/// flattened into the object so the wire shape is
/// `{"id":"…","status":"ready"}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRuntimeEntry {
    pub id: String,
    #[serde(flatten)]
    pub state: PluginRuntimeState,
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
    /// Candidate layout JSON to validate in place of the node's
    /// persisted `layout` slot. Only honoured when `dry_run` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<JsonValue>,
}

/// Response envelope for `POST /api/v1/ui/resolve`. Serialised
/// untagged — a successful resolve carries `{render, subscriptions,
/// meta}`; a dry-run carries `{errors}`. Mirrors the server's
/// `ResolveResponse`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum UiResolveResponse {
    Ok {
        render: UiComponentTree,
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

/// Root of a resolved component tree. Mirrors `ui_ir::ComponentTree`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiComponentTree {
    pub ir_version: u32,
    pub root: UiComponent,
    /// Author-declared constants; referenced from bindings via
    /// `{{$vars.<key>}}`.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub vars: std::collections::HashMap<String, JsonValue>,
}

/// A single component in the IR tree. Discriminated by `"type"`.
/// Mirrors `ui_ir::Component`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiComponent {
    // layout
    Page {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        children: Vec<UiComponent>,
    },
    Row {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        children: Vec<UiComponent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gap: Option<String>,
    },
    Col {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        children: Vec<UiComponent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gap: Option<String>,
    },
    Grid {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        children: Vec<UiComponent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        columns: Option<String>,
    },
    Tabs {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        tabs: Vec<UiTab>,
    },
    // display
    Text {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
    },
    Heading {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        level: Option<u8>,
    },
    Badge {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
    },
    Diff {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        old_text: String,
        new_text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<UiDiffAnnotation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        line_action: Option<UiAction>,
    },
    // data
    Chart {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        source: UiChartSource,
        #[serde(default)]
        series: Vec<UiChartSeries>,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<UiChartRange>,
        #[serde(skip_serializing_if = "Option::is_none")]
        page_state_key: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    Sparkline {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        values: Vec<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        subscribe: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
    },
    Table {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        source: UiTableSource,
        columns: Vec<UiTableColumn>,
        #[serde(skip_serializing_if = "Option::is_none")]
        row_action: Option<UiAction>,
        #[serde(skip_serializing_if = "Option::is_none")]
        page_size: Option<u32>,
    },
    Tree {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        nodes: Vec<UiTreeItem>,
        #[serde(skip_serializing_if = "Option::is_none")]
        node_action: Option<UiAction>,
    },
    Timeline {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        events: Vec<UiTimelineEvent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        subscribe: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
    },
    Markdown {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        subscribe: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
    },
    // input
    RichText {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    RefPicker {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    Wizard {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        steps: Vec<UiWizardStep>,
        #[serde(skip_serializing_if = "Option::is_none")]
        submit: Option<UiAction>,
    },
    DateRange {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        page_state_key: String,
        presets: Vec<UiDateRangePreset>,
    },
    Select {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        page_state_key: String,
        options: Vec<UiSelectOption>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<JsonValue>,
    },
    Kpi {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        label: String,
        source: UiChartSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
    },
    Drawer {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        open: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        page_state_key: Option<String>,
        #[serde(default)]
        children: Vec<UiComponent>,
    },
    // interactive
    Button {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        disabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<UiAction>,
    },
    // composite
    Form {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        schema_ref: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        bindings: Option<JsonValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        submit: Option<UiAction>,
    },
    // placeholder stubs
    Forbidden {
        id: String,
        reason: String,
    },
    Dangling {
        id: String,
    },
    // escape hatch
    Custom {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        renderer_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        props: Option<JsonValue>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        subscribe: Vec<String>,
    },
}

/// Action reference — mirrors `ui_ir::Action`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiAction {
    pub handler: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimistic: Option<UiOptimisticHint>,
}

/// Optimistic hint — mirrors `ui_ir::OptimisticHint`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiOptimisticHint {
    pub target_component_id: String,
    pub fields: JsonValue,
}

/// Chart data source — mirrors `ui_ir::ChartSource`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiChartSource {
    pub node_id: String,
    pub slot: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// One chart series — mirrors `ui_ir::ChartSeries`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiChartSeries {
    pub label: String,
    #[serde(default)]
    pub points: Vec<(i64, f64)>,
}

/// Chart range — mirrors `ui_ir::ChartRange`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct UiChartRange {
    pub from: i64,
    pub to: i64,
}

/// Table data source — mirrors `ui_ir::TableSource`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiTableSource {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
}

/// Table column — mirrors `ui_ir::TableColumn`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiTableColumn {
    pub title: String,
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sortable: Option<bool>,
}

/// Diff annotation — mirrors `ui_ir::DiffAnnotation`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiDiffAnnotation {
    pub line: u32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Tree node — mirrors `ui_ir::TreeItem`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiTreeItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub children: Vec<UiTreeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Timeline event — mirrors `ui_ir::TimelineEvent`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiTimelineEvent {
    pub ts: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
}

/// Wizard step — mirrors `ui_ir::WizardStep`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiWizardStep {
    pub label: String,
    #[serde(default)]
    pub children: Vec<UiComponent>,
}

/// Date-range preset — mirrors `ui_ir::DateRangePreset`. A `None`
/// `duration_ms` means "all time / unbounded."
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiDateRangePreset {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

/// One option in a `Select`. `value` can be any JSON scalar.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiSelectOption {
    pub label: String,
    pub value: JsonValue,
}

/// Tab entry — mirrors `ui_ir::Tab`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiTab {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub label: String,
    #[serde(default)]
    pub children: Vec<UiComponent>,
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

/// Request body for `POST /api/v1/ui/compose`. Agent-side AI authoring.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiComposeRequest {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_layout: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_hints: Option<String>,
}

/// Response from `POST /api/v1/ui/compose` — the generated
/// ComponentTree plus any free-text the model wrote alongside.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiComposeResponse {
    pub layout: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Response from `GET /api/v1/ui/vocabulary` — the JSON Schema of the
/// `ui_ir::Component` union. Consumed by Monaco, Studio's palette, and
/// LLM authors so they can discover the full component vocabulary from
/// a single endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiVocabulary {
    pub ir_version: u32,
    pub schema: JsonValue,
}

// ---- ui table ---------------------------------------------------------------

/// Query params for `GET /api/v1/ui/table`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct UiTableParams {
    /// Base RSQL query string from `TableSource.query`.
    #[serde(default)]
    pub query: String,
    /// Additional client-side RSQL filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<usize>,
    /// Optional table component id for audit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

/// A single node row returned by `GET /api/v1/ui/table`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiTableRow {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub parent_id: Option<String>,
    pub slots: std::collections::HashMap<String, JsonValue>,
}

/// Pagination metadata.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiTableMeta {
    pub total: usize,
    pub page: usize,
    pub size: usize,
    pub pages: usize,
}

/// Response from `GET /api/v1/ui/table`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiTableResponse {
    pub data: Vec<UiTableRow>,
    pub meta: UiTableMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiActionContext {
    /// Component id that fired the action (button, form row, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Ordered nav-node ids forming the breadcrumb stack.
    #[serde(default)]
    pub stack: Vec<String>,
    /// Page-local state at the moment the action fired.
    #[serde(default)]
    pub page_state: JsonValue,
    /// Opaque auth subject identifier threaded through for audit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_subject: Option<String>,
}

impl Default for UiActionContext {
    fn default() -> Self {
        Self {
            target: None,
            stack: vec![],
            page_state: JsonValue::Object(Default::default()),
            auth_subject: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiActionRequest {
    pub handler: String,
    #[serde(default)]
    pub args: JsonValue,
    #[serde(default)]
    pub context: UiActionContext,
}

/// Tagged-union response from `POST /api/v1/ui/action`.
///
/// The `type` field discriminates the variant:
/// `patch | navigate | full_render | toast | form_errors | download | stream | none`
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiActionResponse {
    /// Replace a single subtree in the current render tree.
    Patch {
        target_component_id: String,
        tree: UiComponentTree,
    },
    /// Client-side navigation.
    Navigate { to: UiNavigateTo },
    /// Replace the full page render tree.
    FullRender { tree: UiComponentTree },
    /// Show a transient notification.
    Toast { intent: String, message: String },
    /// Attach field-level validation errors to the originating form.
    FormErrors {
        errors: std::collections::HashMap<String, String>,
    },
    /// Trigger a file download from the given URL.
    Download { url: String },
    /// Long-running response — client subscribes to the given channel.
    Stream { channel: String },
    /// No-op — action succeeded but the UI does not need to change.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UiNavigateTo {
    pub target_ref: String,
}

// ---- auth ----------------------------------------------------------------- Field order must match the
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
