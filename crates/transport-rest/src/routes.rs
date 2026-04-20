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
use graph::{Lifecycle, LinkId, NodeSnapshot, SlotRef};
use query::{FieldType, Operator, QueryRequest, QuerySchema, SortField};
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
        .route("/api/v1/nodes", get(list_nodes).post(create_node))
        .route("/api/v1/node", get(get_node).delete(delete_node))
        .route("/api/v1/slots", post(write_slot))
        .route("/api/v1/config", post(set_config))
        .route("/api/v1/events", get(stream_events))
        .route("/api/v1/links", get(list_links).post(create_link))
        .route("/api/v1/links/:id", delete(remove_link))
        .route("/api/v1/lifecycle", post(transition_lifecycle))
        .route("/api/v1/seed", post(seed_preset))
        // UI is unversioned — it's a tool, not a contract.
        .route("/", get(ui::index))
        // Block REST + MF bundle serving — contributed by the blocks
        // module, merged in so the tower layers below apply uniformly.
        .merge(crate::blocks::routes())
        .merge(crate::kinds::routes())
        .merge(crate::auth_routes::routes())
        .merge(crate::flows::routes())
        .merge(crate::preferences::routes())
        .merge(crate::history::routes())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn capabilities() -> Json<crate::capabilities::CapabilityManifest> {
    Json(host_capabilities())
}

