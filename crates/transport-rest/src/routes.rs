//! Request handlers.

use std::convert::Infallible;
use std::str::FromStr;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use graph::{Lifecycle, LinkId, NodeDto, SlotRef};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::{AuthContext, KindId, NodeId, NodePath, Scope};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

use crate::capabilities::host_capabilities;
use crate::seed::{self, Preset, SeedResult};
use crate::state::AppState;
use crate::ui;

/// Public REST surface — versioned via URI prefix per
/// `docs/design/VERSIONING.md` § "Public API". Bumping to `/api/v2/`
/// requires a 12-month deprecation window for `/api/v1/`.
pub const API_PREFIX: &str = "/api/v1";

pub fn mount(state: AppState) -> Router {
    Router::new()
        // Unversioned ops surface — `/healthz` is for orchestrators
        // (k8s, systemd) and intentionally outside the API contract.
        .route("/healthz", get(healthz))
        // Versioned API.
        .route("/api/v1/capabilities", get(capabilities))
        // `POST /nodes` creates a node. Listing is served by
        // `GET /api/v1/search?scope=nodes` (see `crate::search`).
        .route("/api/v1/nodes", post(create_node))
        .route("/api/v1/node", get(get_node).delete(delete_node))
        .route("/api/v1/node/schema", get(get_node_schema))
        .route("/api/v1/slots", post(write_slot))
        .route("/api/v1/config", post(set_config))
        .route("/api/v1/events", get(stream_events))
        // Listing goes through `/api/v1/search?scope=links`; POST keeps
        // its dedicated route for the create.
        .route("/api/v1/links", post(create_link))
        .route("/api/v1/links/:id", delete(remove_link))
        .route("/api/v1/lifecycle", post(transition_lifecycle))
        .route("/api/v1/seed", post(seed_preset))
        // UI is unversioned — it's a tool, not a contract.
        .route("/", get(ui::index))
        // Block REST + MF bundle serving — contributed by the blocks
        // module, merged in so the tower layers below apply uniformly.
        .merge(crate::ai::routes())
        .merge(crate::analyze::routes())
        .merge(crate::blocks::routes())
        .merge(crate::search::routes())
        .merge(crate::auth_routes::routes())
        .merge(crate::auth_setup::routes())
        .merge(crate::flows::routes())
        .merge(crate::preferences::routes())
        .merge(crate::units_route::routes())
        .merge(crate::history::routes())
        .merge(crate::users::routes())
        .merge(crate::backup::routes())
        // 503 gate for setup mode. No-op on non-setup roles and once
        // setup completes — the service self-reports via
        // `is_configured()`. Layered after `.merge(...)` so every
        // domain surface above is subject to the gate.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth_setup::gate_setup_mode,
        ))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn capabilities() -> Json<crate::capabilities::CapabilityManifest> {
    Json(host_capabilities())
}

async fn healthz() -> &'static str {
    "ok"
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

#[derive(Deserialize)]
struct GetNodeQuery {
    path: String,
    /// When true, bookkeeping slots marked `is_internal` in the kind
    /// manifest are included in the response. Default: false —
    /// Studio's default node card only renders user-facing slots.
    #[serde(default)]
    include_internal: bool,
}

pub(crate) fn get_node_core(
    state: &AppState,
    path_raw: &str,
    include_internal: bool,
) -> Result<NodeDto, ApiError> {
    let path = parse_path(path_raw)?;
    let snap = state
        .graph
        .get(&path)
        .ok_or_else(|| ApiError::not_found(format!("no node at `{path}`")))?;
    // Fetch the manifest before building the DTO so slot-level
    // `quantity`/`unit` metadata travels on every slot in the
    // response — clients then format values using
    // user-preference-driven conversion without a second round-trip.
    let kind = KindId::new(snap.kind.as_str());
    let manifest = state.graph.kinds().get(&kind);
    let mut dto = NodeDto::from_snapshot(snap, manifest.as_ref());
    if !include_internal {
        if let Some(manifest) = manifest.as_ref() {
            let internal: std::collections::HashSet<&str> = manifest
                .slots
                .iter()
                .filter(|s| s.is_internal)
                .map(|s| s.name.as_str())
                .collect();
            dto.slots.retain(|s| !internal.contains(s.name.as_str()));
        }
    }
    Ok(dto)
}

