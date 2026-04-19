//! `FlowRevisionRepo` implementation over SQLite.
//!
//! Writes are serialised through a single connection under a `Mutex`.
//! Every mutating operation that touches both `flows` and `flow_revisions`
//! runs inside an explicit transaction so a mid-write failure leaves the
//! DB untouched.
//!
//! Phase 1: full-snapshot mode — `patch` is always `[]`; every revision
//! carries a complete document snapshot.  Differential patching is a
//! Phase 2 optimisation and requires no schema change.

use std::path::Path;
use std::str::FromStr;
use std::sync::Mutex;

use data_entities::{FlowDocument, FlowId, FlowRevision, RevisionId, RevisionOp};
use data_repos::{FlowRevisionRepo, RepoError};
use rusqlite::{params, Connection};

use crate::connection::{open, Location};
use crate::error::SqliteError;

// Default per-flow revision cap (configurable at higher layers).
const DEFAULT_REVISION_CAP: u32 = 200;

pub struct SqliteFlowRevisionRepo {
    conn: Mutex<Connection>,
}

impl SqliteFlowRevisionRepo {
    pub fn open_file(path: &Path) -> Result<Self, SqliteError> {
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
            .map_err(|_| RepoError::Backend("sqlite flow-revision mutex poisoned".into()))?;
        Ok(f(&mut g)?)
    }
}

// ── FlowRevisionRepo impl ────────────────────────────────────────────────────

impl FlowRevisionRepo for SqliteFlowRevisionRepo {
    fn get_flow(&self, id: FlowId) -> Result<Option<FlowDocument>, RepoError> {
        self.with_conn(|c| get_flow_row(c, id))
    }

    fn save_flow(&self, flow: &FlowDocument) -> Result<(), RepoError> {
        self.with_conn(|c| upsert_flow_row(c, flow))
    }

    fn delete_flow(&self, id: FlowId) -> Result<(), RepoError> {
        self.with_conn(|c| {
            c.execute("DELETE FROM flows WHERE id = ?1", params![id.0.to_string()])?;
            Ok(())
        })
    }

    fn list_flows(&self, limit: u32, offset: u32) -> Result<Vec<FlowDocument>, RepoError> {
        self.with_conn(|c| list_flow_rows(c, limit, offset))
    }

    fn append_revision(
        &self,
        rev: &FlowRevision,
        new_document: &serde_json::Value,
    ) -> Result<(), RepoError> {
        self.with_conn(|c| append_revision_tx(c, rev, new_document))
    }

    fn get_revision(&self, id: RevisionId) -> Result<Option<FlowRevision>, RepoError> {
        self.with_conn(|c| get_revision_row(c, id))
    }

    fn list_revisions(
        &self,
        flow_id: FlowId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<FlowRevision>, RepoError> {
        self.with_conn(|c| list_revision_rows(c, flow_id, limit, offset))
    }

    fn head_revision(&self, flow_id: FlowId) -> Result<Option<FlowRevision>, RepoError> {
        self.with_conn(|c| head_revision_row(c, flow_id))
    }

    fn prune_revisions(&self, flow_id: FlowId, keep: u32) -> Result<(), RepoError> {
        let cap = if keep == 0 { DEFAULT_REVISION_CAP } else { keep };
        self.with_conn(|c| prune_revisions_tx(c, flow_id, cap))
    }
}

// ── Query helpers ────────────────────────────────────────────────────────────

fn get_flow_row(
    conn: &mut Connection,
    id: FlowId,
) -> Result<Option<FlowDocument>, SqliteError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, document, head_revision_id, head_seq \
         FROM flows WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id.0.to_string()], map_flow_row)?;
    rows.next().transpose().map_err(Into::into)
}

fn upsert_flow_row(conn: &mut Connection, flow: &FlowDocument) -> Result<(), SqliteError> {
    let doc_text = serde_json::to_string(&flow.document)
        .map_err(|e| SqliteError::Json(e))?;
    let head_rev = flow.head_revision_id.map(|r| r.0.to_string());
    conn.execute(
        "INSERT INTO flows (id, name, document, head_revision_id, head_seq, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         ON CONFLICT(id) DO UPDATE SET
             name             = excluded.name,
             document         = excluded.document,
             head_revision_id = excluded.head_revision_id,
             head_seq         = excluded.head_seq,
             updated_at       = excluded.updated_at",
        params![flow.id.0.to_string(), flow.name, doc_text, head_rev, flow.head_seq],
    )?;
    Ok(())
}

fn list_flow_rows(
    conn: &mut Connection,
    limit: u32,
    offset: u32,
) -> Result<Vec<FlowDocument>, SqliteError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, document, head_revision_id, head_seq \
         FROM flows ORDER BY head_seq DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], map_flow_row)?;
    rows.collect::<Result<_, rusqlite::Error>>().map_err(Into::into)
}