async fn healthz() -> &'static str {
    "ok"
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NodeDto {
    id: String,
    kind: String,
    path: String,
    /// Materialised parent path (`"/"` for depth-1 nodes, `null` for the
    /// root). Exposed so tree UIs can filter direct children with
    /// `?filter=parent_path==/station/floor1` in a single query,
    /// without walking the full subtree + filtering client-side.
    parent_path: Option<String>,
    parent_id: Option<String>,
    /// Whether the node has at least one child. Computed server-side so
    /// tree UIs can show expand chevrons without a speculative child query.
    has_children: bool,
    lifecycle: Lifecycle,
    slots: Vec<SlotDto>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SlotDto {
    name: String,
    value: JsonValue,
    generation: u64,
}

impl From<NodeSnapshot> for NodeDto {
    fn from(s: NodeSnapshot) -> Self {
        Self {
            id: s.id.to_string(),
            kind: s.kind.as_str().to_string(),
            parent_path: s.path.parent().map(|p| p.to_string()),
            path: s.path.to_string(),
            parent_id: s.parent.map(|p| p.to_string()),
            has_children: s.has_children,
            lifecycle: s.lifecycle,
            slots: s
                .slot_values
                .into_iter()
                .map(|(name, sv)| SlotDto {
                    name,
                    value: sv.value,
                    generation: sv.generation,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct ListNodesQuery {
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub page: Option<usize>,
    pub size: Option<usize>,
}

fn node_query_schema() -> QuerySchema {
    QuerySchema::new(100, 1000)
        .field(
            "id",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix, Operator::In],
        )
        .field(
            "kind",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "path",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "parent_id",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "parent_path",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field("lifecycle", FieldType::Text, [Operator::Eq, Operator::Ne])
        .default_sort([SortField::asc("path")])
}

/// Core logic for `GET /api/v1/nodes`. Shared by the axum handler and
/// the fleet `api.v1.nodes.list` handler — one function, two surfaces.
pub(crate) fn list_nodes_core(
    state: &AppState,
    raw: ListNodesQuery,
) -> Result<query::Page<NodeDto>, ApiError> {
    let mut out: Vec<NodeDto> = state
        .graph
        .snapshots()
        .into_iter()
        .map(NodeDto::from)
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    let query = query::validate(
        &node_query_schema(),
        QueryRequest {
            filter: raw.filter,
            sort: raw.sort,
            page: raw.page,
            size: raw.size,
        },
    )
    .map_err(|e| ApiError::bad_request(e.to_string()))?;
    query::execute(out, &query).map_err(|e| ApiError::bad_request(e.to_string()))
}

async fn list_nodes(
    ctx: AuthContext,
    State(s): State<AppState>,
    Query(raw): Query<ListNodesQuery>,
) -> Result<Json<query::Page<NodeDto>>, ApiError> {
    ctx.require(Scope::ReadNodes).map_err(ApiError::from_auth)?;
    list_nodes_core(&s, raw).map(Json)
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

async fn get_node(
    State(s): State<AppState>,
    Query(q): Query<PathQuery>,
) -> Result<Json<NodeDto>, ApiError> {
    let path = parse_path(&q.path)?;
    let snap = s
        .graph
        .get(&path)
        .ok_or_else(|| ApiError::not_found(format!("no node at `{path}`")))?;
    Ok(Json(NodeDto::from(snap)))
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
struct WriteSlotReq {
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

async fn write_slot(
    ctx: AuthContext,
    State(s): State<AppState>,
    Json(req): Json<WriteSlotReq>,
) -> Result<Response, ApiError> {
    ctx.require(Scope::WriteSlots)
        .map_err(ApiError::from_auth)?;
    let path = parse_path(&req.path)?;
    let result = match req.expected_generation {
        Some(expected) => s
            .graph
            .write_slot_expected(&path, &req.slot, req.value, expected),
        None => s.graph.write_slot(&path, &req.slot, req.value),
    };
    match result {
        Ok(gen) => Ok(Json(WriteSlotResp { generation: gen }).into_response()),
        Err(graph::GraphError::GenerationMismatch { current, .. }) => Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "generation_mismatch",
                "current_generation": current,
            })),
        )
            .into_response()),
        Err(e) => Err(ApiError::from_graph(e)),
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

#[derive(Serialize)]
struct LinkDto {
    id: String,
    source: EndpointDto,
    target: EndpointDto,
}

#[derive(Serialize)]
struct EndpointDto {
    node_id: String,
    path: Option<String>,
    slot: String,
}

impl LinkDto {
    fn from_link(s: &AppState, link: graph::Link) -> Self {
        let source_path = s
            .graph
            .get_by_id(link.source.node)
            .map(|n| n.path.to_string());
        let target_path = s
            .graph
            .get_by_id(link.target.node)
            .map(|n| n.path.to_string());
        Self {
            id: link.id.0.to_string(),
            source: EndpointDto {
                node_id: link.source.node.to_string(),
                path: source_path,
                slot: link.source.slot,
            },
            target: EndpointDto {
                node_id: link.target.node.to_string(),
                path: target_path,
                slot: link.target.slot,
            },
        }
    }
}

async fn list_links(State(s): State<AppState>) -> Json<Vec<LinkDto>> {
    let mut out: Vec<LinkDto> = s
        .graph
        .links()
        .into_iter()
        .map(|l| LinkDto::from_link(&s, l))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Json(out)
}

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
    Ok(Json(CreatedLinkResp {
        id: id.0.to_string(),
    }))
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
        Self {
            status: StatusCode::BAD_REQUEST,
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

    use engine::BehaviorRegistry;
    use blocks_host::BlockRegistry;
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

    #[tokio::test]
    async fn list_nodes_applies_filter_sort_and_paging() {
        let state = test_state();
        let page = list_nodes_core(
            &state,
            ListNodesQuery {
                filter: Some("kind==sys.core.folder".into()),
                sort: Some("-path".into()),
                page: Some(1),
                size: Some(1),
            },
        )
        .unwrap();

        assert_eq!(page.meta.total, 2);
        assert_eq!(page.meta.page, 1);
        assert_eq!(page.meta.size, 1);
        assert_eq!(page.meta.pages, 2);
        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].path, "/beta");
    }

    #[tokio::test]
    async fn list_nodes_rejects_unknown_query_fields() {
        let err = list_nodes_core(
            &test_state(),
            ListNodesQuery {
                filter: Some("nope==x".into()),
                sort: None,
                page: None,
                size: None,
            },
        )
        .unwrap_err();

        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.error.contains("unknown field"));
    }
}
