//! Shared state passed to every request handler.

use std::sync::Arc;

use ai_runner::{AiDefaults, Registry as AiRegistry};
use auth::DevNullProvider;
use blocks_host::{BlockHost, BlockRegistry};
use data_repos::{HistoryRepo, PreferencesService};
use data_tsdb::TelemetryRepo;
use domain_flows::FlowService;
use domain_history::Historizer;
use engine::BehaviorRegistry;
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
    pub blocks: BlockRegistry,
    /// Process-block runtime manager. `None` when the agent's blocks
    /// dir isn't writable (tests, read-only containers) — enable /
    /// disable endpoints fall back to registry-only flips in that case.
    pub plugin_host: Option<BlockHost>,
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
    /// Structured history repo (String/Json/Binary slots → `slot_history` table).
    /// `None` when the agent has no DB path configured.
    pub history_repo: Option<Arc<dyn HistoryRepo>>,
    /// Scalar history repo (Bool/Number slots → `slot_timeseries` table).
    /// `None` when the agent has no DB path configured.
    pub telemetry_repo: Option<Arc<dyn TelemetryRepo>>,
    /// Historizer service — used for `POST /history/record` (on-demand recording).
    /// `None` until Stage 3 wiring lands; REST currently falls back to direct insert.
    pub historizer: Option<Arc<Historizer>>,
    /// Unified AI runner registry. `None` disables `/api/v1/ai/*` with
    /// a deterministic `ai_unavailable` error.
    pub ai_registry: Option<Arc<AiRegistry>>,
    /// Defaults (provider selection, keys, model override) used when a
    /// caller doesn't supply its own.
    pub ai_defaults: AiDefaults,
}

impl AppState {
    pub fn new(
        graph: Arc<GraphStore>,
        behaviors: BehaviorRegistry,
        events: broadcast::Sender<SequencedEvent>,
        blocks: BlockRegistry,
    ) -> Self {
        Self {
            graph,
            behaviors,
            events,
            ring: EventRing::new(crate::ring::DEFAULT_RING_CAPACITY),
            blocks,
            plugin_host: None,
            fleet: Arc::new(NullTransport),
            auth_provider: Arc::new(DevNullProvider::new()),
            flows: None,
            prefs: None,
            history_repo: None,
            telemetry_repo: None,
            historizer: None,
            ai_registry: None,
            ai_defaults: AiDefaults::default(),
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
        blocks: BlockRegistry,
    ) -> Self {
        Self {
            graph,
            behaviors,
            events,
            ring,
            blocks,
            plugin_host: None,
            fleet: Arc::new(NullTransport),
            auth_provider: Arc::new(DevNullProvider::new()),
            flows: None,
            prefs: None,
            history_repo: None,
            telemetry_repo: None,
            historizer: None,
            ai_registry: None,
            ai_defaults: AiDefaults::default(),
        }
    }

    /// Swap in a real fleet transport (e.g. `ZenohTransport`).
    pub fn with_fleet(mut self, fleet: Arc<dyn FleetTransport>) -> Self {
        self.fleet = fleet;
        self
    }

    /// Attach the process-block host. Without this, enable/disable
    /// endpoints only flip the registry's bit; with it, they also
    /// start/stop the child processes.
    pub fn with_plugin_host(mut self, host: BlockHost) -> Self {
        self.plugin_host = Some(host);
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

    /// Attach the structured-history repo (String/Json/Binary slots → `slot_history` table).
    pub fn with_history_repo(mut self, repo: Arc<dyn HistoryRepo>) -> Self {
        self.history_repo = Some(repo);
        self
    }

    /// Attach the scalar telemetry repo (Bool/Number slots → `slot_timeseries` table).
    pub fn with_telemetry_repo(mut self, repo: Arc<dyn TelemetryRepo>) -> Self {
        self.telemetry_repo = Some(repo);
        self
    }

    /// Attach the historizer service (used for on-demand recording).
    pub fn with_historizer(mut self, h: Arc<Historizer>) -> Self {
        self.historizer = Some(h);
        self
    }

    /// Attach the shared AI runner registry + defaults.
    pub fn with_ai(mut self, registry: Arc<AiRegistry>, defaults: AiDefaults) -> Self {
        self.ai_registry = Some(registry);
        self.ai_defaults = defaults;
        self
    }
}
