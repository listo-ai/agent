//! Shared state passed to every request handler.

use std::sync::Arc;

use ai_runner::{AiDefaults, Registry as AiRegistry};
use auth::{DevNullProvider, ProviderCell};
use blocks_host::{BlockHost, BlockRegistry};
use data_repos::{HistoryRepo, PreferencesService};
use data_tsdb::TelemetryRepo;
use domain_auth::SetupService;
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
    /// and by fleet message-level auth. Hot-swappable via
    /// [`ProviderCell`] so the first-boot setup handler can replace
    /// the initial `DevNullProvider` / empty-`StaticTokenProvider`
    /// with a populated one without restarting the process.
    ///
    /// Callers read via `app_state.auth_provider.load()` — the double
    /// Arc is hidden behind the cell.
    pub auth_provider: ProviderCell,
    /// First-boot setup orchestrator. `None` on roles / configs that
    /// never enter setup mode (standalone with DevNull, cloud/edge
    /// already configured via `StaticToken`). Present only when
    /// `AuthConfig::SetupRequired` was resolved at boot.
    pub setup: Option<SetupService>,
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
    /// Path to the live SQLite database file. `None` for in-memory
    /// agents — backup endpoints return 400 in that case.
    pub db_path: Option<std::path::PathBuf>,
    /// Stable identity token for this device (from listod claim).
    /// Falls back to hostname when listod hasn't yet claimed the device.
    pub device_id: String,
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
            auth_provider: ProviderCell::new(Arc::new(DevNullProvider::new())),
            setup: None,
            flows: None,
            prefs: None,
            history_repo: None,
            telemetry_repo: None,
            historizer: None,
            ai_registry: None,
            ai_defaults: AiDefaults::default(),
            db_path: None,
            device_id: hostname(),
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
            auth_provider: ProviderCell::new(Arc::new(DevNullProvider::new())),
            setup: None,
            flows: None,
            prefs: None,
            history_repo: None,
            telemetry_repo: None,
            historizer: None,
            ai_registry: None,
            ai_defaults: AiDefaults::default(),
            db_path: None,
            device_id: hostname(),
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

    /// Seed the initial identity provider at boot. Stores into the
    /// existing [`ProviderCell`], so every `AppState::clone()` sees
    /// the new provider.
    pub fn with_auth_provider(self, provider: Arc<dyn AuthProvider>) -> Self {
        self.auth_provider.store(provider);
        self
    }

    /// Replace the entire [`ProviderCell`] — use when the caller has
    /// already constructed a cell (typically to share one between
    /// `AppState` and `SetupService` so a hot-swap in the service
    /// is observable through `AppState.auth_provider`). `main.rs`
    /// could construct in either order; this lets either order work.
    pub fn with_auth_provider_cell(mut self, cell: ProviderCell) -> Self {
        self.auth_provider = cell;
        self
    }

    /// Attach the first-boot setup service. Present only when
    /// `AuthConfig::SetupRequired` was resolved.
    pub fn with_setup_service(mut self, svc: SetupService) -> Self {
        self.setup = Some(svc);
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

    /// Provide the SQLite database path for backup/restore operations.
    pub fn with_db_path(mut self, path: std::path::PathBuf) -> Self {
        self.db_path = Some(path);
        self
    }

    /// Override the device identity token (from listod claim).
    pub fn with_device_id(mut self, id: String) -> Self {
        self.device_id = id;
        self
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}
