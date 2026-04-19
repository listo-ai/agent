//! Shared state passed to every request handler.

use std::sync::Arc;

use auth::DevNullProvider;
use data_repos::PreferencesService;
use domain_flows::FlowService;
use engine::BehaviorRegistry;
use extensions_host::PluginRegistry;
use graph::GraphStore;
use spi::{AuthProvider, FleetTransport, NullTransport};
use tokio::sync::broadcast;

use crate::event::SequencedEvent;
use crate::ring::EventRing;

#[derive(Clone)]
pub struct AppState {
    pub graph: Arc<GraphStore>,
    pub behaviors: BehaviorRegistry,
    pub events: broadcast::Sender<SequencedEvent>,
    /// Ring buffer for reconnect replay — see `GET /api/v1/events?since=`.
    pub ring: EventRing,
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
    /// Flow document + revision service.  `None` when the agent runs
    /// without a database path (in-memory, no persistence).
    pub flows: Option<FlowService>,
    /// User / org preference service. `None` when the agent runs without
    /// a database (in-memory only). In-memory agents return 400 on
    /// preference endpoints rather than silently losing writes.
    pub prefs: Option<PreferencesService>,
}

impl AppState {
    pub fn new(
        graph: Arc<GraphStore>,
        behaviors: BehaviorRegistry,
        events: broadcast::Sender<SequencedEvent>,
        plugins: PluginRegistry,
    ) -> Self {
        Self {
            graph,
            behaviors,
            events,
            ring: EventRing::new(crate::ring::DEFAULT_RING_CAPACITY),
            plugins,
            fleet: Arc::new(NullTransport),
            auth_provider: Arc::new(DevNullProvider::new()),
            flows: None,
            prefs: None,
        }
    }

    /// Construct with an explicit ring (used by `agent_sink()` which
    /// creates the ring alongside the sink so they share the same
    /// backing storage).
    pub fn new_with_ring(
        graph: Arc<GraphStore>,
        behaviors: BehaviorRegistry,
        events: broadcast::Sender<SequencedEvent>,
        ring: EventRing,
        plugins: PluginRegistry,
    ) -> Self {
        Self {
            graph,
            behaviors,
            events,
            ring,
            plugins,
            fleet: Arc::new(NullTransport),
            auth_provider: Arc::new(DevNullProvider::new()),
            flows: None,
            prefs: None,
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

    /// Provide the flow document + revision service.
    pub fn with_flow_service(mut self, svc: FlowService) -> Self {
        self.flows = Some(svc);
        self
    }

    /// Provide the user / org preferences service.
    pub fn with_prefs_service(mut self, svc: PreferencesService) -> Self {
        self.prefs = Some(svc);
        self
    }
}
