//! Repository traits for flow documents and their revision history.
//!
//! Sync surface (like `GraphRepo`) — SQLite rusqlite under the hood.

use data_entities::{FlowDocument, FlowId, FlowRevision, RevisionId};

use crate::RepoError;

/// All flow-document and flow-revision operations via a single repo.
///
/// Keeping them together avoids the need for multi-repo transactions:
/// every mutation that changes both the `flows` row and the
/// `flow_revisions` table is encapsulated in a single `Mutex<Connection>`
/// and wrapped in one SQLite transaction.
pub trait FlowRevisionRepo: Send + Sync + 'static {
    // ── Flow CRUD ────────────────────────────────────────────────────────────

    /// Return the live flow document, or `None` if not found.
    fn get_flow(&self, id: FlowId) -> Result<Option<FlowDocument>, RepoError>;

    /// Create or update the live flow row (name, document, head pointers).
    fn save_flow(&self, flow: &FlowDocument) -> Result<(), RepoError>;

    /// Delete the flow row **and** its entire revision history.
    fn delete_flow(&self, id: FlowId) -> Result<(), RepoError>;

    /// List flows, newest first (by head_seq descending).
    fn list_flows(&self, limit: u32, offset: u32) -> Result<Vec<FlowDocument>, RepoError>;

    // ── Revision log ─────────────────────────────────────────────────────────

    /// Append a new revision **and** update the live flow row atomically.
    ///
    /// The callee increments `flows.head_seq` / `flows.head_revision_id`
    /// and persists the live document from `rev.snapshot`.  The `rev.seq`
    /// value is set by the caller (= previous `head_seq + 1`).
    fn append_revision(&self, rev: &FlowRevision, new_document: &serde_json::Value)
        -> Result<(), RepoError>;

    /// Fetch a single revision by id.
    fn get_revision(&self, id: RevisionId) -> Result<Option<FlowRevision>, RepoError>;

    /// List revisions for a flow, newest first.
    fn list_revisions(
        &self,
        flow_id: FlowId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<FlowRevision>, RepoError>;

    /// The most-recently appended revision for a flow, or `None` for a
    /// flow that has no revisions yet.
    fn head_revision(&self, flow_id: FlowId) -> Result<Option<FlowRevision>, RepoError>;

    /// Delete revisions for a flow, keeping only the most recent `keep`
    /// entries.  The pruner re-snapshots the oldest surviving revision if
    /// necessary to preserve chain integrity (see design doc §Pruning).
    fn prune_revisions(&self, flow_id: FlowId, keep: u32) -> Result<(), RepoError>;
}