fn map_flow_row(
    row: &rusqlite::Row<'_>,
) -> Result<FlowDocument, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let doc_text: String = row.get(2)?;
    let head_rev_str: Option<String> = row.get(3)?;

    let id = FlowId(parse_uuid_rusqlite(&id_str)?);
    let document: serde_json::Value = serde_json::from_str(&doc_text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let head_revision_id = head_rev_str
        .map(|s| Ok::<_, rusqlite::Error>(RevisionId(parse_uuid_rusqlite(&s)?)))
        .transpose()?;

    Ok(FlowDocument {
        id,
        name: row.get(1)?,
        document,
        head_revision_id,
        head_seq: row.get(4)?,
    })
}

fn append_revision_tx(
    conn: &mut Connection,
    rev: &FlowRevision,
    new_document: &serde_json::Value,
) -> Result<(), SqliteError> {
    let patch_text = serde_json::to_string(&rev.patch)
        .map_err(|e| SqliteError::Json(e))?;
    let snapshot_text = rev
        .snapshot
        .as_ref()
        .map(|s| serde_json::to_string(s))
        .transpose()
        .map_err(|e| SqliteError::Json(e))?;
    let doc_text = serde_json::to_string(new_document)
        .map_err(|e| SqliteError::Json(e))?;

    let tx = conn.transaction()?;

    tx.execute(
        "INSERT INTO flow_revisions \
         (id, flow_id, parent_id, seq, author, op, target_rev_id, summary, patch, snapshot) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            rev.id.0.to_string(),
            rev.flow_id.0.to_string(),
            rev.parent_id.map(|r| r.0.to_string()),
            rev.seq,
            rev.author,
            rev.op.to_string(),
            rev.target_rev_id.map(|r| r.0.to_string()),
            rev.summary,
            patch_text,
            snapshot_text,
        ],
    )?;

    tx.execute(
        "UPDATE flows \
         SET document = ?1, head_revision_id = ?2, head_seq = ?3, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE id = ?4",
        params![
            doc_text,
            rev.id.0.to_string(),
            rev.seq,
            rev.flow_id.0.to_string(),
        ],
    )?;

    tx.commit().map_err(Into::into)
}

fn get_revision_row(
    conn: &mut Connection,
    id: RevisionId,
) -> Result<Option<FlowRevision>, SqliteError> {
    let mut stmt = conn.prepare(
        "SELECT id, flow_id, parent_id, seq, author, op, target_rev_id, \
                summary, patch, snapshot, created_at \
         FROM flow_revisions WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id.0.to_string()], map_revision_row)?;
    rows.next().transpose().map_err(Into::into)
}

fn list_revision_rows(
    conn: &mut Connection,
    flow_id: FlowId,
    limit: u32,
    offset: u32,
) -> Result<Vec<FlowRevision>, SqliteError> {
    let mut stmt = conn.prepare(
        "SELECT id, flow_id, parent_id, seq, author, op, target_rev_id, \
                summary, patch, snapshot, created_at \
         FROM flow_revisions \
         WHERE flow_id = ?1 \
         ORDER BY seq DESC \
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(
        params![flow_id.0.to_string(), limit, offset],
        map_revision_row,
    )?;
    rows.collect::<Result<_, rusqlite::Error>>().map_err(Into::into)
}

fn head_revision_row(
    conn: &mut Connection,
    flow_id: FlowId,
) -> Result<Option<FlowRevision>, SqliteError> {
    let mut stmt = conn.prepare(
        "SELECT id, flow_id, parent_id, seq, author, op, target_rev_id, \
                summary, patch, snapshot, created_at \
         FROM flow_revisions \
         WHERE flow_id = ?1 \
         ORDER BY seq DESC \
         LIMIT 1",
    )?;
    let mut rows = stmt.query_map(params![flow_id.0.to_string()], map_revision_row)?;
    rows.next().transpose().map_err(Into::into)
}

fn map_revision_row(row: &rusqlite::Row<'_>) -> Result<FlowRevision, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let flow_id_str: String = row.get(1)?;
    let parent_str: Option<String> = row.get(2)?;
    let op_str: String = row.get(5)?;
    let target_str: Option<String> = row.get(6)?;
    let patch_text: String = row.get(8)?;
    let snapshot_text: Option<String> = row.get(9)?;

    let parse_rev_id = |s: String| -> Result<RevisionId, rusqlite::Error> {
        Ok(RevisionId(parse_uuid_rusqlite(&s)?))
    };

    let op = op_str
        .parse::<RevisionOp>()
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(StringError(e)),
        ))?;

    let patch: serde_json::Value = serde_json::from_str(&patch_text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let snapshot = snapshot_text
        .map(|s| {
            serde_json::from_str::<serde_json::Value>(&s).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
        })
        .transpose()?;

    Ok(FlowRevision {
        id: parse_rev_id(id_str)?,
        flow_id: FlowId(parse_uuid_rusqlite(&flow_id_str)?),
        parent_id: parent_str.map(parse_rev_id).transpose()?,
        seq: row.get(3)?,
        author: row.get(4)?,
        op,
        target_rev_id: target_str.map(parse_rev_id).transpose()?,
        summary: row.get(7)?,
        patch,
        snapshot,
        created_at: row.get(10)?,
    })
}

