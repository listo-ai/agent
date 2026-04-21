//! `/api/v1/analyze` — the analytical-compute endpoint.
//!
//! Thin wrapper over the analytics-engine sidecar's Zenoh queryable
//! (`listo/<tenant>/analytics/rule/run`) with `dry_run = true`. The
//! request shape is an ad-hoc Rule: inputs + optional DataFusion SQL +
//! optional Rhai; the reply is the raw rule output (no intents).
//!
//! See `docs/design/ANALYTICS.md` § "Global search — the two-endpoint
//! split" for the design contract. `/search` is for "find a thing";
//! `/analyze` is for "compute an answer over time-series + derived
//! state". Conflating them forces admission control onto every palette
//! keystroke — wrong shape.
//!
//! **Stub status.** The analytics-engine sidecar is not yet deployed.
//! This endpoint is wired with its final wire shape so clients can
//! start building against it, but every call currently returns
//! `503 Service Unavailable` with `code: "analytics_unavailable"`.
//! When the sidecar lands (see ANALYTICS.md § Stages 4–5), the handler
//! body swaps from the 503 short-circuit to a Zenoh `get` — no wire
//! change for clients.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/analyze", post(analyze))
}

/// Request body — matches the eventual analytics-engine RPC payload
/// 1:1 so the shim is transparent when the sidecar lands.
#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    /// Named Dataset references or inline Dataset bodies. Each key is
    /// a table name referenced by the SQL stage.
    #[serde(default)]
    pub inputs: serde_json::Map<String, JsonValue>,

    /// DataFusion SQL. Optional — a pure-Rhai rule can skip it.
    #[serde(default)]
    pub sql: Option<String>,

    /// Rhai script. Optional — an SQL-only rule returns the SQL
    /// result directly.
    #[serde(default)]
    pub rhai: Option<String>,

    /// Post-SQL row cap. Defaults apply server-side.
    #[serde(default)]
    pub row_cap: Option<u64>,

    /// Per-call timeout. Defaults apply server-side.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Response envelope on success. The `hits` field is intentionally
/// generic JSON for now — the sidecar returns Arrow RecordBatch on
/// the wire (application/vnd.apache.arrow.stream) with a JSON
/// fallback when the client asks for it; this stub only ever emits
/// JSON.
#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub rows: Vec<JsonValue>,
    pub meta: AnalyzeMeta,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeMeta {
    pub rows_in: u64,
    pub rows_out: u64,
    pub duration_ms: u64,
    /// Always `true` on this endpoint — `/analyze` is `dry_run`-only;
    /// writes happen through the flow engine's `analytics.apply_intents`
    /// block, not here. See ANALYTICS.md § "Non-goals for `/analyze`".
    pub dry_run: bool,
}

/// Typed error surface shared with the future Zenoh wrapper. The
/// discriminants mirror `analytics-engine`'s wire errors 1:1 so the
/// shim is transparent when the sidecar lands.
#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
enum AnalyzeError {
    /// The sidecar isn't deployed (current POC state).
    AnalyticsUnavailable { message: String },
}

async fn analyze(
    State(_s): State<AppState>,
    Json(_req): Json<AnalyzeRequest>,
) -> Response {
    // Short-circuit. When the analytics-engine sidecar lands, this
    // body becomes:
    //
    //     let payload = cbor::to_vec(&req)?;
    //     let subj = Subject::for_agent(tenant, agent_id).kind("analytics.rule.run").build();
    //     let bytes = s.fleet.request(&subj, payload, timeout).await?;
    //     cbor::from_slice::<AnalyzeResponse>(&bytes).into_response()
    //
    // The request/response shapes above stay byte-identical.
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(AnalyzeError::AnalyticsUnavailable {
            message: "analytics-engine sidecar not deployed on this agent".into(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_503_and_typed_error() {
        // Smoke — `AnalyzeError` serialises with the documented
        // `{ code: "analytics_unavailable", message }` shape.
        let err = AnalyzeError::AnalyticsUnavailable {
            message: "not deployed".into(),
        };
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["code"], "analytics_unavailable");
        assert_eq!(v["message"], "not deployed");
    }

    #[test]
    fn request_round_trips_optional_fields() {
        let raw = serde_json::json!({
            "inputs": { "zt": { "dataset": "zone_temps_hourly" } },
            "sql": "SELECT * FROM zt",
            "row_cap": 1000,
        });
        let req: AnalyzeRequest = serde_json::from_value(raw).unwrap();
        assert!(req.rhai.is_none());
        assert_eq!(req.row_cap, Some(1000));
        assert_eq!(req.inputs.len(), 1);
    }

    #[test]
    fn empty_body_parses_with_defaults() {
        let req: AnalyzeRequest = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(req.sql.is_none());
        assert!(req.rhai.is_none());
        assert!(req.inputs.is_empty());
    }
}
