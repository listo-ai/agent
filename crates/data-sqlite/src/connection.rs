//! Open a SQLite DB, apply migrations, return a pooled connection.
//!
//! `rusqlite::Connection` is `!Sync`, so we wrap it in `Mutex` inside
//! the repo. SQLite's WAL mode supports many concurrent readers and a
//! single writer; for the edge agent (one process, bounded writers)
//! the mutex contention is not a real concern. Postgres-scale
//! concurrency is a Stage 5b problem on a different backend.

use std::path::Path;

use rusqlite::Connection;

use crate::error::SqliteError;
use crate::migrations;

/// Open a SQLite connection and apply every outstanding migration.
///
/// Pass [`Location::InMemory`] for tests and `Location::File` for real
/// deployments. The WAL pragma plus a moderate busy timeout make the
/// file safe against concurrent readers.
pub fn open(location: Location<'_>) -> Result<Connection, SqliteError> {
    let conn = match location {
        Location::File(path) => Connection::open(path)?,
        Location::InMemory => Connection::open_in_memory()?,
    };
    configure(&conn)?;
    migrations::apply(&conn)?;
    Ok(conn)
}

#[derive(Debug, Clone, Copy)]
pub enum Location<'a> {
    File(&'a Path),
    InMemory,
}

fn configure(conn: &Connection) -> Result<(), SqliteError> {
    // WAL: concurrent readers, serialised writer. Right default for the
    // edge agent; upgraded to JetStream-backed replication in Stage 7.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // Enforce FK constraints we declare in migrations.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Reasonable busy timeout so a rare writer collision waits
    // instead of erroring.
    conn.pragma_update(None, "busy_timeout", 5000_i64)?;
    Ok(())
}
