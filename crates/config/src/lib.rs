#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Config loading with precedence: flags > env > file > defaults.
//!
//! Each source produces an [`AgentConfigOverlay`]. The binary stacks
//! them in order (later wins) and calls
//! [`AgentConfigOverlay::resolve`] with a role-aware default-DB
//! hook to get the final [`AgentConfig`]. See
//! `docs/design/OVERVIEW.md` \u{00a7} "Deployment profiles" for the role
//! semantics.

mod error;
mod loader;
mod model;
mod role;

pub use error::ConfigError;
pub use loader::{from_env, from_file};
pub use model::{
    default_agent_id, AgentConfig, AgentConfigOverlay, AuthConfig, AuthOverlay, DatabaseConfig,
    DatabaseOverlay, Defaults, FleetConfig, FleetOverlay, LogConfig, LogOverlay, PluginsConfig,
    PluginsOverlay, StaticTokenOverlay, ZenohFleetOverlay,
};
pub use role::{Role, UnknownRole};

use std::path::PathBuf;

/// Role-aware default DB path. Returns `None` for cloud until the
/// Postgres-typed connection-string variant lands in Stage 5b.
pub fn default_db_path(role: Role) -> Option<PathBuf> {
    match role {
        Role::Standalone | Role::Edge => Some(PathBuf::from("./agent.db")),
        Role::Cloud => None,
    }
}

/// Role-aware default plugins dir. See `docs/design/PLUGINS.md`
/// § "Where plugins live".
///
/// `standalone` points at a config-dir-relative path rather than `./`
/// so launching the agent from a different shell doesn't silently
/// change what loads. Pass `--plugins-dir .` for in-tree dev.
pub fn default_plugins_dir(role: Role) -> PathBuf {
    match role {
        Role::Standalone => dirs_config_home()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("plugins"),
        Role::Edge => PathBuf::from("/var/lib/agent/plugins"),
        Role::Cloud => PathBuf::from("/opt/agent/plugins"),
    }
}

/// Minimal XDG / platform-native config home resolver. Stays in-crate
/// to avoid adding `dirs` as a dependency for one lookup.
fn dirs_config_home() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return Some(PathBuf::from(x).join("agent"));
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("agent"))
}