async fn get_node(
    State(s): State<AppState>,
    Query(q): Query<GetNodeQuery>,
) -> Result<Json<NodeDto>, ApiError> {
    get_node_core(&s, &q.path, q.include_internal).map(Json)
}

/// Wire shape of `GET /api/v1/node/schema` — the kind-declared slot
/// schemas for one node. Lets clients answer "what slots does this
/// node have?" without cross-referencing `/kinds`. See
/// [`docs/design/NEW-API.md`] for the contract rules.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct NodeSchemaDto {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub slots: Vec<SlotSchemaDto>,
}

/// One slot's manifest declaration — mirrors `spi::SlotSchema` exactly.
/// Kept here (not reusing `spi::SlotSchema` directly) so the wire shape
/// is an independent contract the clients mirror field-for-field.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct SlotSchemaDto {
    pub name: String,
    pub role: spi::SlotRole,
    pub value_kind: spi::SlotValueKind,
    pub value_schema: JsonValue,
    pub writable: bool,
    pub trigger: bool,
    pub is_internal: bool,
    pub emit_on_init: bool,
    /// Physical quantity declared on the slot, if any. Absent for
    /// dimensionless slots. Clients use this together with
    /// [`SlotSchemaDto::unit`] to drive unit-picker UIs and
    /// preference-aware rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<spi::Quantity>,
    /// Unit the stored value is expressed in — either the quantity's
    /// canonical unit (normal case) or the declared `unit` override
    /// (opt-out from ingest-time conversion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_unit: Option<spi::Unit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<spi::Unit>,
}

impl From<&spi::SlotSchema> for SlotSchemaDto {
    fn from(s: &spi::SlotSchema) -> Self {
        Self {
            name: s.name.clone(),
            role: s.role,
            value_kind: s.value_kind,
            value_schema: s.value_schema.clone(),
            writable: s.writable,
            trigger: s.trigger,
            is_internal: s.is_internal,
            emit_on_init: s.emit_on_init,
            quantity: s.quantity,
            sensor_unit: s.sensor_unit,
            unit: s.unit,
        }
    }
}

#[derive(Deserialize)]
struct GetNodeSchemaQuery {
    path: String,
    /// When true, slots marked `is_internal` in the kind manifest are
    /// included in the response. Default: false, matching `/node`'s
    /// default. Authors debugging internal state flip this.
    #[serde(default)]
    include_internal: bool,
}

pub(crate) fn get_node_schema_core(
    state: &AppState,
    path_raw: &str,
    include_internal: bool,
) -> Result<NodeSchemaDto, ApiError> {
    let path = parse_path(path_raw)?;
    let snap = state
        .graph
        .get(&path)
        .ok_or_else(|| ApiError::not_found(format!("no node at `{path}`")))?;
    let manifest =
        state.graph.kinds().get(&snap.kind).ok_or_else(|| {
            ApiError::bad_request(format!("kind `{}` is not registered", snap.kind))
        })?;
    let slots: Vec<SlotSchemaDto> = manifest
        .slots
        .iter()
        .filter(|s| include_internal || !s.is_internal)
        .map(SlotSchemaDto::from)
        .collect();
    Ok(NodeSchemaDto {
        id: snap.id.to_string(),
        kind: snap.kind.as_str().to_string(),
        path: snap.path.to_string(),
        slots,
    })
}

async fn get_node_schema(
    State(s): State<AppState>,
    Query(q): Query<GetNodeSchemaQuery>,
) -> Result<Json<NodeSchemaDto>, ApiError> {
    get_node_schema_core(&s, &q.path, q.include_internal).map(Json)
}

