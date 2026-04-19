//! REST handlers for flow documents and their revision history.
//!
//! Routes (all under `/api/v1`):
//!
//! ```text
//! GET    /flows                          list flows
//! POST   /flows                          create flow
//! GET    /flows/:id                      get flow (live document)
//! DELETE /flows/:id                      delete flow
//! POST   /flows/:id/edit                 append an edit revision
//! GET    /flows/:id/revisions            list revisions (paged)
//! GET    /flows/:id/revisions/:rev_id    materialised document at revision
//! POST   /flows/:id/undo                 append undo revision
//! POST   /flows/:id/redo                 append redo revision
//! POST   /flows/:id/revert               append revert revision
//! ```
//!
//! All mutating endpoints require `expected_head` for optimistic
//! concurrency and return the new `head_revision_id` on success.

use std::str::FromStr;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use data_entities::{FlowId, RevisionId};
use domain_flows::{FlowError, FlowService};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::state::AppState;

// ── Mounting ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/flows", get(list_flows).post(create_flow))
        .route(
            "/api/v1/flows/:id",
            get(get_flow).delete(delete_flow),
        )
        .route("/api/v1/flows/:id/edit", post(edit_flow))
        .route("/api/v1/flows/:id/undo", post(undo_flow))
        .route("/api/v1/flows/:id/redo", post(redo_flow))
        .route("/api/v1/flows/:id/revert", post(revert_flow))
        .route(
            "/api/v1/flows/:id/revisions",
            get(list_revisions),
        )
        .route(
            "/api/v1/flows/:id/revisions/:rev_id",
            get(get_revision_document),
        )
}

// ── DTOs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct FlowDto {
    pub id: String,
    pub name: String,
    pub document: JsonValue,
    pub head_revision_id: Option<String>,
    pub head_seq: i64,
}

#[derive(Debug, Serialize)]
pub struct RevisionDto {
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

#[derive(Debug, Deserialize)]
pub struct CreateFlowBody {
    pub name: String,
    #[serde(default)]
    pub document: JsonValue,
    #[serde(default = "default_author")]
    pub author: String,
}

#[derive(Debug, Deserialize)]
pub struct EditBody {
    pub expected_head: Option<String>,
    pub document: JsonValue,
    #[serde(default = "default_author")]
    pub author: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct UndoBody {
    pub expected_head: Option<String>,
    #[serde(default = "default_author")]
    pub author: String,
}

#[derive(Debug, Deserialize)]
pub struct RedoBody {
    pub expected_head: Option<String>,
    pub expected_target: Option<String>,
    #[serde(default = "default_author")]
    pub author: String,
}

#[derive(Debug, Deserialize)]
pub struct RevertBody {
    pub expected_head: Option<String>,
    pub target_rev_id: String,
    #[serde(default = "default_author")]
    pub author: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteFlowQuery {
    pub expected_head: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_author() -> String {
    "anonymous".into()
}

fn default_limit() -> u32 {
    50
}

#[derive(Debug, Serialize)]
struct MutationResult {
    head_revision_id: String,
}

// ── Error mapping ─────────────────────────────────────────────────────────────

fn flow_err_to_response(err: FlowError) -> Response {
    match &err {
        FlowError::NotFound(_) | FlowError::RevisionNotFound(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": err.to_string() }))).into_response()
        }
        FlowError::Conflict { .. } | FlowError::StaleRedoCursor { .. } => {
            (StatusCode::CONFLICT, Json(serde_json::json!({ "error": err.to_string() }))).into_response()
        }
        FlowError::NothingToUndo(_) | FlowError::NothingToRedo(_) => {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response()
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_flow_id(s: &str) -> Result<FlowId, Response> {
    FlowId::from_str(s).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("invalid flow id: {s}") })),
        )
            .into_response()
    })
}

fn parse_rev_id(s: &str) -> Result<RevisionId, Response> {
    RevisionId::from_str(s).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("invalid revision id: {s}") })),
        )
            .into_response()
    })
}

fn parse_opt_rev_id(s: Option<&str>) -> Result<Option<RevisionId>, Response> {
    match s {
        None => Ok(None),
        Some(s) => parse_rev_id(s).map(Some),
    }
}

fn flow_to_dto(f: data_entities::FlowDocument) -> FlowDto {
    FlowDto {
        id: f.id.to_string(),
        name: f.name,
        document: f.document,
        head_revision_id: f.head_revision_id.map(|r| r.to_string()),
        head_seq: f.head_seq,
    }
}

