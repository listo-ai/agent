//! REST handlers for slot history queries and on-demand recording.
//!
//! Routes (all under `/api/v1`):
//! * `GET  /api/v1/history`        — structured history (String / Json / Binary slots)
//! * `GET  /api/v1/telemetry`      — scalar history     (Bool / Number slots)
//! * `POST /api/v1/history/record` — on-demand record of a slot's current value
//!
//! Query params for both GET routes:
//!   `path`    node path, e.g. `/station/sensor`
//!   `slot`    slot name, e.g. `temperature`
//!   `from`    inclusive start, Unix ms (default: 0)
//!   `to`      inclusive end,   Unix ms (default: now)
//!   `limit`   max rows returned (default: 1000)
//!
//! Both repos are optional on `AppState`; when absent the endpoint
//! returns 503 (no history / telemetry store configured on this agent).

use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use data_repos::{HistoryQuery, HistoryRecord, HistorySlotKind};
use data_tsdb::{ScalarQuery, ScalarRecord};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::NodePath;

use crate::routes::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/history", get(get_history))
        .route("/api/v1/history/record", post(record_history))
        .route("/api/v1/telemetry", get(get_telemetry))
}

// ---- query params ---------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct HistoryParams {
    pub path: String,
    pub slot: String,
    /// Inclusive start, Unix ms. Defaults to 0 (beginning of time).
    pub from: Option<i64>,
    /// Inclusive end, Unix ms. Defaults to current time.
    pub to: Option<i64>,
    /// Max rows to return. Defaults to 1000.
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct RecordBody {
    pub path: String,
    pub slot: String,
}

// ---- response DTOs --------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct HistoryRecordDto {
    pub id: i64,
    pub node_id: String,
    pub slot_name: String,
    pub slot_kind: String,
    pub ts_ms: i64,
    /// Decoded value for String/Json records; `null` for Binary.
    pub value: Option<JsonValue>,
    pub byte_size: i64,
    pub ntp_synced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_age_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ScalarRecordDto {
    pub node_id: String,
    pub slot_name: String,
    pub ts_ms: i64,
    /// `true` / `false` for Bool slots, numeric for Number slots.
    pub value: JsonValue,
    pub ntp_synced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_age_ms: Option<i64>,
}

// ---- helpers --------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn history_to_dto(r: HistoryRecord) -> HistoryRecordDto {
    let value = if r.slot_kind == HistorySlotKind::Binary {
        None
    } else {
        r.value_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
    };
    HistoryRecordDto {
        id: r.id,
        node_id: r.node_id.to_string(),
        slot_name: r.slot_name,
        slot_kind: r.slot_kind.as_str().to_string(),
        ts_ms: r.ts_ms,
        value,
        byte_size: r.byte_size,
        ntp_synced: r.ntp_synced,
        last_sync_age_ms: r.last_sync_age_ms,
    }
}

fn scalar_to_dto(r: ScalarRecord) -> ScalarRecordDto {
    let value = if let Some(b) = r.bool_value {
        JsonValue::Bool(b)
    } else if let Some(n) = r.num_value {
        serde_json::Number::from_f64(n)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)
    } else {
        JsonValue::Null
    };
    ScalarRecordDto {
        node_id: r.node_id.to_string(),
        slot_name: r.slot_name,
        ts_ms: r.ts_ms,
        value,
        ntp_synced: r.ntp_synced,
        last_sync_age_ms: r.last_sync_age_ms,
    }
}

// ---- handlers -------------------------------------------------------------

/// `GET /api/v1/history` — range query for String/Json/Binary slot history.
async fn get_history(
    State(state): State<AppState>,
    Query(params): Query<HistoryParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = state.history_repo.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "no history store configured — start the agent with a --db path",
        )
    })?;

    let path = NodePath::from_str(&params.path)
        .map_err(|_| ApiError::bad_request(format!("invalid node path `{}`", params.path)))?;
    let node = state
        .graph
        .get(&path)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "node not found"))?;

    let q = HistoryQuery {
        node_id: node.id.0,
        slot_name: params.slot,
        from_ms: params.from.unwrap_or(0),
        to_ms: params.to.unwrap_or_else(now_ms),
        limit: params.limit.or(Some(1000)),
    };

    let records = repo
        .query_range(&q)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    let data: Vec<HistoryRecordDto> = records.into_iter().map(history_to_dto).collect();
    Ok(Json(serde_json::json!({ "data": data })))
}

