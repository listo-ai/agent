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

use rusqlite::{params, Connection, TransactionBehavior};
use uuid::Uuid;

use crate::{AggKind, BucketedQuery, BucketedRow, ScalarQuery, ScalarRecord, TelemetryRepo, TsdbError};

pub struct SqliteTelemetryRepo {
    conn: Mutex<Connection>,
}

impl SqliteTelemetryRepo {
    pub fn open_file(path: &Path) -> Result<Self, TsdbError> {
        let conn = Connection::open(path).map_err(|e| TsdbError::Backend(e.to_string()))?;
        // The table is created by data-sqlite migrations; we just need
        // WAL mode enabled to avoid blocking graph writes.
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;",
        )
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
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            {
                // Group records by (node_id, slot_name) so cap enforcement is
                // O(distinct slots) rather than O(records). Within each group,
                // keep only the newest `max_samples` records, then evict
                // enough existing rows to stay within cap.
                use std::collections::HashMap;
                let mut groups: HashMap<(String, String), Vec<&ScalarRecord>> = HashMap::new();
                for r in records {
                    groups
                        .entry((r.node_id.simple().to_string(), r.slot_name.clone()))
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
                            r.node_id.simple().to_string(),
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
            let nid = q.node_id.simple().to_string();
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

    fn query_bucketed(&self, q: &BucketedQuery) -> Result<Vec<BucketedRow>, TsdbError> {
        if q.bucket_ms <= 0 {
            return Err(TsdbError::Invalid("bucket_ms must be > 0".into()));
        }
        // Coalesce bool_value into num_value for aggregation so the same
        // query works for Bool and Number slots. `last` is picked with a
        // correlated subquery on the max `ts_ms` within each bucket.
        let value_expr = "COALESCE(num_value, CASE WHEN bool_value IS NULL THEN NULL ELSE CAST(bool_value AS REAL) END)";
        let agg_expr = match q.agg {
            AggKind::Avg => format!("AVG({value_expr})"),
            AggKind::Min => format!("MIN({value_expr})"),
            AggKind::Max => format!("MAX({value_expr})"),
            AggKind::Sum => format!("SUM({value_expr})"),
            AggKind::Count => "CAST(COUNT(*) AS REAL)".to_string(),
            // `last` = value whose ts_ms is the max in the bucket. We
            // emulate this with MAX(ts_ms * 1e9 + row_index) then join
            // back, but the cheaper portable form is a window function.
            // SQLite 3.25+ supports window functions; assume that.
            AggKind::Last => format!(
                "(SELECT {value_expr} FROM slot_timeseries t2
                   WHERE t2.node_id = ?1 AND t2.slot_name = ?2
                     AND (t2.ts_ms / ?5) * ?5 = (t.ts_ms / ?5) * ?5
                   ORDER BY t2.ts_ms DESC, t2.id DESC LIMIT 1)"
            ),
        };

        // Don't push LIMIT into SQL: ASC+LIMIT keeps the oldest, we
        // want the most-recent. Truncate in Rust after fetch.
        let _ = q.limit;

        // `Last` is a per-row correlated subquery; we still group, but
        // every row in a bucket reports the same value so a simple
        // `MIN(...)` wrapper collapses them harmlessly.
        let select_expr = if matches!(q.agg, AggKind::Last) {
            format!("MIN({agg_expr})")
        } else {
            agg_expr
        };

        let sql = format!(
            "SELECT (ts_ms / ?5) * ?5 AS bucket_ts,
                    {select_expr}  AS agg_val,
                    COUNT(*)       AS cnt
               FROM slot_timeseries t
              WHERE node_id = ?1 AND slot_name = ?2
                AND ts_ms BETWEEN ?3 AND ?4
              GROUP BY bucket_ts
              ORDER BY bucket_ts ASC"
        );

        self.with_conn(|conn| {
            let nid = q.node_id.simple().to_string();
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                params![nid, q.slot_name, q.from_ms, q.to_ms, q.bucket_ms],
                |row| {
                    Ok(BucketedRow {
                        ts_ms: row.get(0)?,
                        value: row.get::<_, Option<f64>>(1)?,
                        count: row.get::<_, i64>(2)? as u64,
                    })
                },
            )?;
            let mut out: Vec<BucketedRow> = rows.collect::<Result<_, _>>()?;
            // If the caller capped buckets, keep the most-recent ones
            // (SQLite LIMIT on ASC would keep the oldest). Swap if needed.
            if let Some(l) = q.limit {
                let l = l as usize;
                if out.len() > l {
                    let drop = out.len() - l;
                    out.drain(0..drop);
                }
            }
            Ok(out)
        })
    }

    fn count(&self, node_id: Uuid, slot_name: &str) -> Result<u64, TsdbError> {
        self.with_conn(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM slot_timeseries WHERE node_id = ?1 AND slot_name = ?2",
                params![node_id.simple().to_string(), slot_name],
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
                params![node_id.simple().to_string(), slot_name, n as i64],
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

    // Bucketing tests live in `tests/bucketing.rs` (integration
    // tests) to keep this file under the 400-line cap.

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
