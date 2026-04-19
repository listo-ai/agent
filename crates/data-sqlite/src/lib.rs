#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! SQLite-native repo impls + migrations (edge + standalone).
//!
//! Synchronous `rusqlite` under the hood. Matches the `graph::GraphStore`
//! sync surface; wrapping in `block_on` or duplicating as async is a
//! Postgres-era concern, not an edge one.

mod connection;
mod error;
mod flow_revision_repo;
mod graph_repo;
mod history_repo;
mod migrations;
mod preferences_repo;

pub use connection::Location;
pub use error::SqliteError;
pub use flow_revision_repo::SqliteFlowRevisionRepo;
pub use graph_repo::SqliteGraphRepo;
pub use history_repo::SqliteHistoryRepo;
pub use preferences_repo::SqlitePreferencesRepo;