/// `GET /api/v1/telemetry` — range query for Bool/Number slot scalar history.
async fn get_telemetry(
    State(state): State<AppState>,
    Query(params): Query<HistoryParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = state.telemetry_repo.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "no telemetry store configured — start the agent with a --db path",
        )
    })?;

    let path = NodePath::from_str(&params.path)
        .map_err(|_| ApiError::bad_request(format!("invalid node path `{}`", params.path)))?;
    let node = state
        .graph
        .get(&path)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "node not found"))?;

    let q = ScalarQuery {
        node_id: node.id.0,
        slot_name: params.slot,
        from_ms: params.from.unwrap_or(0),
        to_ms: params.to.unwrap_or_else(now_ms),
        limit: params.limit.or(Some(1000)),
    };

    let records = repo
        .query_range(&q)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    let data: Vec<ScalarRecordDto> = records.into_iter().map(scalar_to_dto).collect();
    Ok(Json(serde_json::json!({ "data": data })))
}

/// `POST /api/v1/history/record` — on-demand record of a slot's current value.
///
/// Reads the slot's live value from the graph and inserts it directly into
/// the appropriate repo. Routing is based on the JSON type of the current
/// value: Bool → telemetry, Number → telemetry, String → history,
/// Object/Array → history as Json, Null → 422.
async fn record_history(
    State(state): State<AppState>,
    Json(body): Json<RecordBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = NodePath::from_str(&body.path)
        .map_err(|_| ApiError::bad_request(format!("invalid node path `{}`", body.path)))?;
    let node = state
        .graph
        .get(&path)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "node not found"))?;

    // Read the slot's current live value.
    let live_value = node
        .slot_values
        .iter()
        .find(|(name, _)| name == &body.slot)
        .map(|(_, sv)| sv.value.clone())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                &format!("slot `{}` not found on node `{}`", body.slot, body.path),
            )
        })?;

    let ts_ms = now_ms();

    match &live_value {
        JsonValue::Bool(b) => {
            let repo = state.telemetry_repo.as_ref().ok_or_else(|| {
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no telemetry store configured",
                )
            })?;
            let r = ScalarRecord {
                node_id: node.id.0,
                slot_name: body.slot,
                ts_ms,
                bool_value: Some(*b),
                num_value: None,
                ntp_synced: true,
                last_sync_age_ms: None,
            };
            repo.insert_batch(&[r], 100_000)
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            Ok(Json(serde_json::json!({ "recorded": true, "kind": "bool" })))
        }
        JsonValue::Number(n) => {
            let repo = state.telemetry_repo.as_ref().ok_or_else(|| {
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no telemetry store configured",
                )
            })?;
            let r = ScalarRecord {
                node_id: node.id.0,
                slot_name: body.slot,
                ts_ms,
                bool_value: None,
                num_value: Some(n.as_f64().unwrap_or(0.0)),
                ntp_synced: true,
                last_sync_age_ms: None,
            };
            repo.insert_batch(&[r], 100_000)
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            Ok(Json(serde_json::json!({ "recorded": true, "kind": "number" })))
        }
        JsonValue::String(_) => {
            let repo = state.history_repo.as_ref().ok_or_else(|| {
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no history store configured",
                )
            })?;
            // Encode as JSON so that `history_to_dto` can round-trip
            // it via `serde_json::from_str`. Storing the raw string
            // bytes (without outer quotes) would cause the decode to fail.
            let json_text = serde_json::to_string(&live_value)
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            let byte_size = json_text.len() as i64;
            let r = HistoryRecord {
                id: 0,
                node_id: node.id.0,
                slot_name: body.slot,
                slot_kind: HistorySlotKind::String,
                ts_ms,
                value_json: Some(json_text),
                blob_bytes: None,
                byte_size,
                ntp_synced: true,
                last_sync_age_ms: None,
            };
            repo.insert_batch(&[r], 100_000)
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            Ok(Json(serde_json::json!({ "recorded": true, "kind": "string" })))
        }
        JsonValue::Object(_) | JsonValue::Array(_) => {
            let repo = state.history_repo.as_ref().ok_or_else(|| {
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no history store configured",
                )
            })?;
            let text = live_value.to_string();
            let byte_size = text.len() as i64;
            let r = HistoryRecord {
                id: 0,
                node_id: node.id.0,
                slot_name: body.slot,
                slot_kind: HistorySlotKind::Json,
                ts_ms,
                value_json: Some(text),
                blob_bytes: None,
                byte_size,
                ntp_synced: true,
                last_sync_age_ms: None,
            };
            repo.insert_batch(&[r], 100_000)
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            Ok(Json(serde_json::json!({ "recorded": true, "kind": "json" })))
        }
        JsonValue::Null => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "slot value is null — nothing to record",
        )),
    }
}
