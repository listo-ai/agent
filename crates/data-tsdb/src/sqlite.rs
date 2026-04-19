//! SQLite-backed [`TelemetryRepo`] implementation.
//!
//! Uses the same `slot_timeseries` table created by `data-sqlite`
//! migration v3.  A separate connection is fine here because SQLite's
//! WAL mode allows one writer + concurrent readers; in practice the
//! historizer owns the write path, while the REST query path reads.
//!
//! **Cap enforcement per write:** when `insert_batch` runs, for each
//! (node_id, slot_name) in the batch it counts existing rows and
//! deletes the excess oldest rows inside the same transaction, matching
//! the design's "rolling window eviction at write time" contract.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::{ScalarQuery, ScalarRecord, TelemetryRepo, TsdbError};

pub struct SqliteTelemetryRepo {
    conn: Mutex<Connection>,
}

impl SqliteTelemetryRepo {
    pub fn open_file(path: &Path) -> Result<Self, TsdbError> {
        let conn = Connection::open(path).map_err(|e| TsdbError::Backend(e.to_string()))?;
        // The table is created by data-sqlite migrations; we just need
        // WAL mode enabled to avoid blocking graph writes.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| TsdbError::Backend(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_memory() -> Result<Self, TsdbError> {
        let conn = Connection::open_in_memory().map_err(|e| TsdbError::Backend(e.to_string()))?;
        // Bootstrap minimal schema for unit tests — mirrors migration v3.
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS slot_timeseries (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                node_id          TEXT    NOT NULL,
                slot_name        TEXT    NOT NULL,
                ts_ms            INTEGER NOT NULL,
                bool_value       INTEGER,
                num_value        REAL,
                ntp_synced       INTEGER NOT NULL DEFAULT 1,
                last_sync_age_ms INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_st_node_slot_ts
                ON slot_timeseries(node_id, slot_name, ts_ms);
        "#,
        )
        .map_err(|e| TsdbError::Backend(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with_conn<R>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<R, rusqlite::Error>,
    ) -> Result<R, TsdbError> {
        let mut g = self
            .conn
            .lock()
            .map_err(|_| TsdbError::Backend("sqlite mutex poisoned".into()))?;
        f(&mut g).map_err(|e| TsdbError::Backend(e.to_string()))
    }
}

impl TelemetryRepo for SqliteTelemetryRepo {
    fn insert_batch(&self, records: &[ScalarRecord], max_samples: u64) -> Result<(), TsdbError> {
        if records.is_empty() {
            return Ok(());
        }
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            {
                // Group records by (node_id, slot_name) so cap enforcement is
                // O(distinct slots) rather than O(records). Within each group,
                // keep only the newest `max_samples` records, then evict
                // enough existing rows to stay within cap.
                use std::collections::HashMap;
                let mut groups: HashMap<(String, String), Vec<&ScalarRecord>> = HashMap::new();
                for r in records {
                    groups
                        .entry((r.node_id.to_string(), r.slot_name.clone()))
                        .or_default()
                        .push(r);
                }

                let mut ins = tx.prepare(
                    "INSERT INTO slot_timeseries
                        (node_id, slot_name, ts_ms, bool_value, num_value,
                         ntp_synced, last_sync_age_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?;

                for ((nid, slot_name), mut group) in groups {
                    // Sort ascending so we keep newest.
                    group.sort_by_key(|r| (r.ts_ms, r.ts_ms));
                    // Truncate batch from front if batch alone exceeds cap.
                    if group.len() as u64 > max_samples {
                        let drop_front = group.len() - max_samples as usize;
                        group = group.split_at(drop_front).1.to_vec();
                    }

                    // Count existing rows for this slot.
                    let existing: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM slot_timeseries
                          WHERE node_id = ?1 AND slot_name = ?2",
                        params![nid, slot_name],
                        |row| row.get(0),
                    )?;
                    let total = existing as u64 + group.len() as u64;
                    if total > max_samples {
                        let evict = total - max_samples;
                        tx.execute(
                            "DELETE FROM slot_timeseries WHERE id IN (
                               SELECT id FROM slot_timeseries
                                WHERE node_id = ?1 AND slot_name = ?2
                                ORDER BY ts_ms ASC, id ASC
                                LIMIT ?3
                             )",
                            params![nid, slot_name, evict as i64],
                        )?;
                    }

                    for r in &group {
                        ins.execute(params![
                            r.node_id.to_string(),
                            r.slot_name,
                            r.ts_ms,
                            r.bool_value.map(|b| b as i64),
                            r.num_value,
                            r.ntp_synced as i64,
                            r.last_sync_age_ms,
                        ])?;
                    }
                }
            }
            tx.commit()
        })
    }

