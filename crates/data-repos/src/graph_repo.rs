//! Durable view of the graph.
//!
//! Synchronous by design: matches the `GraphStore`'s sync API (see
//! `docs/design/EVERYTHING-AS-NODE.md` \u{2014} the graph crate is the core
//! substrate and does not own a runtime). Edge backends (rusqlite) are
//! naturally sync; a future async backend (e.g. sqlx on Postgres) wraps
//! its client with `block_on` or exposes a second async-flavoured
//! trait at the data-postgres crate level \u{2014} the graph itself stays sync.
//!
//! DTOs here hold primitive types only (`Uuid`, `String`, `JsonValue`)
//! so this crate does not depend on the graph crate, keeping the type
//! authority in `graph` and avoiding a circular dependency. Mapping
//! code lives in `graph::persist`.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::RepoError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedNode {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub kind_id: String,
    pub path: String,
    pub name: String,
    /// `Lifecycle` encoded as a stable lower-snake string.
    pub lifecycle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSlot {
    pub node_id: Uuid,
    pub name: String,
    /// `SlotRole` encoded as lower-snake.
    pub role: String,
    pub value: JsonValue,
    pub generation: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedLink {
    pub id: Uuid,
    pub source_node: Uuid,
    pub source_slot: String,
    pub target_node: Uuid,
    pub target_slot: String,
}

/// Full snapshot returned by [`GraphRepo::load`]. Node order must be
/// parent-before-child so the in-memory store can reconstruct
/// containment without lookups against unseen parents.
#[derive(Debug, Clone, Default)]
pub struct GraphSnapshot {
    pub nodes: Vec<PersistedNode>,
    pub slots: Vec<PersistedSlot>,
    pub links: Vec<PersistedLink>,
}

/// Synchronous repo backing the node graph. One logical contract,
/// multiple backends (`data-sqlite` today; `data-postgres` in Stage
/// 5b). See [`docs/design/EVERYTHING-AS-NODE.md`] \u{00a7} "Persistence".
pub trait GraphRepo: Send + Sync + 'static {
    /// Load everything into a snapshot. Called once at startup.
    fn load(&self) -> Result<GraphSnapshot, RepoError>;

    /// Upsert a node. Called during create and lifecycle transitions.
    fn save_node(&self, node: &PersistedNode) -> Result<(), RepoError>;

    /// Delete one or more nodes. Callers pass them in post-order
    /// (children first) so foreign-key-free backends can delete in
    /// sequence without ordering surprises.
    fn delete_nodes(&self, ids: &[Uuid]) -> Result<(), RepoError>;

    /// Upsert a slot value. The `generation` monotonically grows per
    /// (node, slot); backends must not reorder writes within a slot.
    fn upsert_slot(&self, slot: &PersistedSlot) -> Result<(), RepoError>;

    /// Insert a link. Deletion happens via [`Self::delete_links`].
    fn save_link(&self, link: &PersistedLink) -> Result<(), RepoError>;

    fn delete_links(&self, ids: &[Uuid]) -> Result<(), RepoError>;
}
