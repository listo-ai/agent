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
    AgentConfig, AgentConfigOverlay, DatabaseConfig, DatabaseOverlay, LogConfig, LogOverlay,
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