/// Prune old revisions keeping only the most recent `keep` rows.
///
/// Invariants upheld:
/// 1. The oldest *surviving* revision always has a full snapshot.
/// 2. Revisions that appear as `target_rev_id` in the contiguous
///    undo/redo suffix of the current head are pinned (not deleted).
fn prune_revisions_tx(
    conn: &mut Connection,
    flow_id: FlowId,
    keep: u32,
) -> Result<(), SqliteError> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM flow_revisions WHERE flow_id = ?1",
        params![flow_id.0.to_string()],
        |r| r.get(0),
    )?;

    if total <= keep as i64 {
        return Ok(());
    }

    // Collect pinned revision ids from the undo/redo suffix of head.
    let pinned = collect_undo_redo_pins(conn, flow_id)?;

    // Find the cutoff seq: the seq below which we delete.
    // Grab rows sorted by seq ascending; we'll delete all up to (total-keep).
    let delete_count = (total - keep as i64).max(0) as u32;

    let cutoff_seq: Option<i64> = {
        let mut stmt = conn.prepare(
            "SELECT seq FROM flow_revisions \
             WHERE flow_id = ?1 \
             ORDER BY seq ASC \
             LIMIT ?2",
        )?;
        let rows: Vec<i64> = stmt
            .query_map(params![flow_id.0.to_string(), delete_count], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        rows.last().copied()
    };

    let cutoff_seq = match cutoff_seq {
        Some(s) => s,
        None => return Ok(()),
    };

    // Identify first survivor id (seq = cutoff_seq + 1 normally, but
    // pinned rows may push it up).
    let first_survivor_id: Option<String> = conn
        .query_row(
            "SELECT id FROM flow_revisions \
             WHERE flow_id = ?1 AND seq > ?2 \
             ORDER BY seq ASC LIMIT 1",
            params![flow_id.0.to_string(), cutoff_seq],
            |r| r.get(0),
        )
        .ok();

    let tx = conn.transaction()?;

    // Ensure first survivor has a snapshot (inherit from highest-seq
    // snapshot in the rows being deleted, or from its own seq).
    if let Some(ref survivor_id) = first_survivor_id {
        let needs_snapshot: bool = tx.query_row(
            "SELECT snapshot IS NULL FROM flow_revisions WHERE id = ?1",
            params![survivor_id],
            |r| r.get(0),
        )?;

        if needs_snapshot {
            // Find highest snapshot in the rows to be deleted.
            let snap: Option<String> = tx
                .query_row(
                    "SELECT snapshot FROM flow_revisions \
                     WHERE flow_id = ?1 AND seq <= ?2 AND snapshot IS NOT NULL \
                     ORDER BY seq DESC LIMIT 1",
                    params![flow_id.0.to_string(), cutoff_seq],
                    |r| r.get(0),
                )
                .ok()
                .flatten();

            if let Some(snap) = snap {
                tx.execute(
                    "UPDATE flow_revisions SET snapshot = ?1 WHERE id = ?2",
                    params![snap, survivor_id],
                )?;
            }
        }
    }

    // Delete everything at or below cutoff, except pinned rows.
    if pinned.is_empty() {
        tx.execute(
            "DELETE FROM flow_revisions WHERE flow_id = ?1 AND seq <= ?2",
            params![flow_id.0.to_string(), cutoff_seq],
        )?;
    } else {
        // SQLite doesn't support array params natively; build placeholders.
        let placeholders: String = pinned
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 3))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "DELETE FROM flow_revisions \
             WHERE flow_id = ?1 AND seq <= ?2 AND id NOT IN ({placeholders})"
        );
        let mut stmt = tx.prepare(&sql)?;
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(flow_id.0.to_string()),
            Box::new(cutoff_seq),
        ];
        for pin in &pinned {
            params_vec.push(Box::new(pin.clone()));
        }
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();
        stmt.execute(params_refs.as_slice())?;
    }

    tx.commit().map_err(Into::into)
}

/// Walk backward from the current head through contiguous undo/redo ops
/// and collect every `target_rev_id` — these must not be pruned because
/// redo reconstruction depends on them.
fn collect_undo_redo_pins(
    conn: &mut Connection,
    flow_id: FlowId,
) -> Result<Vec<String>, SqliteError> {
    // Get the head revision.
    let head = head_revision_row(conn, flow_id)?;
    let mut pins: Vec<String> = Vec::new();
    let mut current = head;

    loop {
        let rev = match current {
            None => break,
            Some(r) => r,
        };
        let is_undo_redo = matches!(rev.op, RevisionOp::Undo | RevisionOp::Redo);
        if !is_undo_redo {
            break;
        }
        if let Some(target) = rev.target_rev_id {
            pins.push(target.0.to_string());
        }
        // Walk to parent.
        current = match rev.parent_id {
            Some(pid) => get_revision_row(conn, pid)?,
            None => None,
        };
    }

    Ok(pins)
}

// ── Shared helpers ───────────────────────────────────────────────────────────

fn parse_uuid_rusqlite(s: &str) -> Result<uuid::Uuid, rusqlite::Error> {
    uuid::Uuid::from_str(s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })
}

/// Thin std::error::Error wrapper around String for rusqlite conversions.
#[derive(Debug)]
struct StringError(String);
impl std::fmt::Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for StringError {}
