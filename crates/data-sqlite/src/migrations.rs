//! Forward-only migrations keyed off SQLite's `user_version` pragma.
//!
//! No external dependency \u{2014} just an append-only list of SQL blocks
//! and a version counter. A fresh DB is brought to the latest schema
//! on `open`; an existing DB fast-forwards. Backwards migrations are
//! intentionally unsupported: rollback happens via the
//! deprecation-and-removal window documented in VERSIONING.md, not via
//! a schema-downgrade path.

use rusqlite::Connection;

use crate::error::SqliteError;

const MIGRATIONS: &[&str] = &[
    // v1 \u{2014} initial graph schema (nodes / slots / links). Matches
    // the "materialized path, SQLite-portable" design called out in
    // EVERYTHING-AS-NODE.md \u{00a7} "Persistence". Tags + node_events
    // land in Stage 5b.
    r#"
    CREATE TABLE nodes (
        id          TEXT PRIMARY KEY,
        parent_id   TEXT,
        kind_id     TEXT NOT NULL,
        path        TEXT NOT NULL UNIQUE,
        name        TEXT NOT NULL,
        lifecycle   TEXT NOT NULL,
        FOREIGN KEY (parent_id) REFERENCES nodes(id) ON DELETE CASCADE
    );
    CREATE INDEX idx_nodes_parent ON nodes(parent_id);
    CREATE INDEX idx_nodes_kind   ON nodes(kind_id);

    CREATE TABLE slots (
        node_id     TEXT NOT NULL,
        name        TEXT NOT NULL,
        role        TEXT NOT NULL,
        value       TEXT NOT NULL,
        generation  INTEGER NOT NULL,
        PRIMARY KEY (node_id, name),
        FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
    );

    CREATE TABLE links (
        id           TEXT PRIMARY KEY,
        source_node  TEXT NOT NULL,
        source_slot  TEXT NOT NULL,
        target_node  TEXT NOT NULL,
        target_slot  TEXT NOT NULL,
        FOREIGN KEY (source_node) REFERENCES nodes(id) ON DELETE CASCADE,
        FOREIGN KEY (target_node) REFERENCES nodes(id) ON DELETE CASCADE
    );
    CREATE INDEX idx_links_source ON links(source_node, source_slot);
    CREATE INDEX idx_links_target ON links(target_node, target_slot);
    "#,
];

pub fn apply(conn: &Connection) -> Result<(), SqliteError> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let current = current as usize;
    if current > MIGRATIONS.len() {
        return Err(SqliteError::Migration(format!(
            "DB at user_version {current} is ahead of this build (has {})",
            MIGRATIONS.len()
        )));
    }
    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let version = idx + 1;
        tracing::info!(version, "applying sqlite migration");
        conn.execute_batch(sql)?;
        conn.pragma_update(None, "user_version", version as i64)?;
    }
    Ok(())
}
