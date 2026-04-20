//! SQLite-backed [`HistoryRepo`] implementation.
//!
//! Writes to the `slot_history` table created by data-sqlite migration v3.
//! Cap enforcement (row count + byte window) runs inside the same
//! transaction as the insert so disk usage stays predictable.

use std::sync::Mutex;

use data_repos::{HistoryQuery, HistoryRecord, HistoryRepo, HistorySlotKind, RepoError};
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::connection::{open, Location};
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

    pub fn open_file(path: &std::path::Path) -> Result<Self, SqliteError> {
        Ok(Self {
            conn: Mutex::new(open(Location::File(path))?),
        })
    }

    pub fn open_memory() -> Result<Self, SqliteError> {
        Ok(Self {
            conn: Mutex::new(open(Location::InMemory)?),
        })
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
                        .entry((r.node_id.simple().to_string(), r.slot_name.clone()))
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
                            r.node_id.simple().to_string(),
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
            let nid = q.node_id.simple().to_string();
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![nid, q.slot_name, q.from_ms, q.to_ms], |row| {
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
            })?;
            Ok(rows.collect::<Result<Vec<_>, rusqlite::Error>>()?)
        })
    }

    fn count(&self, node_id: Uuid, slot_name: &str) -> Result<u64, RepoError> {
        self.with_conn(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM slot_history WHERE node_id = ?1 AND slot_name = ?2",
                params![node_id.simple().to_string(), slot_name],
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
                params![node_id.simple().to_string(), slot_name, n as i64],
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
                params![node_id.simple().to_string(), slot_name, day_start_ms, end_ms],
                |row| row.get(0),
            )?;
            Ok(n)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open an in-memory repo with all migrations applied, and seed a dummy
    /// node so FK constraints on `slot_history.node_id` are satisfied.
    fn mk() -> (SqliteHistoryRepo, Uuid) {
        let repo = SqliteHistoryRepo::open_memory().unwrap();
        let id = Uuid::nil();
        repo.with_conn(|conn| {
            conn.execute(
                "INSERT INTO nodes (id, kind_id, path, name, lifecycle)
                 VALUES (?1, 'sys.core.folder', '/test', 'test', 'created')",
                [id.simple().to_string()],
            )?;
            Ok(())
        })
        .unwrap();
        (repo, id)
    }

    fn string_rec(node_id: Uuid, slot: &str, value: &str, ts: i64) -> HistoryRecord {
        HistoryRecord {
            id: 0,
            node_id,
            slot_name: slot.to_string(),
            slot_kind: HistorySlotKind::String,
            ts_ms: ts,
            value_json: Some(format!("\"{}\"", value)),
            blob_bytes: None,
            byte_size: value.len() as i64,
            ntp_synced: true,
            last_sync_age_ms: None,
        }
    }

    fn json_rec(node_id: Uuid, slot: &str, ts: i64) -> HistoryRecord {
        HistoryRecord {
            id: 0,
            node_id,
            slot_name: slot.to_string(),
            slot_kind: HistorySlotKind::Json,
            ts_ms: ts,
            value_json: Some("{\"x\":1}".to_string()),
            blob_bytes: None,
            byte_size: 7,
            ntp_synced: true,
            last_sync_age_ms: None,
        }
    }

    #[test]
    fn insert_and_query_round_trip() {
        let (repo, id) = mk();
        let records = vec![
            string_rec(id, "notes", "first",  1000),
            string_rec(id, "notes", "second", 2000),
            string_rec(id, "notes", "third",  3000),
        ];
        repo.insert_batch(&records, 100).unwrap();

        let q = HistoryQuery {
            node_id: id,
            slot_name: "notes".into(),
            from_ms: 0,
            to_ms: 9999,
            limit: None,
        };
        let result = repo.query_range(&q).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].ts_ms, 1000);
        assert_eq!(result[0].value_json.as_deref(), Some("\"first\""));
        assert_eq!(result[2].ts_ms, 3000);
    }

    #[test]
    fn time_range_filters_correctly() {
        let (repo, id) = mk();
        let records = vec![
            string_rec(id, "notes", "a", 1000),
            string_rec(id, "notes", "b", 2000),
            string_rec(id, "notes", "c", 3000),
        ];
        repo.insert_batch(&records, 100).unwrap();

        let q = HistoryQuery {
            node_id: id,
            slot_name: "notes".into(),
            from_ms: 1500,
            to_ms: 2500,
            limit: None,
        };
        let result = repo.query_range(&q).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ts_ms, 2000);
    }

    #[test]
    fn limit_caps_result_set() {
        let (repo, id) = mk();
        let records: Vec<_> = (0..10)
            .map(|i| string_rec(id, "notes", "v", i * 1000))
            .collect();
        repo.insert_batch(&records, 100).unwrap();

        let q = HistoryQuery {
            node_id: id,
            slot_name: "notes".into(),
            from_ms: 0,
            to_ms: 99999,
            limit: Some(3),
        };
        let result = repo.query_range(&q).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn cap_enforced_at_insert() {
        let (repo, id) = mk();
        // Insert 7 records with a cap of 4 — oldest 3 must be evicted.
        let batch: Vec<_> = (0..7)
            .map(|i| string_rec(id, "notes", "v", i * 1000))
            .collect();
        repo.insert_batch(&batch, 4).unwrap();
        assert_eq!(repo.count(id, "notes").unwrap(), 4);

        // The 4 surviving records should be the newest.
        let q = HistoryQuery {
            node_id: id,
            slot_name: "notes".into(),
            from_ms: 0,
            to_ms: 99999,
            limit: None,
        };
        let result = repo.query_range(&q).unwrap();
        assert_eq!(result[0].ts_ms, 3000); // oldest evicted: 0,1,2 ms
    }

    #[test]
    fn count_returns_row_count() {
        let (repo, id) = mk();
        assert_eq!(repo.count(id, "notes").unwrap(), 0);
        repo.insert_batch(&[string_rec(id, "notes", "v", 1)], 100).unwrap();
        repo.insert_batch(&[string_rec(id, "notes", "v", 2)], 100).unwrap();
        assert_eq!(repo.count(id, "notes").unwrap(), 2);
        // Different slot — isolated counter.
        assert_eq!(repo.count(id, "other").unwrap(), 0);
    }

    #[test]
    fn evict_oldest_removes_correct_rows() {
        let (repo, id) = mk();
        let batch: Vec<_> = (0..5)
            .map(|i| string_rec(id, "notes", "v", i * 1000))
            .collect();
        repo.insert_batch(&batch, 100).unwrap();

        repo.evict_oldest(id, "notes", 2).unwrap();
        assert_eq!(repo.count(id, "notes").unwrap(), 3);

        // Verify the two oldest (ts=0, ts=1000) are gone.
        let q = HistoryQuery {
            node_id: id,
            slot_name: "notes".into(),
            from_ms: 0,
            to_ms: 1500,
            limit: None,
        };
        assert!(repo.query_range(&q).unwrap().is_empty());
    }

    #[test]
    fn bytes_in_window_sums_byte_size() {
        let (repo, id) = mk();
        let day_start: i64 = 1_700_000_000_000; // some fixed ms timestamp
        let in_window = vec![
            // byte_size is the string length in string_rec
            string_rec(id, "fault", "abc",    day_start + 1_000),   // 3 bytes
            string_rec(id, "fault", "hello",  day_start + 3_600_000), // 5 bytes
        ];
        let out_of_window = vec![
            string_rec(id, "fault", "old", day_start - 1), // before window
            string_rec(id, "fault", "new", day_start + 86_400_001), // after window
        ];
        repo.insert_batch(&in_window, 100).unwrap();
        repo.insert_batch(&out_of_window, 100).unwrap();

        let total = repo.bytes_in_window(id, "fault", day_start).unwrap();
        assert_eq!(total, 3 + 5);
    }

    #[test]
    fn json_slot_kind_round_trips() {
        let (repo, id) = mk();
        repo.insert_batch(&[json_rec(id, "config", 1000)], 100).unwrap();
        let q = HistoryQuery {
            node_id: id,
            slot_name: "config".into(),
            from_ms: 0,
            to_ms: 9999,
            limit: None,
        };
        let result = repo.query_range(&q).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slot_kind, HistorySlotKind::Json);
        assert_eq!(result[0].value_json.as_deref(), Some("{\"x\":1}"));
    }

    #[test]
    fn slot_isolation_between_slots() {
        let (repo, id) = mk();
        repo.insert_batch(&[string_rec(id, "a", "va", 1)], 100).unwrap();
        repo.insert_batch(&[string_rec(id, "b", "vb", 2)], 100).unwrap();

        let q_a = HistoryQuery { node_id: id, slot_name: "a".into(), from_ms: 0, to_ms: 9999, limit: None };
        let q_b = HistoryQuery { node_id: id, slot_name: "b".into(), from_ms: 0, to_ms: 9999, limit: None };
        assert_eq!(repo.query_range(&q_a).unwrap().len(), 1);
        assert_eq!(repo.query_range(&q_b).unwrap().len(), 1);
    }
}
