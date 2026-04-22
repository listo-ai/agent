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
    pub blocks: PluginsConfig,
    pub fleet: FleetConfig,
    pub auth: AuthConfig,
}

/// Resolved identity-provider configuration.
///
/// See `docs/sessions/AUTH-SEAM.md` § "Providers live behind Cargo
/// features" and `docs/design/SYSTEM-BOOTSTRAP.md` for the setup-mode
/// flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthConfig {
    /// Dev-only: every request is stamped as `(DevNull, default, Admin)`.
    /// The agent refuses to boot with this in `role=cloud` on release
    /// builds — enforced by the boot guard in `apps/agent/src/main.rs`.
    DevNull,
    /// Bearer-token table loaded from config. Token material never
    /// leaves the agent's config file; issuance on first boot is via
    /// the setup flow (see `SYSTEM-BOOTSTRAP.md`), which writes a
    /// single entry back to disk.
    StaticToken { tokens: Vec<auth::StaticTokenEntry> },
    /// First-boot marker. Agent mounts only `POST /api/v1/auth/setup`
    /// and 503s every other route until setup completes. The setup
    /// handler then hot-swaps the provider to `StaticToken` and writes
    /// the token back to disk so a restart does not loop back into
    /// setup. Role-aware default for `role=cloud` and `role=edge` when
    /// no `auth:` block is present.
    SetupRequired,
    /// Zitadel-backed OIDC provider. `tenant_id = Some(_)` pins a
    /// single-tenant (edge) install; `None` = multi-tenant cloud.
    ///
    /// Not consumable in Phase A — the provider crate lands in Phase
    /// B, and the boot path in `apps/agent` bails with a clear error
    /// until then. The fields are held so the YAML parses cleanly
    /// (an operator may pre-stage cloud config before Phase B ships).
    Zitadel {
        #[allow(dead_code)]
        issuer: String,
        #[allow(dead_code)]
        jwks_url: String,
        #[allow(dead_code)]
        audience: String,
        #[allow(dead_code)]
        tenant_id: Option<String>,
    },
}

impl AuthConfig {
    /// `true` while first-boot setup has not completed.
    ///
    /// Consulted by the boot path (seed `/agent/setup` + attach
    /// `SetupService` only if true) and by the REST 503-gate
    /// middleware.
    pub fn is_setup_required(&self) -> bool {
        matches!(self, AuthConfig::SetupRequired)
    }
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
    /// Directory `BlockRegistry::scan` reads at startup. Role defaults
    /// per `docs/design/PLUGINS.md` § "Where blocks live".
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
    pub blocks: Option<PluginsOverlay>,
    /// Fleet transport. `fleet: null` in YAML parses as `None` here and
    /// resolves to `FleetConfig::Null`. `fleet: { backend: zenoh, … }`
    /// parses as `Some(FleetOverlay::Zenoh { … })`.
    pub fleet: Option<FleetOverlay>,
    /// Identity provider. Absent → `AuthConfig::DevNull`. See
    /// `docs/sessions/AUTH-SEAM.md`.
    pub auth: Option<AuthOverlay>,
}

/// Overlay form for identity-provider config. Tagged on `provider` so
/// the YAML reads:
///
/// ```yaml
/// auth:
///   provider: static_token
///   tokens:
///     - token: "ed_sys_edge1_xxx"
///       actor: { kind: machine, id: "00000000-0000-0000-0000-000000000001", label: "edge-1" }
///       tenant: sys
///       scopes: [read_nodes, write_slots, manage_fleet]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthOverlay {
    /// Explicit dev-null. Bypasses the role-aware `SetupRequired`
    /// default; operators must opt in by spelling this out — the
    /// standalone role treats this as its natural state, cloud/edge
    /// treat it as an override (and the boot guard vetoes
    /// cloud+DevNull in release).
    DevNull,
    /// Static token table — see `crates/auth/src/static_token.rs`.
    StaticToken(StaticTokenOverlay),
    /// Explicit setup-required. Rarely written by hand; the
    /// role-aware default produces this for cloud/edge when no
    /// `auth:` block is present. Kept explicit so YAML written by
    /// `config::to_file` round-trips unambiguously.
    SetupRequired,
    /// Zitadel provider — Phase B. Boot path refuses to start in
    /// Phase A.
    Zitadel(ZitadelAuthOverlay),
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StaticTokenOverlay {
    pub tokens: Vec<auth::StaticTokenEntry>,
}

/// Overlay form of `AuthConfig::Zitadel`. Fields mirror the resolved
/// variant 1-to-1.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZitadelAuthOverlay {
    pub issuer: String,
    pub jwks_url: String,
    pub audience: String,
    /// `Some` = single-tenant (edge). `None` = multi-tenant cloud.
    #[serde(default)]
    pub tenant_id: Option<String>,
}

/// Overlay form for fleet transport. Tagged on `backend` so the YAML
/// reads:
///
/// ```yaml
/// fleet:
///   backend: zenoh
///   listen: ["tcp/0.0.0.0:7447"]
///   connect: []
///   tenant: sys
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
            blocks: merge_plugins(self.blocks, other.blocks),
            fleet: merge_fleet(self.fleet, other.fleet),
            // Auth doesn't overlay field-by-field — a higher layer
            // either specifies a provider or defers to the next layer.
            // Partial auth configs would hide identity bugs.
            auth: self.auth.or(other.auth),
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
        let blocks_dir = self
            .blocks
            .as_ref()
            .and_then(|p| p.dir.clone())
            .unwrap_or_else(|| (defaults.blocks_dir)(role));
        let fleet = match self.fleet {
            None => FleetConfig::Null,
            Some(FleetOverlay::Zenoh(z)) => FleetConfig::Zenoh {
                listen: z.listen.unwrap_or_default(),
                connect: z.connect.unwrap_or_default(),
                tenant: z.tenant.unwrap_or_else(|| "default".to_string()),
                agent_id: z.agent_id.unwrap_or_else(default_agent_id),
            },
        };
        // Role-aware auth default: cloud/edge with no `auth:` block
        // resolves to `SetupRequired` so first boot is never open.
        // Standalone keeps the historical `DevNull` default — the
        // "just run it" experience that dev + appliance flows expect.
        // Explicit `provider: dev_null` always wins — that's the CI /
        // operator override path.
        let auth = match self.auth {
            None => match role {
                Role::Standalone => AuthConfig::DevNull,
                Role::Cloud | Role::Edge => AuthConfig::SetupRequired,
            },
            Some(AuthOverlay::DevNull) => AuthConfig::DevNull,
            Some(AuthOverlay::StaticToken(t)) => AuthConfig::StaticToken { tokens: t.tokens },
            Some(AuthOverlay::SetupRequired) => AuthConfig::SetupRequired,
            Some(AuthOverlay::Zitadel(z)) => AuthConfig::Zitadel {
                issuer: z.issuer,
                jwks_url: z.jwks_url,
                audience: z.audience,
                tenant_id: z.tenant_id,
            },
        };
        AgentConfig {
            role,
            database: DatabaseConfig { path: db_path },
            log: LogConfig { filter: log_filter },
            blocks: PluginsConfig { dir: blocks_dir },
            fleet,
            auth,
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
    pub blocks_dir: &'a dyn Fn(Role) -> PathBuf,
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
