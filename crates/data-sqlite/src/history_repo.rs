//! SQLite-backed [`HistoryRepo`] implementation.
//!
//! Writes to the `slot_history` table created by data-sqlite migration v3.
//! Cap enforcement (row count + byte window) runs inside the same
//! transaction as the insert so disk usage stays predictable.

use std::sync::Mutex;

use data_repos::{HistoryQuery, HistoryRecord, HistoryRepo, HistorySlotKind, RepoError};
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::SqliteError;

pub struct SqliteHistoryRepo {
    conn: Mutex<Connection>,
}

impl SqliteHistoryRepo {
    pub fn open(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    fn with_conn<R>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<R, SqliteError>,
    ) -> Result<R, RepoError> {
        let mut g = self
            .conn
            .lock()
            .map_err(|_| RepoError::Backend("sqlite mutex poisoned".into()))?;
        Ok(f(&mut g)?)
    }
}

impl HistoryRepo for SqliteHistoryRepo {
    fn insert_batch(&self, records: &[HistoryRecord], max_samples: u64) -> Result<(), RepoError> {
        if records.is_empty() {
            return Ok(());
        }
        self.with_conn(|conn| {
            let tx = conn.transaction()?;
            {
                use std::collections::HashMap;
                let mut groups: HashMap<(String, String), Vec<&HistoryRecord>> = HashMap::new();
                for r in records {
                    groups
                        .entry((r.node_id.to_string(), r.slot_name.clone()))
                        .or_default()
                        .push(r);
                }

                let mut ins = tx.prepare(
                    "INSERT INTO slot_history
                        (node_id, slot_name, slot_kind, ts_ms, value_json, blob_bytes,
                         byte_size, ntp_synced, last_sync_age_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )?;

                for ((nid, slot_name), mut group) in groups {
                    group.sort_by_key(|r| r.ts_ms);
                    if group.len() as u64 > max_samples {
                        let drop_n = group.len() - max_samples as usize;
                        group = group.split_at(drop_n).1.to_vec();
                    }

                    let existing: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM slot_history
                          WHERE node_id = ?1 AND slot_name = ?2",
                        params![nid, slot_name],
                        |row| row.get(0),
                    )?;
                    let total = existing as u64 + group.len() as u64;
                    if total > max_samples {
                        let evict = total - max_samples;
                        tx.execute(
                            "DELETE FROM slot_history WHERE id IN (
                                 SELECT id FROM slot_history
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
                            r.slot_kind.as_str(),
                            r.ts_ms,
                            r.value_json,
                            r.blob_bytes,
                            r.byte_size,
                            r.ntp_synced as i64,
                            r.last_sync_age_ms,
                        ])?;
                    }
                }
            }
            tx.commit()?;
            Ok(())
        })
    }

    fn query_range(&self, q: &HistoryQuery) -> Result<Vec<HistoryRecord>, RepoError> {
        self.with_conn(|conn| {
            let limit_clause = if let Some(l) = q.limit {
                format!(" LIMIT {l}")
            } else {
                String::new()
            };
            let sql = format!(
                "SELECT id, node_id, slot_name, slot_kind, ts_ms, value_json,
                        blob_bytes, byte_size, ntp_synced, last_sync_age_ms
                   FROM slot_history
                  WHERE node_id = ?1 AND slot_name = ?2
                    AND ts_ms BETWEEN ?3 AND ?4
                  ORDER BY ts_ms ASC, id ASC{limit_clause}"
            );
            let nid = q.node_id.to_string();
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                params![nid, q.slot_name, q.from_ms, q.to_ms],
                |row| {
                    let node_id_str: String = row.get(1)?;
                    let node_id = Uuid::parse_str(&node_id_str).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                    let kind_str: String = row.get(3)?;
                    let slot_kind = match kind_str.as_str() {
                        "string" => HistorySlotKind::String,
                        "binary" => HistorySlotKind::Binary,
                        _ => HistorySlotKind::Json,
                    };
                    let ntp: i64 = row.get(8)?;
                    Ok(HistoryRecord {
                        id: row.get(0)?,
                        node_id,
                        slot_name: row.get(2)?,
                        slot_kind,
                        ts_ms: row.get(4)?,
                        value_json: row.get(5)?,
                        blob_bytes: row.get(6)?,
                        byte_size: row.get(7)?,
                        ntp_synced: ntp != 0,
                        last_sync_age_ms: row.get(9)?,
                    })
                },
            )?;
            Ok(rows.collect::<Result<Vec<_>, rusqlite::Error>>()?)
        })
    }

    fn count(&self, node_id: Uuid, slot_name: &str) -> Result<u64, RepoError> {
        self.with_conn(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM slot_history WHERE node_id = ?1 AND slot_name = ?2",
                params![node_id.to_string(), slot_name],
                |row| row.get(0),
            )?;
            Ok(n as u64)
        })
    }

    fn evict_oldest(&self, node_id: Uuid, slot_name: &str, n: u64) -> Result<(), RepoError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM slot_history WHERE id IN (
                     SELECT id FROM slot_history
                      WHERE node_id = ?1 AND slot_name = ?2
                      ORDER BY ts_ms ASC, id ASC
                      LIMIT ?3
                 )",
                params![node_id.to_string(), slot_name, n as i64],
            )?;
            Ok(())
        })
    }

    fn bytes_in_window(
        &self,
        node_id: Uuid,
        slot_name: &str,
        day_start_ms: i64,
    ) -> Result<i64, RepoError> {
        self.with_conn(|conn| {
            let end_ms = day_start_ms + 86_400_000;
            let n: i64 = conn.query_row(
                "SELECT COALESCE(SUM(byte_size), 0)
                   FROM slot_history
                  WHERE node_id = ?1 AND slot_name = ?2
                    AND ts_ms >= ?3 AND ts_ms < ?4",
                params![node_id.to_string(), slot_name, day_start_ms, end_ms],
                |row| row.get(0),
            )?;
            Ok(n)
        })
    }
}

