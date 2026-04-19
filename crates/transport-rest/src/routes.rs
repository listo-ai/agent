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
use futures_util::stream::Stream;
use graph::{Lifecycle, LinkId, NodeSnapshot, SlotRef};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::{KindId, NodeId, NodePath};
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
        .route("/api/v1/node", get(get_node))
        .route("/api/v1/slots", post(write_slot))
        .route("/api/v1/config", post(set_config))
        .route("/api/v1/events", get(stream_events))
        .route("/api/v1/links", get(list_links).post(create_link))
        .route("/api/v1/links/:id", delete(remove_link))
        .route("/api/v1/lifecycle", post(transition_lifecycle))
        .route("/api/v1/seed", post(seed_preset))
        // UI is unversioned — it's a tool, not a contract.
        .route("/", get(ui::index))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn capabilities() -> Json<crate::capabilities::CapabilityManifest> {
    Json(host_capabilities())
}

async fn healthz() -> &'static str {
    "ok"
}

#[derive(Serialize)]
struct NodeDto {
    id: String,
    kind: String,
    path: String,
    parent_id: Option<String>,
    lifecycle: Lifecycle,
    slots: Vec<SlotDto>,
}

#[derive(Serialize)]
struct SlotDto {
    name: String,
    value: JsonValue,
    generation: u64,
}

impl From<NodeSnapshot> for NodeDto {
    fn from(s: NodeSnapshot) -> Self {
        Self {
            id: s.id.to_string(),
            kind: s.kind.as_str().to_string(),
            path: s.path.to_string(),
            parent_id: s.parent.map(|p| p.to_string()),
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

async fn list_nodes(State(s): State<AppState>) -> Json<Vec<NodeDto>> {
    let mut out: Vec<NodeDto> = s.graph.snapshots().into_iter().map(NodeDto::from).collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Json(out)
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
    let id = s
        .graph
        .create_child(&parent, kind, &req.name)
        .map_err(ApiError::from_graph)?;
    Ok(Json(CreatedNodeResp {
        id: id.to_string(),
        path: parent.child(&req.name).to_string(),
    }))
}

#[derive(Deserialize)]
struct WriteSlotReq {
    path: String,
    slot: String,
    value: JsonValue,
}

#[derive(Serialize)]
struct WriteSlotResp {
    generation: u64,
}

async fn write_slot(
    State(s): State<AppState>,
    Json(req): Json<WriteSlotReq>,
) -> Result<Json<WriteSlotResp>, ApiError> {
    let path = parse_path(&req.path)?;
    let gen = s
        .graph
        .write_slot(&path, &req.slot, req.value)
        .map_err(ApiError::from_graph)?;
    Ok(Json(WriteSlotResp { generation: gen }))
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
    s.behaviors.set_config(id, req.config);
    // Re-run on_init so the new config takes effect. Idempotent for
    // well-behaved behaviours (count seeds the slot; trigger resets
    // armed/pending_timer).
    s.behaviors
        .dispatch_init(id)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stream_events(
    State(s): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = s.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => Some(Ok(Event::default().json_data(&ev).unwrap_or_default())),
        Err(_lag) => None, // slow consumer lagged; drop and continue
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn parse_path(s: &str) -> Result<NodePath, ApiError> {
    NodePath::from_str(s).map_err(|e| ApiError::bad_request(format!("bad path `{s}`: {e}")))
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
    status: StatusCode,
    error: String,
}

impl ApiError {
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
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self)).into_response()
    }
}
