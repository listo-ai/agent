#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! REST + SSE surface for the agent.
//!
//! Stage-3a manual-test surface. Not the final public API (that lands
//! in Stage 9 with versioning and OpenAPI) — the goal here is to give
//! operators, authors, and demos a way to drive the running agent by
//! hand before the Studio UI arrives in Stage 4.
//!
//! Routes (versioned per `docs/design/VERSIONING.md` § "Public API"):
//! * `GET  /healthz`                              → liveness (unversioned)
//! * `GET  /api/v1/capabilities`                  → host capability manifest
//! * `GET  /api/v1/nodes`                         → node snapshots via the
//!                                                  generic query surface
//! * `GET  /api/v1/node?path=/a/b`                → one node snapshot
//! * `POST /api/v1/nodes`   `{parent,kind,name}`  → create child
//! * `POST /api/v1/slots`   `{path,slot,value}`   → write a slot (fires
//!                                                  behaviour + live-wire)
//! * `POST /api/v1/config`  `{path,config}`       → set a node's config
//!                                                  blob + run `on_init`
//! * `GET  /api/v1/events`                        → SSE stream of `GraphEvent`s
//! * `GET  /`                                     → built-in manual-test UI
//!
//! `GET /api/v1/nodes` accepts the first generic query params slice:
//! `filter`, `sort`, `page`, `size`. Response shape is the list
//! envelope `{ data, meta }`; the higher-level Rust/TS clients unwrap
//! `data` for the simple `list()` helpers so existing callers stay
//! stable.
//!
//! All paths in request bodies / query strings are the canonical
//! `/station/floor1/ahu-5` form — no percent-encoding required in JSON
//! bodies. Query-string callers should still URL-encode reserved
//! characters.

use std::sync::Arc;

use axum::Router;
use graph::{EventSink, GraphEvent};
use tokio::sync::{broadcast, mpsc};

pub mod auth;
pub mod auth_routes;
pub mod capabilities;
pub mod fleet;
pub mod kinds;
pub mod plugins;
pub mod routes;
pub mod seed;
pub mod sink;
pub mod state;
pub mod ui;

pub use capabilities::{host_capabilities, CapabilityManifest, REST_API_VERSION};
pub use routes::API_PREFIX;
pub use sink::AgentSink;
pub use state::AppState;

/// Build the paired sink + receivers the agent hands to the graph
/// store and engine. Every graph mutation fires once, fans out to:
///
///   * the engine worker (mpsc, unbounded, as before) — drives
///     live-wire + behaviour dispatch
///   * the REST SSE broadcast (bounded, lossy on slow consumers) —
///     drives the `GET /api/events` stream
pub fn agent_sink() -> (
    Arc<dyn EventSink>,
    mpsc::UnboundedReceiver<GraphEvent>,
    broadcast::Sender<GraphEvent>,
) {
    let (engine_tx, engine_rx) = mpsc::unbounded_channel();
    let (bcast_tx, _) = broadcast::channel(512);
    let sink: Arc<dyn EventSink> = Arc::new(AgentSink::new(engine_tx, bcast_tx.clone()));
    (sink, engine_rx, bcast_tx)
}

/// Build the axum router. Call once, serve with
/// `axum::serve(listener, router)`.
pub fn router(state: AppState) -> Router {
    routes::mount(state)
}