async fn delete_node(
    State(s): State<AppState>,
    Query(q): Query<PathQuery>,
) -> Result<StatusCode, ApiError> {
    let path = parse_path(&q.path)?;
    s.graph.delete(&path).map_err(ApiError::from_graph)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CreateNodeReq {
    parent: String,
    kind: String,
    name: String,
}

#[derive(Serialize)]
struct CreatedNodeResp {
    id: String,
    path: String,
}

async fn create_node(
    State(s): State<AppState>,
    Json(req): Json<CreateNodeReq>,
) -> Result<Json<CreatedNodeResp>, ApiError> {
    let parent = parse_path(&req.parent)?;
    let kind = KindId::new(req.kind);
    let (id, actual_name) = s
        .graph
        .create_child_unique(&parent, kind, &req.name)
        .map_err(ApiError::from_graph)?;
    Ok(Json(CreatedNodeResp {
        id: id.to_string(),
        path: parent.child(&actual_name).to_string(),
    }))
}

#[derive(Deserialize)]
pub(crate) struct WriteSlotReq {
    path: String,
    slot: String,
    value: JsonValue,
    #[serde(default)]
    expected_generation: Option<u64>,
}

#[derive(Serialize)]
struct WriteSlotResp {
    generation: u64,
}

/// Result type returned by [`write_slot_core`].
///
/// Serialised as a tagged JSON object so the fleet handler can embed
/// the outcome in a reply payload without introducing HTTP status codes
/// on the fleet path. The `status` discriminant mirrors the two
/// meaningful outcomes of a `write_slot` call:
///
/// - `{ "status": "ok", "generation": N }` — write committed.
/// - `{ "status": "generation_mismatch", "current_generation": N }` — CAS
///   conflict; caller should re-read and retry.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum WriteSlotResult {
    Ok { generation: u64 },
    GenerationMismatch { current_generation: u64 },
}

/// Core logic for `POST /api/v1/slots`. Shared by the axum handler and
/// the fleet `api.v1.slots.write` handler — one function, two surfaces.
///
/// Goes through the **tenant-facing** graph API so the manifest's
/// `writable: false` guard is enforced. Bootstrappers and engine
/// writes that legitimately populate `writable: false` status slots
/// (e.g. `/agent/setup.status`, `/agent/fleet.connection`) call
/// `GraphStore::write_slot` directly and bypass this path.
///
/// Auth is the caller's responsibility (the axum handler checks
/// `Scope::WriteSlots`; the fleet handler validates the bearer token
/// forwarded as a message header).
pub(crate) fn write_slot_core(
    state: &AppState,
    req: WriteSlotReq,
) -> Result<WriteSlotResult, ApiError> {
    let path = parse_path(&req.path)?;
    let opts = match req.expected_generation {
        Some(expected) => graph::WriteSlotOpts::tenant_expected(expected),
        None => graph::WriteSlotOpts::tenant(),
    };
    let result = state
        .graph
        .write_slot_with(&path, &req.slot, req.value, opts);
    match result {
        Ok(gen) => Ok(WriteSlotResult::Ok { generation: gen }),
        Err(graph::GraphError::GenerationMismatch { current, .. }) => {
            Ok(WriteSlotResult::GenerationMismatch {
                current_generation: current,
            })
        }
        Err(e) => Err(ApiError::from_graph(e)),
    }
}

async fn write_slot(
    ctx: AuthContext,
    State(s): State<AppState>,
    Json(req): Json<WriteSlotReq>,
) -> Result<Response, ApiError> {
    ctx.require(Scope::WriteSlots)
        .map_err(ApiError::from_auth)?;
    match write_slot_core(&s, req)? {
        WriteSlotResult::Ok { generation } => {
            Ok(Json(WriteSlotResp { generation }).into_response())
        }
        WriteSlotResult::GenerationMismatch { current_generation } => Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "generation_mismatch",
                "current_generation": current_generation,
            })),
        )
            .into_response()),
    }
}

#[derive(Deserialize)]
struct SetConfigReq {
    path: String,
    config: JsonValue,
}

