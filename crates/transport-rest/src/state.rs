//! Shared state passed to every request handler.

use std::sync::Arc;

use auth::DevNullProvider;
use engine::BehaviorRegistry;
use extensions_host::PluginRegistry;
use graph::{GraphEvent, GraphStore};
use spi::{AuthProvider, FleetTransport, NullTransport};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub graph: Arc<GraphStore>,
    pub behaviors: BehaviorRegistry,
    pub events: broadcast::Sender<GraphEvent>,
    pub plugins: PluginRegistry,
    /// Fleet transport — `NullTransport` when the agent is configured
    /// with `fleet: null` (standalone / offline). Holding it in
    /// `AppState` makes the one handler fn usable from both the REST
    /// router and `fleet::mount` without a second source of state.
    pub fleet: Arc<dyn FleetTransport>,
    /// Identity resolver consulted by the `AuthContext` axum extractor
    /// and by fleet message-level auth. Defaults to `DevNullProvider`
    /// which the agent refuses to launch with in `role=cloud` +
    /// `--release` (see `docs/sessions/AUTH-SEAM.md`).
    pub auth_provider: Arc<dyn AuthProvider>,
}

impl AppState {
    pub fn new(
        graph: Arc<GraphStore>,
        behaviors: BehaviorRegistry,
        events: broadcast::Sender<GraphEvent>,
        plugins: PluginRegistry,
    ) -> Self {
        Self {
            graph,
            behaviors,
            events,
            plugins,
            fleet: Arc::new(NullTransport),
            auth_provider: Arc::new(DevNullProvider::new()),
        }
    }

    /// Swap in a real fleet transport (e.g. `ZenohTransport`).
    pub fn with_fleet(mut self, fleet: Arc<dyn FleetTransport>) -> Self {
        self.fleet = fleet;
        self
    }

    /// Swap in a real identity provider (e.g. `StaticTokenProvider`,
    /// future `ZitadelProvider`).
    pub fn with_auth_provider(mut self, provider: Arc<dyn AuthProvider>) -> Self {
        self.auth_provider = provider;
        self
    }
}
