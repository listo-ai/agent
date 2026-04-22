// STAGE-1 complete — SQLite dump with VACUUM INTO + integrity check

//! SQLite dump utilities for snapshot export.
//!
//! Uses `VACUUM INTO` to produce a consistent copy of an online WAL-mode
//! database without quiescing writes. The copy is verified with
//! `PRAGMA integrity_check` before bundling.
//!
//! See BACKUP.md § 4.5 ("Database restore specifics — SQLite").

use std::path::Path;

use rusqlite::Connection;
use tracing::info;

use crate::error::BackupError;

/// Dump a live SQLite database to `dest_path` via `VACUUM INTO`.
///
/// The source database must be in WAL mode (which the agent enforces
/// at open time). The destination file is created; any existing file
/// at the path is overwritten.
///
/// Returns the `PRAGMA user_version` of the dumped database (the
/// schema version used for restore-time compatibility gating).
pub fn dump_sqlite(source_path: &Path, dest_path: &Path) -> Result<u32, BackupError> {
    let conn = Connection::open_with_flags(
        source_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| BackupError::SqliteDump(format!("open {}: {e}", source_path.display())))?;

    let dest_str = dest_path
        .to_str()
        .ok_or_else(|| BackupError::SqliteDump("non-UTF8 dest path".into()))?;

    // VACUUM INTO produces a standalone database file from an online
    // WAL without holding locks beyond the duration of the copy.
    conn.execute_batch(&format!("VACUUM INTO '{dest_str}';"))
        .map_err(|e| BackupError::SqliteDump(format!("VACUUM INTO: {e}")))?;

    info!(
        source = %source_path.display(),
        dest = %dest_path.display(),
        "SQLite VACUUM INTO complete"
    );

    // Verify the copy.
    verify_sqlite(dest_path)?;

    // Read schema version from the copy.
    let copy = Connection::open_with_flags(
        dest_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| BackupError::SqliteDump(format!("open copy: {e}")))?;

    let version: u32 = copy
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| BackupError::SqliteDump(format!("user_version: {e}")))?;

    Ok(version)
}

/// Run `PRAGMA integrity_check` on the file. Returns an error if the
/// check fails or the database can't be opened.
pub fn verify_sqlite(path: &Path) -> Result<(), BackupError> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| BackupError::SqliteDump(format!("open {}: {e}", path.display())))?;

    let result: String = conn
        .pragma_query_value(None, "integrity_check", |row| row.get(0))
        .map_err(|e| BackupError::SqliteDump(format!("integrity_check: {e}")))?;

    if result != "ok" {
        return Err(BackupError::IntegrityCheck {
            path: path.display().to_string(),
            detail: result,
        });
    }

    info!(path = %path.display(), "SQLite integrity check passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn seed_test_db(dir: &Path) -> std::path::PathBuf {
        let db_path = dir.join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA user_version=42;
             CREATE TABLE t (id INTEGER PRIMARY KEY, data TEXT);
             INSERT INTO t VALUES (1, 'hello');
             INSERT INTO t VALUES (2, 'world');",
        )
        .unwrap();
        db_path
    }

    #[test]
    fn dump_and_verify_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let src = seed_test_db(tmp.path());
        let dst = tmp.path().join("copy.db");

        let version = dump_sqlite(&src, &dst).unwrap();
        assert_eq!(version, 42);

        // Verify the copy has the data.
        let conn = Connection::open(&dst).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn verify_detects_corruption() {
        let tmp = TempDir::new().unwrap();
        let bad = tmp.path().join("bad.db");
        // Write garbage that looks like a SQLite header but is corrupt.
        std::fs::write(&bad, b"SQLite format 3\0garbage").unwrap();
        assert!(verify_sqlite(&bad).is_err());
    }
}
