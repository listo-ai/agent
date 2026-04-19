//! Forward-only migrations keyed off SQLite's `user_version` pragma.
//!
//! No external dependency — just an append-only list of SQL blocks
//! and a version counter. A fresh DB is brought to the latest schema
//! on `open`; an existing DB fast-forwards. Backwards migrations are
//! intentionally unsupported: rollback happens via the
//! deprecation-and-removal window documented in VERSIONING.md, not via
//! a schema-downgrade path.

use rusqlite::Connection;

use crate::error::SqliteError;

const MIGRATIONS: &[&str] = &[
    // v1 — initial graph schema (nodes / slots / links).
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
    // v2 — SLOT-STORAGE Stage 1: add nullable `kind` column to `slots`.
    r#"
    ALTER TABLE slots ADD COLUMN kind TEXT;
    CREATE INDEX idx_slots_kind ON slots(kind);
    "#,
    // v3 — SLOT-STORAGE Stage 4: history persistence tables.
    r#"
    CREATE TABLE slot_timeseries (
        id               INTEGER PRIMARY KEY AUTOINCREMENT,
        node_id          TEXT    NOT NULL,
        slot_name        TEXT    NOT NULL,
        ts_ms            INTEGER NOT NULL,
        bool_value       INTEGER,
        num_value        REAL,
        ntp_synced       INTEGER NOT NULL DEFAULT 1,
        last_sync_age_ms INTEGER,
        FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
    );
    CREATE INDEX idx_st_node_slot_ts ON slot_timeseries(node_id, slot_name, ts_ms);

    CREATE TABLE slot_history (
        id               INTEGER PRIMARY KEY AUTOINCREMENT,
        node_id          TEXT    NOT NULL,
        slot_name        TEXT    NOT NULL,
        slot_kind        TEXT    NOT NULL,
        ts_ms            INTEGER NOT NULL,
        value_json       TEXT,
        blob_bytes       BLOB,
        byte_size        INTEGER NOT NULL DEFAULT 0,
        ntp_synced       INTEGER NOT NULL DEFAULT 1,
        last_sync_age_ms INTEGER,
        FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
    );
    CREATE INDEX idx_sh_node_slot_ts ON slot_history(node_id, slot_name, ts_ms);
    CREATE INDEX idx_sh_node_slot_id ON slot_history(node_id, slot_name, id);
    "#,
    // v4 — UNDO-REDO Phase 1: flow documents + append-only revision history.
    // Implements docs/sessions/UNDO-REDO.md.
    //
    // Design notes:
    //   • `flows` is the denormalised "current" state row — fast reads
    //     never touch flow_revisions.
    //   • `flow_revisions.patch` is always '[]' in Phase 1 (full snapshot
    //     per revision); the column exists for Phase 2 differential patches.
    //   • `target_rev_id` on an 'undo' entry = the forward revision stepped
    //     over (what redo restores).  See UNDO-REDO.md § redo algorithm.
    //   • `node_setting_revisions` is created here so Phase 1 DB files
    //     can already receive Phase 2 writes without another migration.
    r#"
    CREATE TABLE flows (
        id                  TEXT PRIMARY KEY,
        name                TEXT NOT NULL,
        document            TEXT NOT NULL DEFAULT '{}',
        head_revision_id    TEXT,
        head_seq            INTEGER NOT NULL DEFAULT 0,
        created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );

    CREATE TABLE flow_revisions (
        id              TEXT PRIMARY KEY,
        flow_id         TEXT NOT NULL,
        parent_id       TEXT,
        seq             INTEGER NOT NULL,
        author          TEXT NOT NULL,
        op              TEXT NOT NULL,
        target_rev_id   TEXT,
        summary         TEXT NOT NULL DEFAULT '',
        patch           TEXT NOT NULL DEFAULT '[]',
        snapshot        TEXT,
        created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        FOREIGN KEY (flow_id) REFERENCES flows(id) ON DELETE CASCADE
    );
    CREATE INDEX idx_flow_revisions_flow_seq ON flow_revisions(flow_id, seq DESC);
    CREATE INDEX idx_flow_revisions_target   ON flow_revisions(target_rev_id)
        WHERE target_rev_id IS NOT NULL;

    CREATE TABLE node_setting_revisions (
        id              TEXT PRIMARY KEY,
        flow_id         TEXT NOT NULL,
        node_id         TEXT NOT NULL,
        parent_id       TEXT,
        seq             INTEGER NOT NULL,
        author          TEXT NOT NULL,
        op              TEXT NOT NULL,
        target_rev_id   TEXT,
        schema_version  TEXT NOT NULL DEFAULT '1.0',
        patch           TEXT NOT NULL DEFAULT '[]',
        snapshot        TEXT,
        created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
    CREATE INDEX idx_nsr_node_seq ON node_setting_revisions(node_id, seq DESC);
    CREATE INDEX idx_nsr_flow     ON node_setting_revisions(flow_id);
    "#,
    // v5 — User / org preferences. Implements docs/design/USER-PREFERENCES.md.
    //
    // Design notes:
    //   • `org_preferences` is keyed by `org_id` (today always "default").
    //   • `user_preferences` is keyed by `(user_id, org_id)` — a user has
    //     one row per org they belong to.
    //   • All preference columns are nullable; NULL = "inherit from next layer".
    //   • `updated_at` is UTC epoch *milliseconds* (INTEGER), consistent with
    //     the platform-wide convention of INTEGER timestamps in ms.
    //   • `theme` is user-only — intentionally absent from `org_preferences`.
    r#"
    CREATE TABLE org_preferences (
        org_id              TEXT PRIMARY KEY,
        timezone            TEXT,
        locale              TEXT,
        language            TEXT,
        unit_system         TEXT,
        temperature_unit    TEXT,
        pressure_unit       TEXT,
        date_format         TEXT,
        time_format         TEXT,
        week_start          TEXT,
        number_format       TEXT,
        currency            TEXT,
        updated_at          INTEGER
    );

    CREATE TABLE user_preferences (
        user_id             TEXT NOT NULL,
        org_id              TEXT NOT NULL,
        timezone            TEXT,
        locale              TEXT,
        language            TEXT,
        unit_system         TEXT,
        temperature_unit    TEXT,
        pressure_unit       TEXT,
        date_format         TEXT,
        time_format         TEXT,
        week_start          TEXT,
        number_format       TEXT,
        currency            TEXT,
        theme               TEXT,
        updated_at          INTEGER,
        PRIMARY KEY (user_id, org_id)
    );
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