    fn query_range(&self, q: &ScalarQuery) -> Result<Vec<ScalarRecord>, TsdbError> {
        self.with_conn(|conn| {
            let limit_clause = if let Some(l) = q.limit {
                format!(" LIMIT {l}")
            } else {
                String::new()
            };
            let sql = format!(
                "SELECT node_id, slot_name, ts_ms, bool_value, num_value,
                        ntp_synced, last_sync_age_ms
                   FROM slot_timeseries
                  WHERE node_id = ?1 AND slot_name = ?2
                    AND ts_ms BETWEEN ?3 AND ?4
                  ORDER BY ts_ms ASC, id ASC{limit_clause}"
            );
            let nid = q.node_id.to_string();
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![nid, q.slot_name, q.from_ms, q.to_ms], |row| {
                let node_id_str: String = row.get(0)?;
                let node_id = Uuid::parse_str(&node_id_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                let bool_value: Option<i64> = row.get(3)?;
                Ok(ScalarRecord {
                    node_id,
                    slot_name: row.get(1)?,
                    ts_ms: row.get(2)?,
                    bool_value: bool_value.map(|v| v != 0),
                    num_value: row.get(4)?,
                    ntp_synced: {
                        let v: i64 = row.get(5)?;
                        v != 0
                    },
                    last_sync_age_ms: row.get(6)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
    }

    fn count(&self, node_id: Uuid, slot_name: &str) -> Result<u64, TsdbError> {
        self.with_conn(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM slot_timeseries WHERE node_id = ?1 AND slot_name = ?2",
                params![node_id.to_string(), slot_name],
                |row| row.get(0),
            )?;
            Ok(n as u64)
        })
    }

    fn evict_oldest(&self, node_id: Uuid, slot_name: &str, n: u64) -> Result<(), TsdbError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM slot_timeseries WHERE id IN (
                     SELECT id FROM slot_timeseries
                      WHERE node_id = ?1 AND slot_name = ?2
                      ORDER BY ts_ms ASC, id ASC
                      LIMIT ?3
                 )",
                params![node_id.to_string(), slot_name, n as i64],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk() -> SqliteTelemetryRepo {
        SqliteTelemetryRepo::open_memory().unwrap()
    }

    fn rec(num: f64, ts: i64) -> ScalarRecord {
        ScalarRecord {
            node_id: Uuid::nil(),
            slot_name: "temp".into(),
            ts_ms: ts,
            bool_value: None,
            num_value: Some(num),
            ntp_synced: true,
            last_sync_age_ms: None,
        }
    }

    #[test]
    fn insert_and_query() {
        let repo = mk();
        let records = vec![rec(1.0, 1000), rec(2.0, 2000), rec(3.0, 3000)];
        repo.insert_batch(&records, 100).unwrap();
        let q = ScalarQuery {
            node_id: Uuid::nil(),
            slot_name: "temp".into(),
            from_ms: 1000,
            to_ms: 2500,
            limit: None,
        };
        let result = repo.query_range(&q).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].num_value, Some(1.0));
        assert_eq!(result[1].num_value, Some(2.0));
    }

    #[test]
    fn cap_enforced_at_insert() {
        let repo = mk();
        // Insert 5 records with cap of 3.
        let batch: Vec<_> = (0..5).map(|i| rec(i as f64, i * 1000)).collect();
        repo.insert_batch(&batch, 3).unwrap();
        assert_eq!(repo.count(Uuid::nil(), "temp").unwrap(), 3);
    }

    #[test]
    fn evict_oldest() {
        let repo = mk();
        let batch: Vec<_> = (0..5).map(|i| rec(i as f64, i * 1000)).collect();
        repo.insert_batch(&batch, 100).unwrap();
        repo.evict_oldest(Uuid::nil(), "temp", 2).unwrap();
        assert_eq!(repo.count(Uuid::nil(), "temp").unwrap(), 3);
        // Oldest two rows (ts=0, ts=1000) should be gone.
        let q = ScalarQuery {
            node_id: Uuid::nil(),
            slot_name: "temp".into(),
            from_ms: 0,
            to_ms: 1500,
            limit: None,
        };
        assert!(repo.query_range(&q).unwrap().is_empty());
    }
}
