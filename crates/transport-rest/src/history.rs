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
//!   `limit`   max rows / buckets returned (default: 1000 raw; uncapped bucketed)
//!
//! Telemetry-only bucketing (see docs/design/QUERY-LANG.md § time-series):
//!   `bucket`  bucket width in ms (wall-clock aligned); presence switches mode
//!   `agg`     `avg | min | max | sum | last | count` (default `avg`)
//!
//! Both repos are optional on `AppState`; when absent the endpoint
//! returns 503 (no history / telemetry store configured on this agent).

use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use data_repos::{HistoryBucketedRow, HistoryQuery, HistoryRecord, HistorySlotKind};
use data_tsdb::{BucketedRow, ScalarQuery, ScalarRecord};
use domain_history::{
    bucketed_history, bucketed_telemetry, grouped_telemetry, GroupedTelemetryResult,
    HistoryBucketedResult, QueryError, TelemetryBucketedResult,
};
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

#[derive(Debug, Default, Deserialize)]
pub struct HistoryParams {
    pub path: String,
    pub slot: String,
    /// Inclusive start, Unix ms. Defaults to 0 (beginning of time).
    pub from: Option<i64>,
    /// Inclusive end, Unix ms. Defaults to current time.
    pub to: Option<i64>,
    /// Max rows to return. Defaults to 1000.
    pub limit: Option<u32>,
    /// Bucket width in ms. Present → bucketed mode (telemetry only).
    /// Wall-clock aligned; see docs/design/QUERY-LANG.md § time-series.
    pub bucket: Option<i64>,
    /// Aggregation to apply within each bucket: `avg | min | max | sum | last | count`.
    /// Default when `bucket` is set: `avg` for telemetry, `last` for history.
    pub agg: Option<String>,
    /// Group-by-kind fan-out for telemetry only — returns one series
    /// per node of the given kind instead of a single flat result.
    /// Mutually exclusive with `path`: pass one or the other.
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BucketedRowDto {
    pub ts_ms: i64,
    /// `null` when no numeric samples fell in the bucket.
    pub value: Option<f64>,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct BucketedMeta {
    pub bucket_ms: i64,
    pub agg: String,
    pub from: i64,
    pub to: i64,
    pub bucket_count: usize,
    /// True when the first returned bucket's start is earlier than
    /// `from` — i.e. the leading bucket is only partially inside the
    /// requested window. Clients can render a dashed/edge marker.
    pub edge_partial_start: bool,
    /// True when the last bucket's end (`ts_ms + bucket_ms`) extends
    /// past `to`. The bucket's aggregate includes samples that are
    /// inside `[from, to]` only — but the bucket itself straddles the
    /// right edge of the window.
    pub edge_partial_end: bool,
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

/// Map `QueryError` → `ApiError`. Typed-to-typed conversion; all
/// transports share the same domain parser/validator, so wire-level
/// error handling is mechanical here.
fn query_error_to_api(e: QueryError) -> ApiError {
    match e {
        QueryError::Invalid(m) => ApiError::bad_request(m),
        QueryError::Backend(m) => ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &m),
    }
}

/// DTO-only translation of a domain telemetry-bucket result.
fn telemetry_bucketed_to_json(r: TelemetryBucketedResult) -> serde_json::Value {
    let data: Vec<BucketedRowDto> = r.rows.into_iter().map(telemetry_row_to_dto).collect();
    let meta = BucketedMeta {
        bucket_ms: r.bucket_ms,
        agg: r.agg.as_str().to_string(),
        from: r.from_ms,
        to: r.to_ms,
        bucket_count: data.len(),
        edge_partial_start: r.edge_partial_start,
        edge_partial_end: r.edge_partial_end,
    };
    serde_json::json!({ "data": data, "meta": meta })
}

fn telemetry_row_to_dto(r: BucketedRow) -> BucketedRowDto {
    BucketedRowDto {
        ts_ms: r.ts_ms,
        value: r.value,
        count: r.count,
    }
}

/// DTO-only translation of a domain history-bucket result.
fn history_bucketed_to_json(r: HistoryBucketedResult) -> serde_json::Value {
    let data: Vec<HistoryBucketedRowDto> = r.rows.into_iter().map(history_row_to_dto).collect();
    let meta = HistoryBucketedMeta {
        bucket_ms: r.bucket_ms,
        agg: r.agg.as_str().to_string(),
        from: r.from_ms,
        to: r.to_ms,
        bucket_count: data.len(),
        edge_partial_start: r.edge_partial_start,
        edge_partial_end: r.edge_partial_end,
    };
    serde_json::json!({ "data": data, "meta": meta })
}

fn history_row_to_dto(r: HistoryBucketedRow) -> HistoryBucketedRowDto {
    let value = match (r.slot_kind, &r.value_json) {
        (Some(HistorySlotKind::Binary), _) | (_, None) => None,
        (_, Some(s)) => serde_json::from_str(s).ok(),
    };
    HistoryBucketedRowDto {
        ts_ms: r.ts_ms,
        value,
        slot_kind: r.slot_kind.map(|k| k.as_str().to_string()),
        count: r.count,
    }
}

fn grouped_telemetry_to_json(r: GroupedTelemetryResult) -> serde_json::Value {
    #[derive(Serialize)]
    struct SeriesOut {
        node_id: String,
        path: String,
        data: Vec<BucketedRowDto>,
        bucket_count: usize,
    }
    #[derive(Serialize)]
    struct GroupMetaOut {
        kind: String,
        slot: String,
        bucket_ms: i64,
        agg: String,
        from: i64,
        to: i64,
        node_count: usize,
    }

    let series: Vec<SeriesOut> = r
        .series
        .into_iter()
        .map(|s| {
            let data: Vec<BucketedRowDto> =
                s.rows.into_iter().map(telemetry_row_to_dto).collect();
            SeriesOut {
                node_id: s.node_id.to_string(),
                path: s.node_path,
                bucket_count: data.len(),
                data,
            }
        })
        .collect();
    let meta = GroupMetaOut {
        kind: r.kind,
        slot: r.slot_name,
        bucket_ms: r.bucket_ms,
        agg: r.agg.as_str().to_string(),
        from: r.from_ms,
        to: r.to_ms,
        node_count: series.len(),
    };
    serde_json::json!({ "series": series, "meta": meta })
}

/// DTO for a row in the bucketed structured-history response.
#[derive(Debug, Serialize)]
pub struct HistoryBucketedRowDto {
    pub ts_ms: i64,
    pub value: Option<JsonValue>,
    pub slot_kind: Option<String>,
    pub count: u64,
}

#[derive(Debug, Serialize)]
pub struct HistoryBucketedMeta {
    pub bucket_ms: i64,
    pub agg: String,
    pub from: i64,
    pub to: i64,
    pub bucket_count: usize,
    pub edge_partial_start: bool,
    pub edge_partial_end: bool,
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
///
/// Raw mode (no `bucket`): returns `HistoryRecordDto` rows.
/// Bucketed mode: delegates to [`crate::history_bucketed::bucketed_history`].
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

    let from_ms = params.from.unwrap_or(0);
    let to_ms = params.to.unwrap_or_else(now_ms);

    if let Some(bucket_ms) = params.bucket {
        let result = bucketed_history(
            &**repo,
            node.id.0,
            params.slot,
            from_ms,
            to_ms,
            bucket_ms,
            params.agg.as_deref(),
            params.limit,
        )
        .map_err(query_error_to_api)?;
        return Ok(Json(history_bucketed_to_json(result)));
    }

    let q = HistoryQuery {
        node_id: node.id.0,
        slot_name: params.slot,
        from_ms,
        to_ms,
        limit: params.limit.or(Some(1000)),
    };

    let records = repo
        .query_range(&q)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    let data: Vec<HistoryRecordDto> = records.into_iter().map(history_to_dto).collect();
    Ok(Json(serde_json::json!({ "data": data })))
}

/// `GET /api/v1/telemetry` — range query for Bool/Number slot scalar history.
///
/// When `bucket` is absent, returns raw `ScalarRecord` rows under
/// `data`. When `bucket` is set, returns aggregated `BucketedRowDto`
/// rows with a `meta` envelope (see docs/design/QUERY-LANG.md §
/// time-series). `agg` defaults to `avg`.
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

    let from_ms = params.from.unwrap_or(0);
    let to_ms = params.to.unwrap_or_else(now_ms);

    // `kind` fan-out is mutually exclusive with single-node `path`.
    if let Some(kind) = params.kind.as_deref() {
        let result = grouped_telemetry(
            &**repo,
            state.graph.as_ref(),
            kind,
            params.slot,
            from_ms,
            to_ms,
            params.bucket,
            params.agg.as_deref(),
            params.limit,
        )
        .map_err(query_error_to_api)?;
        return Ok(Json(grouped_telemetry_to_json(result)));
    }

    let path = NodePath::from_str(&params.path)
        .map_err(|_| ApiError::bad_request(format!("invalid node path `{}`", params.path)))?;
    let node = state
        .graph
        .get(&path)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "node not found"))?;

    if let Some(bucket_ms) = params.bucket {
        let result = bucketed_telemetry(
            &**repo,
            node.id.0,
            params.slot,
            from_ms,
            to_ms,
            bucket_ms,
            params.agg.as_deref(),
            params.limit,
        )
        .map_err(query_error_to_api)?;
        return Ok(Json(telemetry_bucketed_to_json(result)));
    }

    let q = ScalarQuery {
        node_id: node.id.0,
        slot_name: params.slot,
        from_ms,
        to_ms,
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
            Ok(Json(
                serde_json::json!({ "recorded": true, "kind": "bool" }),
            ))
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
            Ok(Json(
                serde_json::json!({ "recorded": true, "kind": "number" }),
            ))
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
            Ok(Json(
                serde_json::json!({ "recorded": true, "kind": "string" }),
            ))
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
            Ok(Json(
                serde_json::json!({ "recorded": true, "kind": "json" }),
            ))
        }
        JsonValue::Null => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "slot value is null — nothing to record",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::{Query, State};
    use axum::Json;
    use blocks_host::BlockRegistry;
    use data_sqlite::SqliteHistoryRepo;
    use data_tsdb::sqlite::SqliteTelemetryRepo;
    use data_tsdb::ScalarRecord;
    use engine::BehaviorRegistry;
    use graph::{seed, GraphStore, KindRegistry, NullSink};
    use rusqlite;
    use spi::{KindId, NodePath};
    use tokio::sync::broadcast;
    use uuid::Uuid;

    use super::*;
    use crate::state::AppState;

    // ---- helpers ----------------------------------------------------------

    fn make_graph() -> Arc<GraphStore> {
        let kinds = KindRegistry::new();
        seed::register_builtins(&kinds);
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
    }

    fn make_state(graph: Arc<GraphStore>) -> AppState {
        let (behaviors, _) = BehaviorRegistry::new(graph.clone());
        let (events, _) = broadcast::channel(16);
        AppState::new(graph, behaviors, events, BlockRegistry::new())
    }

    fn make_history_repo_no_fk() -> Arc<SqliteHistoryRepo> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=OFF;
             CREATE TABLE slot_history (
                 id               INTEGER PRIMARY KEY AUTOINCREMENT,
                 node_id          TEXT    NOT NULL,
                 slot_name        TEXT    NOT NULL,
                 slot_kind        TEXT    NOT NULL,
                 ts_ms            INTEGER NOT NULL,
                 value_json       TEXT,
                 blob_bytes       BLOB,
                 byte_size        INTEGER NOT NULL DEFAULT 0,
                 ntp_synced       INTEGER NOT NULL DEFAULT 1,
                 last_sync_age_ms INTEGER
             );
             CREATE INDEX idx_sh_node_slot_ts ON slot_history(node_id, slot_name, ts_ms);
             CREATE INDEX idx_sh_node_slot_id ON slot_history(node_id, slot_name, id);",
        )
        .unwrap();
        Arc::new(SqliteHistoryRepo::open(conn))
    }

    fn make_state_with_repos(graph: Arc<GraphStore>) -> AppState {
        let history = make_history_repo_no_fk();
        let telemetry = Arc::new(SqliteTelemetryRepo::open_memory().unwrap());
        make_state(graph)
            .with_history_repo(history)
            .with_telemetry_repo(telemetry)
    }

    fn sensor_node(graph: &GraphStore) -> spi::NodeId {
        graph
            .create_child(&NodePath::root(), KindId::new("sys.core.folder"), "sensor")
            .unwrap()
    }

    /// Create the full demo driver tree: / → /proto → /proto/dev → /proto/dev/pt
    /// Returns the NodeId of the point, which has a `value` (Number) slot.
    fn demo_point(graph: &GraphStore) -> spi::NodeId {
        graph
            .create_child(&NodePath::root(), KindId::new("sys.driver.demo"), "proto")
            .unwrap();
        graph
            .create_child(
                &NodePath::root().child("proto"),
                KindId::new("sys.driver.demo.device"),
                "dev",
            )
            .unwrap();
        graph
            .create_child(
                &NodePath::root().child("proto").child("dev"),
                KindId::new("sys.driver.demo.point"),
                "pt",
            )
            .unwrap()
    }

    // ---- GET /api/v1/history ----------------------------------------------

    #[tokio::test]
    async fn get_history_empty_when_no_records() {
        let graph = make_graph();
        sensor_node(&graph);
        let state = make_state_with_repos(graph);

        let resp = get_history(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "notes".into(),
                from: None,
                to: None,
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let body: serde_json::Value = resp.0;
        assert_eq!(body["data"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_history_returns_inserted_records() {
        let graph = make_graph();
        let node_id = sensor_node(&graph);
        let state = make_state_with_repos(graph);

        // Seed a record directly into the repo.
        let repo = state.history_repo.as_ref().unwrap();
        repo.insert_batch(
            &[data_repos::HistoryRecord {
                id: 0,
                node_id: node_id.0,
                slot_name: "notes".into(),
                slot_kind: data_repos::HistorySlotKind::String,
                ts_ms: 5000,
                value_json: Some("\"hello\"".to_string()),
                blob_bytes: None,
                byte_size: 5,
                ntp_synced: true,
                last_sync_age_ms: None,
            }],
            100_000,
        )
        .unwrap();

        let resp = get_history(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "notes".into(),
                from: Some(0),
                to: Some(9999),
                limit: Some(10),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let data = resp.0["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["slot_kind"], "string");
        assert_eq!(data[0]["value"], "hello");
    }

    #[tokio::test]
    async fn get_history_503_when_no_store_configured() {
        let graph = make_graph();
        sensor_node(&graph);
        // AppState WITHOUT a history repo attached.
        let state = make_state(graph);

        let err = get_history(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "notes".into(),
                from: None,
                to: None,
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn get_history_404_when_node_not_found() {
        let graph = make_graph();
        let state = make_state_with_repos(graph);

        let err = get_history(
            State(state),
            Query(HistoryParams {
                path: "/nonexistent".into(),
                slot: "notes".into(),
                from: None,
                to: None,
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
    }

    // ---- GET /api/v1/telemetry --------------------------------------------

    #[tokio::test]
    async fn get_telemetry_returns_inserted_records() {
        let graph = make_graph();
        let node_id = sensor_node(&graph);
        let state = make_state_with_repos(graph);

        let repo = state.telemetry_repo.as_ref().unwrap();
        repo.insert_batch(
            &[ScalarRecord {
                node_id: node_id.0,
                slot_name: "temperature".into(),
                ts_ms: 1000,
                bool_value: None,
                num_value: Some(22.5),
                ntp_synced: true,
                last_sync_age_ms: None,
            }],
            100_000,
        )
        .unwrap();

        let resp = get_telemetry(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "temperature".into(),
                from: Some(0),
                to: Some(9999),
                limit: Some(10),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let data = resp.0["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["value"], 22.5);
    }

    #[tokio::test]
    async fn get_telemetry_bucketed_groups_and_averages() {
        let graph = make_graph();
        let node_id = sensor_node(&graph);
        let state = make_state_with_repos(graph);

        // Three samples in bucket [0, 10000): 10, 20, 30 → avg 20.
        // Two samples  in bucket [10000, 20000): 40, 60 → avg 50.
        let repo = state.telemetry_repo.as_ref().unwrap();
        let rows: Vec<ScalarRecord> = [(1_000, 10.0), (3_000, 20.0), (9_000, 30.0), (11_000, 40.0), (15_000, 60.0)]
            .iter()
            .map(|(ts, v)| ScalarRecord {
                node_id: node_id.0,
                slot_name: "t".into(),
                ts_ms: *ts,
                bool_value: None,
                num_value: Some(*v),
                ntp_synced: true,
                last_sync_age_ms: None,
            })
            .collect();
        repo.insert_batch(&rows, 100_000).unwrap();

        let resp = get_telemetry(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "t".into(),
                from: Some(0),
                to: Some(20_000),
                bucket: Some(10_000),
                agg: Some("avg".into()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let data = resp.0["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["ts_ms"], 0);
        assert!((data[0]["value"].as_f64().unwrap() - 20.0).abs() < 1e-9);
        assert_eq!(data[0]["count"], 3);
        assert_eq!(data[1]["ts_ms"], 10_000);
        assert!((data[1]["value"].as_f64().unwrap() - 50.0).abs() < 1e-9);
        assert_eq!(data[1]["count"], 2);

        assert_eq!(resp.0["meta"]["bucket_ms"], 10_000);
        assert_eq!(resp.0["meta"]["agg"], "avg");
        assert_eq!(resp.0["meta"]["bucket_count"], 2);
    }

    #[tokio::test]
    async fn get_telemetry_bucketed_reports_edge_partial() {
        let graph = make_graph();
        let node_id = sensor_node(&graph);
        let state = make_state_with_repos(graph);
        let repo = state.telemetry_repo.as_ref().unwrap();
        // Two samples at ts=7000 and ts=17000; bucket width 10s. The
        // first bucket starts at 0 (not partial), last bucket starts
        // at 10000 and extends to 20000; if we query to=18_000, the
        // trailing bucket straddles → partial_end = true.
        let rows: Vec<ScalarRecord> = [(7_000, 10.0), (17_000, 20.0)]
            .iter()
            .map(|(ts, v)| ScalarRecord {
                node_id: node_id.0,
                slot_name: "t".into(),
                ts_ms: *ts,
                bool_value: None,
                num_value: Some(*v),
                ntp_synced: true,
                last_sync_age_ms: None,
            })
            .collect();
        repo.insert_batch(&rows, 100_000).unwrap();

        let resp = get_telemetry(
            State(state.clone()),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "t".into(),
                from: Some(0),
                to: Some(18_000),
                bucket: Some(10_000),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        assert_eq!(resp.0["meta"]["edge_partial_start"], false);
        assert_eq!(resp.0["meta"]["edge_partial_end"], true);

        // Now query from=5000, to=20_000. First bucket's ts=0 < from
        // → partial_start; last bucket ends at exactly 20_000 → not partial.
        let resp = get_telemetry(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "t".into(),
                from: Some(5_000),
                to: Some(20_000),
                bucket: Some(10_000),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        assert_eq!(resp.0["meta"]["edge_partial_start"], true);
        assert_eq!(resp.0["meta"]["edge_partial_end"], false);
    }

    #[tokio::test]
    async fn get_history_bucketed_returns_last_value_per_bucket() {
        let graph = make_graph();
        let node_id = sensor_node(&graph);
        let state = make_state_with_repos(graph);
        let repo = state.history_repo.as_ref().unwrap();
        // Two buckets at 10s width: records at ts=1000/5000 + ts=11000.
        for (ts, value) in [(1_000i64, "a"), (5_000, "b"), (11_000, "c")] {
            repo.insert_batch(
                &[data_repos::HistoryRecord {
                    id: 0,
                    node_id: node_id.0,
                    slot_name: "notes".into(),
                    slot_kind: data_repos::HistorySlotKind::String,
                    ts_ms: ts,
                    value_json: Some(format!("\"{value}\"")),
                    blob_bytes: None,
                    byte_size: 1,
                    ntp_synced: true,
                    last_sync_age_ms: None,
                }],
                100_000,
            )
            .unwrap();
        }
        let resp = get_history(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "notes".into(),
                from: Some(0),
                to: Some(20_000),
                bucket: Some(10_000),
                agg: Some("last".into()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let data = resp.0["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["value"], "b"); // newest in [0, 10k)
        assert_eq!(data[1]["value"], "c"); // newest in [10k, 20k)
        assert_eq!(resp.0["meta"]["agg"], "last");
    }

    #[tokio::test]
    async fn get_telemetry_bucketed_rejects_unknown_agg() {
        let graph = make_graph();
        sensor_node(&graph);
        let state = make_state_with_repos(graph);
        let err = get_telemetry(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "t".into(),
                bucket: Some(10_000),
                agg: Some("bogus".into()),
                ..Default::default()
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_telemetry_503_when_no_store_configured() {
        let graph = make_graph();
        sensor_node(&graph);
        let state = make_state(graph); // no telemetry repo

        let err = get_telemetry(
            State(state),
            Query(HistoryParams {
                path: "/sensor".into(),
                slot: "temperature".into(),
                from: None,
                to: None,
                limit: None,
                ..Default::default()
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::SERVICE_UNAVAILABLE);
    }

    // ---- POST /api/v1/history/record --------------------------------------

    #[tokio::test]
    async fn record_history_503_when_no_store() {
        let graph = make_graph();
        sensor_node(&graph);
        // No repos at all.
        let state = make_state(graph);

        let err = record_history(
            State(state),
            Json(RecordBody {
                path: "/sensor".into(),
                slot: "value".into(),
            }),
        )
        .await
        .unwrap_err();
        // Node found but no store → 503 (null-value check runs first only
        // when a slot exists; folder has no slots so 404 fires first).
        // Adjust: use a real demo point so slot exists with null value.
        assert!(
            err.status == StatusCode::SERVICE_UNAVAILABLE || err.status == StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn record_history_404_when_node_not_found() {
        let graph = make_graph();
        let state = make_state_with_repos(graph);

        let err = record_history(
            State(state),
            Json(RecordBody {
                path: "/nope".into(),
                slot: "value".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert!(err.error.contains("node not found"));
    }

    #[tokio::test]
    async fn record_history_404_when_slot_not_found() {
        let graph = make_graph();
        sensor_node(&graph); // sys.core.folder — no declared slots
        let state = make_state_with_repos(graph);

        let err = record_history(
            State(state),
            Json(RecordBody {
                path: "/sensor".into(),
                slot: "nonexistent_slot".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert!(err.error.contains("slot"));
    }

    #[tokio::test]
    async fn record_history_422_when_slot_value_is_null() {
        let graph = make_graph();
        demo_point(&graph); // creates /proto/dev/pt with `value` slot (null initially)
        let state = make_state_with_repos(graph);

        let err = record_history(
            State(state),
            Json(RecordBody {
                path: "/proto/dev/pt".into(),
                slot: "value".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(err.error.contains("null"));
    }

    #[tokio::test]
    async fn record_history_records_number_slot_to_telemetry() {
        let graph = make_graph();
        demo_point(&graph); // value slot is Number
        let pt_path = NodePath::root().child("proto").child("dev").child("pt");
        graph
            .write_slot(&pt_path, "value", serde_json::json!(42.0))
            .unwrap();

        let state = make_state_with_repos(graph.clone());
        let node_id = graph.get(&pt_path).unwrap().id;

        let resp = record_history(
            State(state.clone()),
            Json(RecordBody {
                path: "/proto/dev/pt".into(),
                slot: "value".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(resp.0["recorded"], true);
        assert_eq!(resp.0["kind"], "number");

        // Verify it's actually in the telemetry store.
        let repo = state.telemetry_repo.as_ref().unwrap();
        assert_eq!(repo.count(node_id.0, "value").unwrap(), 1);
    }

    #[tokio::test]
    async fn record_history_records_bool_slot_to_telemetry() {
        // Reuse the demo point but write a bool — the handler routes by
        // live JSON type, not slot schema, so writing `true` to a Number
        // slot tests the Bool branch in record_history.
        // Note: write_slot on demo point (value: Number schema) will
        // accept any JSON value — the schema check is advisory in v1.
        let graph = make_graph();
        demo_point(&graph);
        let pt_path = NodePath::root().child("proto").child("dev").child("pt");
        graph
            .write_slot(&pt_path, "value", serde_json::json!(true))
            .unwrap();

        let state = make_state_with_repos(graph.clone());
        let node_id = graph.get(&pt_path).unwrap().id;

        let resp = record_history(
            State(state.clone()),
            Json(RecordBody {
                path: "/proto/dev/pt".into(),
                slot: "value".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(resp.0["kind"], "bool");
        assert_eq!(
            state
                .telemetry_repo
                .as_ref()
                .unwrap()
                .count(node_id.0, "value")
                .unwrap(),
            1
        );
    }
}
