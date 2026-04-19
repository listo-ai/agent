#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! REST transport for the dashboard resolver.
//!
//! Mounts `GET /api/v1/ui/nav` and `POST /api/v1/ui/resolve` under the
//! caller's router (conventionally `transport-rest`). See DASHBOARD.md
//! § M3 for the scope and NEW-API.md for the five-touchpoint rule —
//! every path here has a matching Rust/TS/CLI client surface.

use axum::routing::{get, post};
use axum::Router;

pub mod acl;
pub mod audit;
pub mod error;
pub mod invalidate;
pub mod limits;
pub mod nav;
pub mod reader;
pub mod resolve;
pub mod state;
pub mod widget_registry;

pub use acl::{AclCheck, AclSubject, AllowAll};
pub use audit::{AuditEvent, AuditSink, TracingAudit};
pub use error::TransportError;
pub use invalidate::{InvalidateEvent, InvalidateReason, InvalidateSink, TracingInvalidate};
pub use reader::GraphReader;
pub use state::DashboardState;
pub use widget_registry::WidgetRegistry;

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
        .with_state(state)
}
