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

impl AgentConfigOverlay {
    /// Layer `self` onto `other`, returning a fresh overlay. Values
    /// present in `self` win. Missing values in both stay missing.
    pub fn merge_over(self, other: AgentConfigOverlay) -> AgentConfigOverlay {
        AgentConfigOverlay {
            role: self.role.or(other.role),
            database: merge_db(self.database, other.database),
            log: merge_log(self.log, other.log),
        }
    }

    /// Fill in defaults and return the concrete config. A role-aware
    /// `default_db_path` lets edge / standalone get a sensible DB
    /// path if the user left it unset.
    pub fn resolve(self, default_db_path_for: impl Fn(Role) -> Option<PathBuf>) -> AgentConfig {
        let role = self.role.unwrap_or_default();
        let db_path = self
            .database
            .as_ref()
            .and_then(|d| d.path.clone())
            .or_else(|| default_db_path_for(role));
        let log_filter = self
            .log
            .as_ref()
            .and_then(|l| l.filter.clone())
            .unwrap_or_else(|| "info".to_string());
        AgentConfig {
            role,
            database: DatabaseConfig { path: db_path },
            log: LogConfig { filter: log_filter },
        }
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