async fn set_config(
    State(s): State<AppState>,
    Json(req): Json<SetConfigReq>,
) -> Result<StatusCode, ApiError> {
    let path = parse_path(&req.path)?;
    let id = s
        .graph
        .get(&path)
        .ok_or_else(|| ApiError::not_found(format!("no node at `{path}`")))?
        .id;
    // Settings now live as a config-role slot on the node itself;
    // `set_config` writes it through the graph store, so the change
    // persists and fires `SlotChanged` for every subscriber.
    s.behaviors
        .set_config(id, req.config)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    // Re-run on_init so the new settings take effect. Idempotent for
    // well-behaved behaviours (count seeds the slot; trigger resets
    // armed/pending_timer).
    s.behaviors
        .dispatch_init(id)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stream_events(State(s): State<AppState>, Query(q): Query<EventsQuery>) -> Response {
    // Determine replay set and current seq, or return 409 if cursor is
    // below the ring's floor.
    let (replay, current_seq) = if let Some(since) = q.since {
        match s.ring.since(since) {
            Err(available_from) => {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": "cursor_too_old",
                        "available_from": available_from
                    })),
                )
                    .into_response();
            }
            Ok(events) => (events, s.ring.current_seq()),
        }
    } else {
        (Vec::new(), s.ring.current_seq())
    };

    // Subscribe to live events *before* building the replay prefix so
    // we don't miss events that arrive between ring-read and subscribe.
    let rx = s.events.subscribe();

    // First frame is always `hello { seq }` so the client knows where
    // "now" is without having consumed any events yet.
    let hello_frame: Result<Event, Infallible> = {
        let f = crate::event::HelloFrame::new(current_seq);
        Ok(Event::default().json_data(&f).unwrap_or_default())
    };

    // Replay prefix (empty on fresh connect).
    let replay_frames = replay
        .into_iter()
        .map(|ev| Ok(Event::default().json_data(&ev).unwrap_or_default()));

    // Live stream — slow consumers drop frames (broadcast semantics).
    let live = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => Some(Ok(Event::default().json_data(&ev).unwrap_or_default())),
        Err(_lag) => None,
    });

    let stream = futures_util::stream::once(futures_util::future::ready(hello_frame))
        .chain(futures_util::stream::iter(replay_frames))
        .chain(live);

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

pub(crate) fn parse_path(s: &str) -> Result<NodePath, ApiError> {
    NodePath::from_str(s).map_err(|e| ApiError::bad_request(format!("bad path `{s}`: {e}")))
}

#[derive(Debug, Default, Deserialize)]
struct EventsQuery {
    /// Resume from this sequence number — reply with events whose `seq > since`.
    /// Omit for a fresh connection.
    since: Option<u64>,
}

// ---- links ----------------------------------------------------------------
//
// `LinkDto` / `EndpointDto` / listing logic live in `graph::links`
// (consumed by every transport via `/api/v1/search?scope=links`).
// This module keeps only the create/delete routes.

/// Endpoint addressed by path for ergonomics; `node_id` accepted as a
/// fallback when the caller only has ids (e.g. event-log consumers).
#[derive(Deserialize)]
struct EndpointReq {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    node_id: Option<String>,
    slot: String,
}

#[derive(Deserialize)]
struct CreateLinkReq {
    source: EndpointReq,
    target: EndpointReq,
}

fn resolve_endpoint(s: &AppState, e: &EndpointReq) -> Result<SlotRef, ApiError> {
    let node_id = match (&e.path, &e.node_id) {
        (Some(p), _) => {
            let path = parse_path(p)?;
            s.graph
                .get(&path)
                .ok_or_else(|| ApiError::not_found(format!("no node at `{path}`")))?
                .id
        }
        (None, Some(raw)) => {
            let uuid = Uuid::parse_str(raw)
                .map_err(|e| ApiError::bad_request(format!("bad node_id `{raw}`: {e}")))?;
            NodeId(uuid)
        }
        (None, None) => {
            return Err(ApiError::bad_request(
                "endpoint requires `path` or `node_id`",
            ));
        }
    };
    Ok(SlotRef::new(node_id, e.slot.clone()))
}

async fn create_link(
    State(s): State<AppState>,
    Json(req): Json<CreateLinkReq>,
) -> Result<Json<CreatedLinkResp>, ApiError> {
    let source = resolve_endpoint(&s, &req.source)?;
    let target = resolve_endpoint(&s, &req.target)?;
    let id = s
        .graph
        .add_link(source, target)
        .map_err(ApiError::from_graph)?;
    Ok(Json(CreatedLinkResp { id: id.to_string() }))
}

#[derive(Serialize)]
struct CreatedLinkResp {
    id: String,
}

async fn remove_link(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|e| ApiError::bad_request(format!("bad link id `{id}`: {e}")))?;
    s.graph
        .remove_link(LinkId(uuid))
        .map_err(ApiError::from_graph)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- lifecycle ------------------------------------------------------------

#[derive(Deserialize)]
struct LifecycleReq {
    path: String,
    to: Lifecycle,
}