fn require_flows(state: &AppState) -> Result<&FlowService, Response> {
    state.flows.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "flow service not available (no database path configured)" })),
        )
            .into_response()
    })
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn list_flows(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    match svc.list_flows(q.limit, q.offset) {
        Ok(flows) => {
            let dtos: Vec<FlowDto> = flows.into_iter().map(flow_to_dto).collect();
            Json(dtos).into_response()
        }
        Err(e) => flow_err_to_response(e),
    }
}

async fn create_flow(
    State(state): State<AppState>,
    Json(body): Json<CreateFlowBody>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    match svc.create_flow(body.name, body.document, body.author) {
        Ok(flow) => (StatusCode::CREATED, Json(flow_to_dto(flow))).into_response(),
        Err(e) => flow_err_to_response(e),
    }
}

async fn get_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let flow_id = match parse_flow_id(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    match svc.get_flow(flow_id) {
        Ok(flow) => Json(flow_to_dto(flow)).into_response(),
        Err(e) => flow_err_to_response(e),
    }
}

async fn delete_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<DeleteFlowQuery>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let flow_id = match parse_flow_id(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let expected_head = match parse_opt_rev_id(q.expected_head.as_deref()) {
        Ok(v) => v,
        Err(r) => return r,
    };
    match svc.delete_flow(flow_id, expected_head) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => flow_err_to_response(e),
    }
}

async fn edit_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EditBody>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let flow_id = match parse_flow_id(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let expected_head = match parse_opt_rev_id(body.expected_head.as_deref()) {
        Ok(v) => v,
        Err(r) => return r,
    };
    match svc.edit(flow_id, expected_head, body.document, body.author, body.summary) {
        Ok(rev_id) => Json(MutationResult { head_revision_id: rev_id.to_string() }).into_response(),
        Err(e) => flow_err_to_response(e),
    }
}

async fn undo_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UndoBody>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let flow_id = match parse_flow_id(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let expected_head = match parse_opt_rev_id(body.expected_head.as_deref()) {
        Ok(v) => v,
        Err(r) => return r,
    };
    match svc.undo(flow_id, expected_head, body.author) {
        Ok(rev_id) => Json(MutationResult { head_revision_id: rev_id.to_string() }).into_response(),
        Err(e) => flow_err_to_response(e),
    }
}

async fn redo_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RedoBody>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let flow_id = match parse_flow_id(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let expected_head = match parse_opt_rev_id(body.expected_head.as_deref()) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let expected_target = match parse_opt_rev_id(body.expected_target.as_deref()) {
        Ok(v) => v,
        Err(r) => return r,
    };
    match svc.redo(flow_id, expected_head, expected_target, body.author) {
        Ok(rev_id) => Json(MutationResult { head_revision_id: rev_id.to_string() }).into_response(),
        Err(e) => flow_err_to_response(e),
    }
}

async fn revert_flow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RevertBody>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let flow_id = match parse_flow_id(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let expected_head = match parse_opt_rev_id(body.expected_head.as_deref()) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let target_rev_id = match parse_rev_id(&body.target_rev_id) {
        Ok(v) => v,
        Err(r) => return r,
    };
    match svc.revert(flow_id, expected_head, target_rev_id, body.author) {
        Ok(rev_id) => Json(MutationResult { head_revision_id: rev_id.to_string() }).into_response(),
        Err(e) => flow_err_to_response(e),
    }
}

async fn list_revisions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<PaginationQuery>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let flow_id = match parse_flow_id(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    match svc.list_revisions(flow_id, q.limit, q.offset) {
        Ok(revs) => {
            let dtos: Vec<RevisionDto> = revs
                .into_iter()
                .map(|r| RevisionDto {
                    id: r.id.to_string(),
                    flow_id: r.flow_id.to_string(),
                    parent_id: r.parent_id.map(|p| p.to_string()),
                    seq: r.seq,
                    author: r.author,
                    op: r.op.to_string(),
                    target_rev_id: r.target_rev_id.map(|t| t.to_string()),
                    summary: r.summary,
                    created_at: r.created_at,
                })
                .collect();
            Json(dtos).into_response()
        }
        Err(e) => flow_err_to_response(e),
    }
}

async fn get_revision_document(
    State(state): State<AppState>,
    Path((id, rev_id)): Path<(String, String)>,
) -> Response {
    let svc = match require_flows(&state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let flow_id = match parse_flow_id(&id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    let rev_id = match parse_rev_id(&rev_id) {
        Ok(id) => id,
        Err(r) => return r,
    };
    match svc.document_at(flow_id, rev_id) {
        Ok(doc) => Json(doc).into_response(),
        Err(e) => flow_err_to_response(e),
    }
}
