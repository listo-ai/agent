//! Deployment role selected at startup.
//!
//! Per `docs/design/OVERVIEW.md` \u{00a7} "Deployment profiles":
//!
//! * [`Role::Standalone`] \u{2014} everything in one process (dev / appliance).
//! * [`Role::Edge`] \u{2014} engine + local extensions + leaf NATS + SQLite.
//! * [`Role::Cloud`] \u{2014} control plane + fleet orchestration + Postgres.
//!
//! The enum is the **runtime** selector. Cargo features (`role-edge`,
//! `role-cloud`, `role-standalone`) gate code that shouldn't compile
//! for a given target \u{2014} e.g. a browser build strips native-only
//! crates, an edge build can omit the Postgres driver. Runtime roles
//! choose among the compiled-in capabilities; feature flags decide
//! which are compiled in.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    Standalone,
    Edge,
    Cloud,
}

impl Role {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Role::Standalone => "standalone",
            Role::Edge => "edge",
            Role::Cloud => "cloud",
        }
    }

    /// Whether this role runs a local flow engine. All three do today;
    /// the seam exists so future browser / studio-only roles can opt
    /// out without touching call sites.
    pub const fn runs_engine(&self) -> bool {
        true
    }

    /// Whether this role serves the Control Plane API. Edge agents
    /// don't; cloud + standalone do.
    pub const fn serves_control_plane(&self) -> bool {
        matches!(self, Role::Cloud | Role::Standalone)
    }

    /// Whether this role expects a durable database by default. A
    /// role that persists still respects an explicit `db = none`
    /// override (useful for ephemeral test deployments).
    pub const fn expects_persistence(&self) -> bool {
        matches!(self, Role::Edge | Role::Cloud | Role::Standalone)
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Role {
    type Err = UnknownRole;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "standalone" => Ok(Role::Standalone),
            "edge" => Ok(Role::Edge),
            "cloud" => Ok(Role::Cloud),
            other => Err(UnknownRole(other.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown role `{0}`; expected one of `standalone`, `edge`, `cloud`")]
pub struct UnknownRole(pub String);
