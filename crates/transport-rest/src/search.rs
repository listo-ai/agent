//! `/api/v1/search` — the global lookup endpoint.
//!
//! Thin dispatcher. Takes `scope=<id>` plus the scope's accepted query
//! params, forwards to the matching [`graph::SearchScope`], and renders
//! the hits back as JSON. All real work — RSQL, placement checks, data
//! shaping — lives in the scope implementation (in `graph` or a domain
//! crate). If more than "extract → dispatch → serialize" shows up in
//! this file, the scope isn't carrying its weight; move the logic back.
//!
//! Scopes supported today: `kinds`. Every future scope (`nodes`,
//! `flows`, `audit`, …) is one `match` arm here, no new route.

use std::str::FromStr;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use graph::{KindsQuery, KindsScope, ScopeError, SearchScope};
use serde::{Deserialize, Serialize};
use spi::{Facet, NodePath};

fn parse_facet(raw: &str) -> Option<Facet> {
    // `Facet` derives camelCase serde — round-trip through a JSON string.
    serde_json::from_str(&format!("\"{raw}\"")).ok()
}

use crate::routes::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/search", get(search))
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Scope id — `"kinds"` today, more to come.
    pub scope: String,

    /// RSQL filter; scope-specific field set (see each scope's schema).
    #[serde(default)]
    pub filter: Option<String>,

    /// Comma-separated sort fields; `-field` for descending.
    #[serde(default)]
    pub sort: Option<String>,

    /// Scope-specific shortcut — facet for kinds, kind for nodes, …
    #[serde(default)]
    pub facet: Option<String>,

    /// Scope-specific shortcut — placement-admissible parent for
    /// `scope=kinds`.
    #[serde(default)]
    pub placeable_under: Option<String>,
}

/// Uniform envelope every scope emits. `hits` is the scope-specific
/// row type; serde flattens it into JSON so callers never see the
/// inner type parameter on the wire.
#[derive(Debug, Serialize)]
pub struct SearchResponse<T: Serialize> {
    pub scope: &'static str,
    pub hits: Vec<T>,
    pub meta: SearchMeta,
}

#[derive(Debug, Serialize)]
pub struct SearchMeta {
    pub total: usize,
}

async fn search(
    State(s): State<AppState>,
    Query(p): Query<SearchParams>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;

    match p.scope.as_str() {
        "kinds" => {
            let facet = match p.facet.as_deref() {
                Some(raw) => Some(
                    parse_facet(raw)
                        .ok_or_else(|| ApiError::bad_request(format!("unknown facet `{raw}`")))?,
                ),
                None => None,
            };
            let placeable_under = match p.placeable_under.as_deref() {
                Some(raw) => Some(
                    NodePath::from_str(raw)
                        .map_err(|e| ApiError::bad_request(format!("bad path `{raw}`: {e}")))?,
                ),
                None => None,
            };
            let scope = KindsScope::new(&s.graph);
            let hits = scope
                .query(KindsQuery {
                    facet,
                    placeable_under,
                    filter: p.filter,
                    sort: p.sort,
                })
                .map_err(ApiError::from_scope)?;
            Ok(Json(SearchResponse {
                scope: "kinds",
                hits: hits.data,
                meta: SearchMeta { total: hits.total },
            })
            .into_response())
        }
        other => Err(ApiError::bad_request(format!("unknown scope `{other}`"))),
    }
}

impl ApiError {
    pub(crate) fn from_scope(err: ScopeError) -> Self {
        match err {
            ScopeError::BadRequest(msg) => Self::bad_request(msg),
            ScopeError::NotFound(msg) => Self::not_found(msg),
            ScopeError::Graph(e) => Self::from_graph(e),
        }
    }
}
