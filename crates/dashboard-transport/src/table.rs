//! `GET /ui/table` — server-paginated table row endpoint.
//!
//! Query string:
//!
//! ```text
//! GET /api/v1/ui/table
//!   ?query=<rsql>      # base RSQL from TableSource.query; required
//!   &filter=<extra>    # additional RSQL clauses; optional
//!   &page=1
//!   &size=50
//!   &sort=<field>,-<field>
//!   &source_id=<uuid>  # table component id; optional, for audit/caching
//! ```
//!
//! Each response row mirrors the REST node shape so column `field` paths
//! like `"path"`, `"kind"`, `"slots.present_value.value"` resolve
//! naturally through the query executor's dot-path accessor.
//!
//! See `docs/design/SDUI.md` § S3 "Table pagination endpoint".

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use query::{execute, parse_only, QueryRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::state::DashboardState;

/// Max rows per page for table queries.
pub const TABLE_MAX_PAGE_SIZE: usize = 200;

// ---- query params ----------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TableParams {
    /// Base RSQL query string (from `TableSource.query`).
    #[serde(default)]
    pub query: String,
    /// Additional client-side RSQL filter clauses, semicolon-separated.
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub page: Option<usize>,
    pub size: Option<usize>,
    /// The originating table component id. Included in tracing for
    /// future query-result caching; not used for lookup.
    pub source_id: Option<String>,
}

// ---- row shape -------------------------------------------------------------

/// A flattened node row as returned by the table endpoint.
///
/// The query executor's dot-path accessor resolves column `field` values
/// like `"slots.present_value.value"` against the serialised form of
/// this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRow {
    pub id: String,
    pub kind: String,
    /// Absolute path, e.g. `"/station/floor1/sensor1"`. Empty string
    /// if the underlying [`NodeReader`] did not populate it.
    pub path: String,
    pub parent_id: Option<String>,
    /// All slot values keyed by slot name. Nested via dot-path.
    pub slots: HashMap<String, JsonValue>,
}

/// Top-level response from `GET /api/v1/ui/table`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableResponse {
    pub data: Vec<TableRow>,
    pub meta: TableMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMeta {
    pub total: usize,
    pub page: usize,
    pub size: usize,
    pub pages: usize,
}

// ---- handler ---------------------------------------------------------------

pub async fn handler(
    State(state): State<DashboardState>,
    Query(params): Query<TableParams>,
) -> Response {
    // Merge base query + extra filter into a single RSQL filter string.
    let merged_filter = match (&*params.query, &params.filter) {
        ("", None) => None,
        ("", Some(f)) => Some(f.clone()),
        (q, None) => Some(q.to_string()),
        (q, Some(f)) => Some(format!("{q};{f}")),
    };

    let req = QueryRequest {
        filter: merged_filter,
        sort: params.sort,
        page: params.page,
        size: params.size,
    };

    let validated = match parse_only(req, TABLE_MAX_PAGE_SIZE) {
        Ok(v) => v,
        Err(e) => {
            let body = serde_json::json!({ "error": e.to_string() });
            return (StatusCode::BAD_REQUEST, Json(body)).into_response();
        }
    };

    // Collect all nodes from the reader, convert to TableRows.
    let rows: Vec<TableRow> = state
        .reader
        .list_all()
        .into_iter()
        .map(|snap| TableRow {
            id: snap.id.0.to_string(),
            kind: snap.kind.as_str().to_string(),
            path: snap.path.unwrap_or_default(),
            parent_id: snap.parent_id,
            slots: snap.slots,
        })
        .collect();

    match execute(rows, &validated) {
        Ok(page) => {
            let resp = TableResponse {
                data: page.data,
                meta: TableMeta {
                    total: page.meta.total,
                    page: page.meta.page,
                    size: page.meta.size,
                    pages: page.meta.pages,
                },
            };
            Json(resp).into_response()
        }
        Err(e) => {
            let body = serde_json::json!({ "error": e.to_string() });
            (StatusCode::BAD_REQUEST, Json(body)).into_response()
        }
    }
}

// ---- tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::{Query, State};
    use axum::response::IntoResponse as _;
    use dashboard_runtime::{InMemoryReader, NodeSnapshot};
    use spi::NodeId;

    use crate::state::DashboardState;

    use super::*;

    fn node_id(n: u8) -> NodeId {
        NodeId(uuid::Uuid::from_bytes([n; 16]))
    }

    fn make_state() -> DashboardState {
        let mut r = InMemoryReader::new();
        // point node
        let mut snap = NodeSnapshot::new(node_id(1), "sys.driver.point");
        snap.slots
            .insert("name".into(), serde_json::json!("Temperature"));
        snap.slots
            .insert("present_value".into(), serde_json::json!({ "value": 21.5 }));
        snap.path = Some("/station/floor1/temp".into());
        r.insert(snap);
        // nav node
        let snap2 = NodeSnapshot::new(node_id(2), "ui.nav");
        r.insert(snap2);
        DashboardState::new(Arc::new(r))
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn returns_all_rows_without_filter() {
        let state = make_state();
        let params = TableParams {
            query: String::new(),
            filter: None,
            sort: None,
            page: None,
            size: None,
            source_id: None,
        };
        let resp = handler(State(state), Query(params)).await;
        let resp = resp.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["meta"]["total"], 2);
    }

    #[tokio::test]
    async fn filters_by_kind() {
        let state = make_state();
        let params = TableParams {
            query: "kind==sys.driver.point".into(),
            filter: None,
            sort: None,
            page: None,
            size: None,
            source_id: None,
        };
        let resp = handler(State(state), Query(params)).await;
        let resp = resp.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["meta"]["total"], 1);
        assert_eq!(json["data"][0]["kind"], "sys.driver.point");
        assert_eq!(json["data"][0]["path"], "/station/floor1/temp");
    }

    #[tokio::test]
    async fn pagination_works() {
        let mut r = InMemoryReader::new();
        for i in 0u8..5 {
            let snap = NodeSnapshot::new(node_id(i + 10), "sys.driver.point");
            r.insert(snap);
        }
        let state = DashboardState::new(Arc::new(r));
        let params = TableParams {
            query: String::new(),
            filter: None,
            sort: None,
            page: Some(1),
            size: Some(2),
            source_id: None,
        };
        let resp = handler(State(state), Query(params)).await;
        let resp = resp.into_response();
        let json = body_json(resp).await;
        assert_eq!(json["meta"]["total"], 5);
        assert_eq!(json["meta"]["pages"], 3);
        assert_eq!(json["data"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn bad_filter_returns_400() {
        let state = make_state();
        let params = TableParams {
            query: String::new(),
            filter: Some("not_a_valid_filter".into()),
            sort: None,
            page: None,
            size: None,
            source_id: None,
        };
        let resp = handler(State(state), Query(params)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
