#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! SQLite-native repo impls + migrations (edge + standalone).
//!
//! Synchronous `rusqlite` under the hood. Matches the `graph::GraphStore`
//! sync surface; wrapping in `block_on` or duplicating as async is a
//! Postgres-era concern, not an edge one.

mod connection;
mod error;
mod graph_repo;
mod migrations;

pub use connection::Location;
pub use error::SqliteError;
pub use graph_repo::SqliteGraphRepo;
