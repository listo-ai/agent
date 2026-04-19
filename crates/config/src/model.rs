//! Resolved agent configuration and its partial overlay type.
//!
//! Layers produce [`AgentConfigOverlay`] (every field `Option<T>`).
//! The resolver walks them in precedence order and composes them into
//! [`AgentConfig`] (every field concrete, with defaults filled in).
//! Separating overlay from resolved keeps "user didn't set this"
//! distinguishable from "user set it to the default" \u{2014} necessary for
//! any later layer to actually override.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::role::Role;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfig {
    pub role: Role,
    pub database: DatabaseConfig,
    pub log: LogConfig,
    pub plugins: PluginsConfig,
    pub fleet: FleetConfig,
}

/// Resolved fleet-transport configuration. See
/// `docs/design/FLEET-TRANSPORT.md` § "Backend selection".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FleetConfig {
    /// Standalone — no cloud at all. `AppState` gets a `NullTransport`.
    Null,
    /// Embedded Zenoh — pure-Rust library, no broker sidecar. Right
    /// default for dev laptops, single-tenant clouds, appliances.
    Zenoh {
        listen: Vec<String>,
        connect: Vec<String>,
        tenant: String,
        agent_id: String,
    },
}

impl FleetConfig {
    /// `true` if this config wants a real transport opened at boot.
    pub fn is_enabled(&self) -> bool {
        !matches!(self, FleetConfig::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseConfig {
    /// `None` keeps the graph in memory. Role defaults fill this in
    /// if the user didn't specify: edge / standalone get
    /// `./agent.db`; cloud leaves it `None` until the Postgres-typed
    /// variant lands in Stage 5b.
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginsConfig {
    /// Directory `PluginRegistry::scan` reads at startup. Role defaults
    /// per `docs/design/PLUGINS.md` § "Where plugins live".
    pub dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogConfig {
    /// `tracing_subscriber`-compatible filter directive. Defaults to
    /// `info`.
    pub filter: String,
}

/// Partial / layered form used by each config source. Missing fields
/// defer to the next layer; present fields win over earlier ones.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfigOverlay {
    pub role: Option<Role>,
    pub database: Option<DatabaseOverlay>,
    pub log: Option<LogOverlay>,
    pub plugins: Option<PluginsOverlay>,
    /// Fleet transport. `fleet: null` in YAML parses as `None` here and
    /// resolves to `FleetConfig::Null`. `fleet: { backend: zenoh, … }`
    /// parses as `Some(FleetOverlay::Zenoh { … })`.
    pub fleet: Option<FleetOverlay>,
}

/// Overlay form for fleet transport. Tagged on `backend` so the YAML
/// reads:
///
/// ```yaml
/// fleet:
///   backend: zenoh
///   listen: ["tcp/0.0.0.0:7447"]
///   connect: []
///   tenant: acme
///   agent_id: edge-1
/// ```
///
/// Today only `zenoh` is wired; `nats` and `mqtt` slot in as additional
/// variants when their crates land.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum FleetOverlay {
    Zenoh(ZenohFleetOverlay),
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ZenohFleetOverlay {
    /// Endpoints this node listens on. Empty for client-only.
    pub listen: Option<Vec<String>>,
    /// Endpoints to dial outbound. Empty for multicast discovery only.
    pub connect: Option<Vec<String>>,
    /// Tenant id for the fleet subject prefix. Defaults to `default`.
    pub tenant: Option<String>,
    /// Agent id for the fleet subject prefix. Defaults to hostname.
    pub agent_id: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseOverlay {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogOverlay {
    pub filter: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PluginsOverlay {
    pub dir: Option<PathBuf>,
}

impl AgentConfigOverlay {
    /// Layer `self` onto `other`, returning a fresh overlay. Values
    /// present in `self` win. Missing values in both stay missing.
    pub fn merge_over(self, other: AgentConfigOverlay) -> AgentConfigOverlay {
        AgentConfigOverlay {
            role: self.role.or(other.role),
            database: merge_db(self.database, other.database),
            log: merge_log(self.log, other.log),
            plugins: merge_plugins(self.plugins, other.plugins),
            fleet: merge_fleet(self.fleet, other.fleet),
        }
    }

    /// Fill in defaults and return the concrete config. Role-aware
    /// hooks provide sensible defaults for fields the user left unset.
    pub fn resolve(self, defaults: Defaults<'_>) -> AgentConfig {
        let role = self.role.unwrap_or_default();
        let db_path = self
            .database
            .as_ref()
            .and_then(|d| d.path.clone())
            .or_else(|| (defaults.db_path)(role));
        let log_filter = self
            .log
            .as_ref()
            .and_then(|l| l.filter.clone())
            .unwrap_or_else(|| "info".to_string());
        let plugins_dir = self
            .plugins
            .as_ref()
            .and_then(|p| p.dir.clone())
            .unwrap_or_else(|| (defaults.plugins_dir)(role));
        let fleet = match self.fleet {
            None => FleetConfig::Null,
            Some(FleetOverlay::Zenoh(z)) => FleetConfig::Zenoh {
                listen: z.listen.unwrap_or_default(),
                connect: z.connect.unwrap_or_default(),
                tenant: z.tenant.unwrap_or_else(|| "default".to_string()),
                agent_id: z.agent_id.unwrap_or_else(default_agent_id),
            },
        };
        AgentConfig {
            role,
            database: DatabaseConfig { path: db_path },
            log: LogConfig { filter: log_filter },
            plugins: PluginsConfig { dir: plugins_dir },
            fleet,
        }
    }
}

/// Hostname, falling back to `local`. Shared with the binary so every
/// place that defaults an agent id uses the same rule.
pub fn default_agent_id() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

/// Role-aware default hooks used by [`AgentConfigOverlay::resolve`].
/// Separated so the binary can compose them without caring about
/// field-by-field plumbing.
pub struct Defaults<'a> {
    pub db_path: &'a dyn Fn(Role) -> Option<PathBuf>,
    pub plugins_dir: &'a dyn Fn(Role) -> PathBuf,
}

fn merge_plugins(
    top: Option<PluginsOverlay>,
    bot: Option<PluginsOverlay>,
) -> Option<PluginsOverlay> {
    match (top, bot) {
        (None, b) => b,
        (Some(t), None) => Some(t),
        (Some(t), Some(b)) => Some(PluginsOverlay {
            dir: t.dir.or(b.dir),
        }),
    }
}

fn merge_db(top: Option<DatabaseOverlay>, bot: Option<DatabaseOverlay>) -> Option<DatabaseOverlay> {
    match (top, bot) {
        (None, b) => b,
        (Some(t), None) => Some(t),
        (Some(t), Some(b)) => Some(DatabaseOverlay {
            path: t.path.or(b.path),
        }),
    }
}

fn merge_log(top: Option<LogOverlay>, bot: Option<LogOverlay>) -> Option<LogOverlay> {
    match (top, bot) {
        (None, b) => b,
        (Some(t), None) => Some(t),
        (Some(t), Some(b)) => Some(LogOverlay {
            filter: t.filter.or(b.filter),
        }),
    }
}

fn merge_fleet(top: Option<FleetOverlay>, bot: Option<FleetOverlay>) -> Option<FleetOverlay> {
    match (top, bot) {
        (None, b) => b,
        (Some(t), None) => Some(t),
        (Some(FleetOverlay::Zenoh(t)), Some(FleetOverlay::Zenoh(b))) => {
            Some(FleetOverlay::Zenoh(ZenohFleetOverlay {
                listen: t.listen.or(b.listen),
                connect: t.connect.or(b.connect),
                tenant: t.tenant.or(b.tenant),
                agent_id: t.agent_id.or(b.agent_id),
            }))
        }
    }
}
