#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! REST transport for the dashboard resolver.
//!
//! Mounts `GET /api/v1/ui/nav` and `POST /api/v1/ui/resolve` under the
//! caller's router (conventionally `transport-rest`). See DASHBOARD.md
//! § M3 for the scope and NEW-API.md for the five-touchpoint rule —
//! every path here has a matching Rust/TS/CLI client surface.

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;

pub mod acl;
pub mod action;
pub mod audit;
pub mod binding_walk;
pub mod compose;
pub mod error;
pub mod handler_registry;
pub mod invalidate;
pub mod limits;
pub mod nav;
pub mod reader;
pub mod render;
pub mod resolve;
pub mod state;
pub mod table;
pub mod vocabulary;

pub use acl::{AclCheck, AclSubject, AllowAll};
pub use action::{ActionContext, ActionResponse, NavigateTo, ToastIntent};
pub use audit::{AuditEvent, AuditSink, TracingAudit};
pub use error::TransportError;
pub use handler_registry::HandlerRegistry;
pub use invalidate::{InvalidateEvent, InvalidateReason, InvalidateSink, TracingInvalidate};
pub use reader::GraphReader;
pub use state::DashboardState;
pub use table::{TableMeta, TableResponse, TableRow};

/// REST API version. Matches `transport-rest::API_PREFIX`.
pub const API_VERSION: u32 = 1;

/// Build the dashboard subrouter. Caller merges into the main router.
///
/// Routes are versioned under `/api/v1/ui/...` — bumping the prefix
/// requires a 12-month deprecation window per `docs/design/VERSIONING.md`.
pub fn router(state: DashboardState) -> Router {
    Router::new()
        .route("/api/v1/ui/nav", get(nav::handler))
        .route("/api/v1/ui/resolve", post(resolve::handler))
        .route("/api/v1/ui/action", post(action::handler))
        .route("/api/v1/ui/table", get(table::handler))
        .route("/api/v1/ui/render", get(render::handler))
        .route("/api/v1/ui/vocabulary", get(vocabulary::handler))
        .route("/api/v1/ui/compose", post(compose::handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