#[derive(Serialize)]
struct LifecycleResp {
    path: String,
    to: Lifecycle,
}

async fn transition_lifecycle(
    State(s): State<AppState>,
    Json(req): Json<LifecycleReq>,
) -> Result<Json<LifecycleResp>, ApiError> {
    let path = parse_path(&req.path)?;
    let to = s
        .graph
        .transition(&path, req.to)
        .map_err(ApiError::from_graph)?;
    Ok(Json(LifecycleResp {
        path: path.to_string(),
        to,
    }))
}

// ---- seed -----------------------------------------------------------------

#[derive(Deserialize)]
struct SeedReq {
    preset: Preset,
}

async fn seed_preset(
    State(s): State<AppState>,
    Json(req): Json<SeedReq>,
) -> Result<Json<SeedResult>, ApiError> {
    let result = seed::apply(&s, req.preset)?;
    Ok(Json(result))
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiError {
    #[serde(skip)]
    pub(crate) status: StatusCode,
    pub(crate) error: String,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, msg: impl Into<String>) -> Self {
        Self {
            status,
            error: msg.into(),
        }
    }

    pub(crate) fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: msg.into(),
        }
    }

    pub(crate) fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error: msg.into(),
        }
    }

    pub(crate) fn from_graph(err: graph::GraphError) -> Self {
        let status = match &err {
            // Not-a-valid-tenant-op: the slot exists and is well-formed
            // but the manifest declares it non-writable. 403 matches
            // the semantic of "you don't have the right to mutate this
            // on the tenant surface" even though the auth context
            // itself is fine.
            graph::GraphError::SlotNotWritable { .. } => StatusCode::FORBIDDEN,
            _ => StatusCode::BAD_REQUEST,
        };
        Self {
            status,
            error: err.to_string(),
        }
    }

    /// Map an `AuthError` from a scope check. The extractor-level
    /// failures (missing / invalid credentials) are surfaced by the
    /// `AuthErrorResponse` wrapper before a handler runs; this helper
    /// covers the `ctx.require(Scope::X)?` path inside a handler body.
    pub(crate) fn from_auth(err: spi::AuthError) -> Self {
        let status = match &err {
            spi::AuthError::MissingCredentials | spi::AuthError::InvalidCredentials { .. } => {
                StatusCode::UNAUTHORIZED
            }
            spi::AuthError::MissingScope { .. } | spi::AuthError::WrongTenant => {
                StatusCode::FORBIDDEN
            }
            spi::AuthError::Provider(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self {
            status,
            error: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blocks_host::BlockRegistry;
    use engine::BehaviorRegistry;
    use graph::{seed, GraphStore, KindRegistry};
    use tokio::sync::broadcast;

    use super::*;

    fn test_state() -> AppState {
        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(graph::NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("sys.core.folder"), "alpha")
            .unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("sys.core.folder"), "beta")
            .unwrap();
        let (behaviors, _timers) = BehaviorRegistry::new(graph.clone());
        let (events, _) = broadcast::channel(16);
        AppState::new(graph, behaviors, events, BlockRegistry::new())
    }

    // List-nodes behaviour lives in `graph::nodes::scope::tests` — the
    // transport no longer owns that logic after the /search migration.

    #[tokio::test]
    async fn get_node_filters_internal_slots_by_default() {
        use spi::{
            Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindManifest, SlotRole,
            SlotSchema,
        };

        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        kinds.register(KindManifest {
            id: KindId::new("test.widget"),
            display_name: None,
            facets: FacetSet::of([Facet::IsCompute]),
            containment: ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
            slots: vec![
                SlotSchema::new("out", SlotRole::Output).writable(),
                SlotSchema::new("pending_timer", SlotRole::Status)
                    .writable()
                    .internal(),
            ],
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        });
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(graph::NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("test.widget"), "w1")
            .unwrap();
        let path = NodePath::root().child("w1");
        graph
            .write_slot(&path, "out", serde_json::json!({"payload": 1}))
            .unwrap();
        graph
            .write_slot(&path, "pending_timer", serde_json::json!(42))
            .unwrap();
        let (behaviors, _timers) = BehaviorRegistry::new(graph.clone());
        let (events, _) = broadcast::channel(16);
        let state = AppState::new(graph, behaviors, events, BlockRegistry::new());

        let default = get_node_core(&state, "/w1", false).unwrap();
        let names: Vec<&str> = default.slots.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"out"), "out should be visible: {names:?}");
        assert!(
            !names.contains(&"pending_timer"),
            "pending_timer should be hidden by default: {names:?}"
        );

        let with_internal = get_node_core(&state, "/w1", true).unwrap();
        let names: Vec<&str> = with_internal
            .slots
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"pending_timer"), "got {names:?}");
    }

    #[tokio::test]
    async fn get_node_schema_returns_kind_declared_slots() {
        use spi::{
            Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindManifest, SlotRole,
            SlotSchema,
        };

        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        kinds.register(KindManifest {
            id: KindId::new("test.widget"),
            display_name: None,
            facets: FacetSet::of([Facet::IsCompute]),
            containment: ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
            slots: vec![
                SlotSchema::new("out", SlotRole::Output).writable(),
                SlotSchema::new("pending_timer", SlotRole::Status)
                    .writable()
                    .internal(),
            ],
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        });
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(graph::NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("test.widget"), "w1")
            .unwrap();
        let (behaviors, _timers) = BehaviorRegistry::new(graph.clone());
        let (events, _) = broadcast::channel(16);
        let state = AppState::new(graph, behaviors, events, BlockRegistry::new());

        // Default: internal slots filtered out. (The registry also
        // injects synthesised canvas slots `position` / `notes`; we
        // assert on the slots the test author declared.)
        let dto = get_node_schema_core(&state, "/w1", false).unwrap();
        assert_eq!(dto.kind, "test.widget");
        assert_eq!(dto.path, "/w1");
        let names: Vec<&str> = dto.slots.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"out"), "got {names:?}");
        assert!(
            !names.contains(&"pending_timer"),
            "internal slot leaked: {names:?}"
        );
        let out = dto.slots.iter().find(|s| s.name == "out").unwrap();
        assert_eq!(out.role, spi::SlotRole::Output);
        assert!(out.writable);

        // include_internal=true reveals the bookkeeping slot.
        let full = get_node_schema_core(&state, "/w1", true).unwrap();
        let names: Vec<&str> = full.slots.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"pending_timer"), "got {names:?}");

        // 404 for missing node.
        let err = get_node_schema_core(&state, "/nope", false).unwrap_err();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
    }

    // `list_nodes_rejects_unknown_query_fields` lives in
    // `graph::nodes::scope::tests::rejects_unknown_rsql_field` now.

    /// Phase 2 read path: a slot declared with `quantity: Temperature`
    /// + `sensor_unit: Fahrenheit` must surface both fields on the
    /// `GET /api/v1/node` response so clients (CLI, Studio, MCP)
    /// format with the right user preference without a second call.
    ///
    /// Also proves ingest conversion (tested in depth in
    /// `graph::tests::ingest_units`) — the stored value comes back as
    /// canonical (°C), never the raw °F.
    #[tokio::test]
    async fn get_node_returns_quantity_and_unit_metadata() {
        use spi::{
            Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindManifest, Quantity,
            SlotRole, SlotSchema, SlotValueKind, Unit,
        };

        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        kinds.register(KindManifest {
            id: KindId::new("test.thermometer"),
            display_name: None,
            // `IsAnywhere` keeps the fixture tight — no need to teach
            // the station's containment about a test kind.
            facets: FacetSet::of([Facet::IsAnywhere]),
            containment: ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
            slots: vec![
                SlotSchema::new("temp", SlotRole::Input)
                    .with_kind(SlotValueKind::Number)
                    .writable()
                    .with_quantity(Quantity::Temperature)
                    .with_sensor_unit(Unit::Fahrenheit),
                // Dimensionless sibling — must return quantity/unit as
                // absent, not as Null.
                SlotSchema::new("dimensionless", SlotRole::Status)
                    .with_kind(SlotValueKind::Number)
                    .writable(),
            ],
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        });
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(graph::NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("test.thermometer"), "t")
            .unwrap();
        let path = NodePath::root().child("t");
        // °F on the wire → °C in storage via `normalize_for_storage`.
        graph.write_slot(&path, "temp", serde_json::json!(72.4)).unwrap();
        graph
            .write_slot(&path, "dimensionless", serde_json::json!(7))
            .unwrap();

        let (behaviors, _timers) = BehaviorRegistry::new(graph.clone());
        let (events, _) = broadcast::channel(16);
        let state = AppState::new(graph, behaviors, events, BlockRegistry::new());

        let dto = get_node_core(&state, "/t", false).unwrap();
        let temp = dto.slots.iter().find(|s| s.name == "temp").unwrap();
        assert_eq!(temp.quantity, Some(spi::Quantity::Temperature));
        // Stored value is canonical (°C); `unit` on the DTO is the
        // declared override, which is `None` here (canonical is
        // implicit). Phase 3 serialisation middleware will add the
        // resolved canonical; for now clients read `/api/v1/units` to
        // fill the blank.
        assert_eq!(temp.unit, None);
        let stored = temp.value.as_f64().unwrap();
        assert!(
            (stored - 22.44).abs() < 0.01,
            "expected ~22.44 °C stored, got {stored}",
        );

        let d = dto.slots.iter().find(|s| s.name == "dimensionless").unwrap();
        assert_eq!(d.quantity, None);
        assert_eq!(d.unit, None);
    }

    /// `POST /api/v1/slots` must refuse writes to manifest-declared
    /// `writable: false` slots even when the caller holds
    /// `Scope::WriteSlots`. Guards the bootstrap surface:
    /// `/agent/setup.status` and `/agent/fleet.connection` are the
    /// concrete tenants of this rule.
    #[tokio::test]
    async fn write_slot_core_rejects_non_writable_slot_with_403() {
        use spi::{
            Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindManifest, SlotRole,
            SlotSchema, SlotValueKind,
        };

        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        kinds.register(KindManifest {
            id: KindId::new("test.status_only"),
            display_name: None,
            // `IsAnywhere` lets the test keep the container wiring one
            // line instead of teaching station containment.
            facets: FacetSet::of([Facet::IsAnywhere]),
            containment: ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
            // Status slot is deliberately `writable: false`.
            slots: vec![SlotSchema::new("status", SlotRole::Status)
                .with_kind(SlotValueKind::String)],
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        });
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(graph::NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("test.status_only"), "n")
            .unwrap();
        // Bootstrap-style seed via the internal API — succeeds.
        graph
            .write_slot(
                &NodePath::root().child("n"),
                "status",
                serde_json::json!("seeded"),
            )
            .unwrap();

        let (behaviors, _timers) = BehaviorRegistry::new(graph.clone());
        let (events, _) = broadcast::channel(16);
        let state = AppState::new(graph, behaviors, events, BlockRegistry::new());

        let req = WriteSlotReq {
            path: "/n".to_string(),
            slot: "status".to_string(),
            value: serde_json::json!("overridden"),
            expected_generation: None,
        };
        let err = write_slot_core(&state, req).unwrap_err();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert!(
            err.error.contains("writable: false"),
            "error should name the guard: {:?}",
            err.error,
        );
    }

    /// `writable: true` slots flow through normally — proves the new
    /// enforcement path is a guard, not a blanket refusal.
    #[tokio::test]
    async fn write_slot_core_permits_writable_true_slot() {
        use spi::{
            Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindManifest, SlotRole,
            SlotSchema, SlotValueKind,
        };

        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        kinds.register(KindManifest {
            id: KindId::new("test.value_only"),
            display_name: None,
            facets: FacetSet::of([Facet::IsAnywhere]),
            containment: ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
            slots: vec![SlotSchema::new("value", SlotRole::Input)
                .with_kind(SlotValueKind::Number)
                .writable()],
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        });
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(graph::NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("test.value_only"), "v")
            .unwrap();
        let (behaviors, _timers) = BehaviorRegistry::new(graph.clone());
        let (events, _) = broadcast::channel(16);
        let state = AppState::new(graph, behaviors, events, BlockRegistry::new());

        let req = WriteSlotReq {
            path: "/v".to_string(),
            slot: "value".to_string(),
            value: serde_json::json!(42),
            expected_generation: None,
        };
        let result = write_slot_core(&state, req).unwrap();
        match result {
            WriteSlotResult::Ok { generation: _ } => {} // passes
            WriteSlotResult::GenerationMismatch { .. } => panic!("unexpected generation mismatch"),
        }
    }
}
