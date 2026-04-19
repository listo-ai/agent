//! `GraphRepo` implementation over SQLite.
//!
//! Writes are serialised through a single connection under a mutex.
//! SQLite's single-writer/WAL-reader model means this matches the
//! engine's access pattern without extra coordination. Reads share the
//! same connection today; a read-replica pool is a Stage 7 concern.
//!
//! Every multi-row write runs inside an explicit transaction so a
//! failure mid-way leaves the DB untouched \u{2014} the paired in-memory
//! store mutation in `graph::GraphStore` is the only other authority.

use std::path::Path;
use std::sync::Mutex;

use data_repos::{
    GraphRepo, GraphSnapshot, PersistedLink, PersistedNode, PersistedSlot, RepoError,
};
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::connection::{open, Location};
use crate::error::SqliteError;

pub struct SqliteGraphRepo {
    conn: Mutex<Connection>,
}

impl SqliteGraphRepo {
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
            .map_err(|_| RepoError::Backend("sqlite mutex poisoned".into()))?;
        Ok(f(&mut g)?)
    }
}

impl GraphRepo for SqliteGraphRepo {
    fn load(&self) -> Result<GraphSnapshot, RepoError> {
        self.with_conn(load_all)
    }

    fn save_node(&self, node: &PersistedNode) -> Result<(), RepoError> {
        self.with_conn(|c| save_node_row(c, node))
    }

    fn delete_nodes(&self, ids: &[Uuid]) -> Result<(), RepoError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.with_conn(|c| delete_node_rows(c, ids))
    }

    fn upsert_slot(&self, slot: &PersistedSlot) -> Result<(), RepoError> {
        self.with_conn(|c| upsert_slot_row(c, slot))
    }

    fn save_link(&self, link: &PersistedLink) -> Result<(), RepoError> {
        self.with_conn(|c| save_link_row(c, link))
    }

    fn delete_links(&self, ids: &[Uuid]) -> Result<(), RepoError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.with_conn(|c| delete_link_rows(c, ids))
    }
}

fn load_all(conn: &mut Connection) -> Result<GraphSnapshot, SqliteError> {
    // Order by `path` so parents precede children \u{2014} materialised paths
    // sort correctly lexicographically (`/` < `/a` < `/a/b`).
    let mut nodes_stmt = conn
        .prepare("SELECT id, parent_id, kind_id, path, name, lifecycle FROM nodes ORDER BY path")?;
    let nodes: Vec<PersistedNode> = nodes_stmt
        .query_map([], |row| {
            Ok(PersistedNode {
                id: parse_uuid(row.get::<_, String>(0)?)?,
                parent_id: row
                    .get::<_, Option<String>>(1)?
                    .map(parse_uuid)
                    .transpose()?,
                kind_id: row.get(2)?,
                path: row.get(3)?,
                name: row.get(4)?,
                lifecycle: row.get(5)?,
            })
        })?
        .collect::<Result<_, rusqlite::Error>>()?;

    let mut slots_stmt = conn.prepare(
        "SELECT node_id, name, role, value, generation, kind FROM slots ORDER BY node_id, name",
    )?;
    let slots: Vec<PersistedSlot> = slots_stmt
        .query_map([], |row| {
            let value_text: String = row.get(3)?;
            let value: serde_json::Value = serde_json::from_str(&value_text).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(PersistedSlot {
                node_id: parse_uuid(row.get::<_, String>(0)?)?,
                name: row.get(1)?,
                role: row.get(2)?,
                value,
                generation: row.get(4)?,
                kind: row.get(5)?,
            })
        })?
        .collect::<Result<_, rusqlite::Error>>()?;

    let mut links_stmt =
        conn.prepare("SELECT id, source_node, source_slot, target_node, target_slot FROM links")?;
    let links: Vec<PersistedLink> = links_stmt
        .query_map([], |row| {
            Ok(PersistedLink {
                id: parse_uuid(row.get::<_, String>(0)?)?,
                source_node: parse_uuid(row.get::<_, String>(1)?)?,
                source_slot: row.get(2)?,
                target_node: parse_uuid(row.get::<_, String>(3)?)?,
                target_slot: row.get(4)?,
            })
        })?
        .collect::<Result<_, rusqlite::Error>>()?;

    Ok(GraphSnapshot {
        nodes,
        slots,
        links,
    })
}

fn save_node_row(conn: &mut Connection, n: &PersistedNode) -> Result<(), SqliteError> {
    conn.execute(
        "INSERT INTO nodes (id, parent_id, kind_id, path, name, lifecycle)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            parent_id=excluded.parent_id,
            kind_id=excluded.kind_id,
            path=excluded.path,
            name=excluded.name,
            lifecycle=excluded.lifecycle",
        params![
            n.id.to_string(),
            n.parent_id.map(|p| p.to_string()),
            n.kind_id,
            n.path,
            n.name,
            n.lifecycle,
        ],
    )?;
    Ok(())
}

fn delete_node_rows(conn: &mut Connection, ids: &[Uuid]) -> Result<(), SqliteError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("DELETE FROM nodes WHERE id = ?1")?;
        for id in ids {
            stmt.execute(params![id.to_string()])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn upsert_slot_row(conn: &mut Connection, s: &PersistedSlot) -> Result<(), SqliteError> {
    let value = serde_json::to_string(&s.value)?;
    conn.execute(
        "INSERT INTO slots (node_id, name, role, value, generation, kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(node_id, name) DO UPDATE SET
            role=excluded.role,
            value=excluded.value,
            generation=excluded.generation,
            kind=excluded.kind",
        params![s.node_id.to_string(), s.name, s.role, value, s.generation, s.kind],
    )?;
    Ok(())
}

fn save_link_row(conn: &mut Connection, l: &PersistedLink) -> Result<(), SqliteError> {
    conn.execute(
        "INSERT INTO links (id, source_node, source_slot, target_node, target_slot)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO NOTHING",
        params![
            l.id.to_string(),
            l.source_node.to_string(),
            l.source_slot,
            l.target_node.to_string(),
            l.target_slot,
        ],
    )?;
    Ok(())
}

fn delete_link_rows(conn: &mut Connection, ids: &[Uuid]) -> Result<(), SqliteError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("DELETE FROM links WHERE id = ?1")?;
        for id in ids {
            stmt.execute(params![id.to_string()])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn parse_uuid(s: String) -> Result<Uuid, rusqlite::Error> {
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}
