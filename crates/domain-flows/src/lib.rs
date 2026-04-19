//! Flow document service — create, edit, undo, redo, revert, history.
//!
//! This is the domain layer between HTTP handlers and the repo.  All
//! concurrency checks (optimistic `expected_head`) are enforced here so
//! endpoint handlers stay thin.
//!
//! See docs/sessions/UNDO-REDO.md for the full design.

use std::sync::Arc;

use data_entities::{FlowDocument, FlowId, FlowRevision, RevisionId, RevisionOp};
use data_repos::{FlowRevisionRepo, RepoError};
use thiserror::Error;
use tracing::instrument;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum FlowError {
    #[error("flow not found: {0}")]
    NotFound(FlowId),

    #[error("revision not found: {0}")]
    RevisionNotFound(RevisionId),

    #[error("conflict: expected head {expected} but current head is {actual:?}")]
    Conflict {
        expected: RevisionId,
        actual: Option<RevisionId>,
    },

    #[error("conflict: expected redo target {expected} but computed target is {actual}")]
    StaleRedoCursor {
        expected: RevisionId,
        actual: RevisionId,
    },

    #[error("nothing to undo for flow {0}")]
    NothingToUndo(FlowId),

    #[error("nothing to redo for flow {0}")]
    NothingToRedo(FlowId),

    #[error("revision {0} has no snapshot (not yet materialised)")]
    NoSnapshot(RevisionId),

    #[error("storage error: {0}")]
    Repo(#[from] RepoError),
}

// ── Service ───────────────────────────────────────────────────────────────────

/// High-level service for flow documents and their revision history.
///
/// Clone-cheap: wraps an `Arc<dyn FlowRevisionRepo>`.
#[derive(Clone)]
pub struct FlowService {
    repo: Arc<dyn FlowRevisionRepo>,
    /// Per-flow revision cap (default 200, matches UNDO-REDO.md).
    revision_cap: u32,
}

impl FlowService {
    pub fn new(repo: Arc<dyn FlowRevisionRepo>) -> Self {
        Self {
            repo,
            revision_cap: 200,
        }
    }

    pub fn with_revision_cap(mut self, cap: u32) -> Self {
        self.revision_cap = cap;
        self
    }
}

// ── Flow CRUD ─────────────────────────────────────────────────────────────────

impl FlowService {
    /// Create a new flow with an initial `create` revision.
    #[instrument(skip_all)]
    pub fn create_flow(
        &self,
        name: impl Into<String>,
        document: serde_json::Value,
        author: impl Into<String>,
    ) -> Result<FlowDocument, FlowError> {
        let flow_id = FlowId::new_random();
        let rev_id = RevisionId::new_random();
        let author = author.into();

        // Persist the blank flow row first (FOREIGN KEY target for revision).
        let flow = FlowDocument {
            id: flow_id,
            name: name.into(),
            document: document.clone(),
            head_revision_id: None,
            head_seq: 0,
        };
        self.repo.save_flow(&flow)?;

        // Write first revision.
        let rev = FlowRevision {
            id: rev_id,
            flow_id,
            parent_id: None,
            seq: 1,
            author,
            op: RevisionOp::Create,
            target_rev_id: None,
            summary: "created".into(),
            patch: serde_json::json!([]),
            snapshot: Some(document.clone()),
            created_at: String::new(),
        };
        self.repo.append_revision(&rev, &document)?;
        // Fetch back the updated row (head_revision_id now set).
        self.repo
            .get_flow(flow_id)?
            .ok_or(FlowError::NotFound(flow_id))
    }

    pub fn get_flow(&self, id: FlowId) -> Result<FlowDocument, FlowError> {
        self.repo.get_flow(id)?.ok_or(FlowError::NotFound(id))
    }

    pub fn list_flows(&self, limit: u32, offset: u32) -> Result<Vec<FlowDocument>, FlowError> {
        Ok(self.repo.list_flows(limit, offset)?)
    }

    /// Delete the flow and all its revision history.
    ///
    /// `expected_head` must match the current head for optimistic concurrency.
    pub fn delete_flow(
        &self,
        id: FlowId,
        expected_head: Option<RevisionId>,
    ) -> Result<(), FlowError> {
        let flow = self.require_flow(id)?;
        self.check_head(flow.head_revision_id, expected_head)?;
        Ok(self.repo.delete_flow(id)?)
    }
}

// ── Edit / Undo / Redo / Revert ───────────────────────────────────────────────

impl FlowService {
    /// Apply a new forward edit and append an `edit` revision.
    ///
    /// If the flow has no revisions yet (brand-new import with no `create`
    /// call), the first edit bootstraps the chain.
    #[instrument(skip_all)]
    pub fn edit(
        &self,
        flow_id: FlowId,
        expected_head: Option<RevisionId>,
        new_document: serde_json::Value,
        author: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<RevisionId, FlowError> {
        let flow = self.require_flow(flow_id)?;
        self.check_head(flow.head_revision_id, expected_head)?;

        let rev_id = RevisionId::new_random();
        let seq = flow.head_seq + 1;
        let rev = FlowRevision {
            id: rev_id,
            flow_id,
            parent_id: flow.head_revision_id,
            seq,
            author: author.into(),
            op: RevisionOp::Edit,
            target_rev_id: None,
            summary: summary.into(),
            patch: serde_json::json!([]),
            snapshot: Some(new_document.clone()),
            created_at: String::new(),
        };
        self.repo.append_revision(&rev, &new_document)?;
        self.maybe_prune(flow_id);
        Ok(rev_id)
    }

    /// Undo the last logical edit and append an `undo` revision.
    ///
    /// Undo is not "delete" — it appends a new revision whose content equals
    /// the state N-1 steps back in the logical history.  Redo is
    /// reconstructable from the log alone (no cursor stored).
    #[instrument(skip_all)]
    pub fn undo(
        &self,
        flow_id: FlowId,
        expected_head: Option<RevisionId>,
        author: impl Into<String>,
    ) -> Result<RevisionId, FlowError> {
        let flow = self.require_flow(flow_id)?;
        self.check_head(flow.head_revision_id, expected_head)?;

        let head = self
            .repo
            .head_revision(flow_id)?
            .ok_or(FlowError::NothingToUndo(flow_id))?;

        let (target_rev_id, restore_snapshot) = self.find_undo_args(&head)?;
        let rev_id = RevisionId::new_random();
        let seq = flow.head_seq + 1;
        let rev = FlowRevision {
            id: rev_id,
            flow_id,
            parent_id: Some(head.id),
            seq,
            author: author.into(),
            op: RevisionOp::Undo,
            target_rev_id: Some(target_rev_id),
            summary: format!("undo {}", target_rev_id),
            patch: serde_json::json!([]),
            snapshot: Some(restore_snapshot.clone()),
            created_at: String::new(),
        };
        self.repo.append_revision(&rev, &restore_snapshot)?;
        Ok(rev_id)
    }

    /// Redo the next undone edit.
    ///
    /// The redo target is derived purely from the revision log — no cursor
    /// is stored.  Optionally accepts `expected_target` for the two-tab
    /// stale-cursor case; if provided and it doesn't match, returns
    /// `FlowError::StaleRedoCursor`.
    #[instrument(skip_all)]
    pub fn redo(
        &self,
        flow_id: FlowId,
        expected_head: Option<RevisionId>,
        expected_target: Option<RevisionId>,
        author: impl Into<String>,
    ) -> Result<RevisionId, FlowError> {
        let flow = self.require_flow(flow_id)?;
        self.check_head(flow.head_revision_id, expected_head)?;

        let head = self
            .repo
            .head_revision(flow_id)?
            .ok_or(FlowError::NothingToRedo(flow_id))?;

        let (redo_target_id, restore_snapshot) = self.find_redo_args(flow_id, &head)?;

        if let Some(et) = expected_target {
            if et != redo_target_id {
                return Err(FlowError::StaleRedoCursor {
                    expected: et,
                    actual: redo_target_id,
                });
            }
        }

        let rev_id = RevisionId::new_random();
        let seq = flow.head_seq + 1;
        let rev = FlowRevision {
            id: rev_id,
            flow_id,
            parent_id: Some(head.id),
            seq,
            author: author.into(),
            op: RevisionOp::Redo,
            target_rev_id: Some(redo_target_id),
            summary: format!("redo {}", redo_target_id),
            patch: serde_json::json!([]),
            snapshot: Some(restore_snapshot.clone()),
            created_at: String::new(),
        };
        self.repo.append_revision(&rev, &restore_snapshot)?;
        Ok(rev_id)
    }

    /// Revert the flow to the state captured in `target_rev_id`.
    ///
    /// This appends a `revert` revision — nothing is deleted.
    #[instrument(skip_all)]
    pub fn revert(
        &self,
        flow_id: FlowId,
        expected_head: Option<RevisionId>,
        target_rev_id: RevisionId,
        author: impl Into<String>,
    ) -> Result<RevisionId, FlowError> {
        let flow = self.require_flow(flow_id)?;
        self.check_head(flow.head_revision_id, expected_head)?;

        let head = self.repo.head_revision(flow_id)?;
        let head_id = head.map(|h| h.id);
        let target_rev = self
            .repo
            .get_revision(target_rev_id)?
            .ok_or(FlowError::RevisionNotFound(target_rev_id))?;
        let target_doc = target_rev
            .snapshot
            .ok_or(FlowError::NoSnapshot(target_rev_id))?;

        let rev_id = RevisionId::new_random();
        let seq = flow.head_seq + 1;
        let rev = FlowRevision {
            id: rev_id,
            flow_id,
            parent_id: head_id,
            seq,
            author: author.into(),
            op: RevisionOp::Revert,
            target_rev_id: Some(target_rev_id),
            summary: format!("revert to {}", target_rev_id),
            patch: serde_json::json!([]),
            snapshot: Some(target_doc.clone()),
            created_at: String::new(),
        };
        self.repo.append_revision(&rev, &target_doc)?;
        self.maybe_prune(flow_id);
        Ok(rev_id)
    }
}

// ── History queries ───────────────────────────────────────────────────────────

impl FlowService {
    pub fn list_revisions(
        &self,
        flow_id: FlowId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<FlowRevision>, FlowError> {
        Ok(self.repo.list_revisions(flow_id, limit, offset)?)
    }

    /// Return the materialised document at a specific revision.
    pub fn document_at(
        &self,
        flow_id: FlowId,
        rev_id: RevisionId,
    ) -> Result<serde_json::Value, FlowError> {
        // Ensure the revision belongs to this flow.
        let rev = self
            .repo
            .get_revision(rev_id)?
            .ok_or(FlowError::RevisionNotFound(rev_id))?;
        if rev.flow_id != flow_id {
            return Err(FlowError::RevisionNotFound(rev_id));
        }
        rev.snapshot.ok_or(FlowError::NoSnapshot(rev_id))
    }

    pub fn get_revision(&self, rev_id: RevisionId) -> Result<FlowRevision, FlowError> {
        self.repo
            .get_revision(rev_id)?
            .ok_or(FlowError::RevisionNotFound(rev_id))
    }
}

// ── Undo / redo chain algorithms ──────────────────────────────────────────────

impl FlowService {
    /// Determine (target_rev_id, document) for the next undo operation.
    ///
    /// `target_rev_id` = the revision being stepped over (what redo restores).
    /// `document`      = the snapshot to materialise as the new state.
    fn find_undo_args(
        &self,
        head: &FlowRevision,
    ) -> Result<(RevisionId, serde_json::Value), FlowError> {
        // The "logical current" revision = the one whose snapshot is the
        // current visible content.
        let logical_current = self.find_logical_current(head)?;

        // The revision before the logical current = what we're restoring.
        let prev_id = logical_current
            .parent_id
            .ok_or(FlowError::NothingToUndo(head.flow_id))?;

        let prev = self
            .repo
            .get_revision(prev_id)?
            .ok_or(FlowError::RevisionNotFound(prev_id))?;

        let snapshot = prev.snapshot.ok_or(FlowError::NoSnapshot(prev_id))?;

        // target = the logical-current revision (what redo will restore).
        Ok((logical_current.id, snapshot))
    }

    /// Determine (target_rev_id, document) for the next redo operation.
    ///
    /// Scans the contiguous undo/redo suffix of the revision chain from `head`
    /// backwards and computes the outstanding redo stack.  The next redo =
    /// `stack[0]` (the most-recent unmatched undo).
    fn find_redo_args(
        &self,
        flow_id: FlowId,
        head: &FlowRevision,
    ) -> Result<(RevisionId, serde_json::Value), FlowError> {
        let mut stack: Vec<RevisionId> = Vec::new();
        let mut redo_credits: u32 = 0;
        let mut current_opt: Option<FlowRevision> = Some(head.clone());

        loop {
            let current = match current_opt {
                None => break,
                Some(r) => r,
            };

            match &current.op {
                RevisionOp::Undo => {
                    if redo_credits > 0 {
                        redo_credits -= 1;
                        // This undo is cancelled by an earlier redo; skip.
                    } else {
                        if let Some(t) = current.target_rev_id {
                            stack.push(t);
                        }
                    }
                }
                RevisionOp::Redo => {
                    redo_credits += 1;
                }
                _ => {
                    // Forward edit — end of the undo/redo suffix.
                    break;
                }
            }

            current_opt = match current.parent_id {
                Some(pid) => self.repo.get_revision(pid)?,
                None => None,
            };
        }

        let target_id = stack
            .into_iter()
            .next()
            .ok_or(FlowError::NothingToRedo(flow_id))?;

        let target_rev = self
            .repo
            .get_revision(target_id)?
            .ok_or(FlowError::RevisionNotFound(target_id))?;

        let snapshot = target_rev
            .snapshot
            .ok_or(FlowError::NoSnapshot(target_id))?;

        Ok((target_id, snapshot))
    }

    /// Find the revision whose snapshot represents the current logical state.
    ///
    /// - Forward edit: the revision itself.
    /// - Undo: `target.parent` (undo stepped over `target`, so current = target's predecessor).
    /// - Redo: `target` (redo restored the target revision's content).
    fn find_logical_current(&self, rev: &FlowRevision) -> Result<FlowRevision, FlowError> {
        match &rev.op {
            op if op.is_forward() => Ok(rev.clone()),
            RevisionOp::Redo => {
                let target_id = rev
                    .target_rev_id
                    .ok_or(FlowError::NothingToUndo(rev.flow_id))?;
                self.repo
                    .get_revision(target_id)?
                    .ok_or(FlowError::RevisionNotFound(target_id))
            }
            RevisionOp::Undo => {
                let target_id = rev
                    .target_rev_id
                    .ok_or(FlowError::NothingToUndo(rev.flow_id))?;
                let target = self
                    .repo
                    .get_revision(target_id)?
                    .ok_or(FlowError::RevisionNotFound(target_id))?;
                // logical_current = target's predecessor
                let prev_id = target
                    .parent_id
                    .ok_or(FlowError::NothingToUndo(rev.flow_id))?;
                self.repo
                    .get_revision(prev_id)?
                    .ok_or(FlowError::RevisionNotFound(prev_id))
            }
            _ => unreachable!(),
        }
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

impl FlowService {
    fn require_flow(&self, id: FlowId) -> Result<FlowDocument, FlowError> {
        self.repo.get_flow(id)?.ok_or(FlowError::NotFound(id))
    }

    fn check_head(
        &self,
        actual: Option<RevisionId>,
        expected: Option<RevisionId>,
    ) -> Result<(), FlowError> {
        // None means "skip OCC check" — caller doesn't care about concurrency.
        let Some(expected) = expected else {
            return Ok(());
        };
        if actual != Some(expected) {
            return Err(FlowError::Conflict { expected, actual });
        }
        Ok(())
    }

    fn maybe_prune(&self, flow_id: FlowId) {
        if let Err(e) = self.repo.prune_revisions(flow_id, self.revision_cap) {
            tracing::warn!(
                flow_id = %flow_id,
                error = %e,
                "revision pruning failed (non-fatal)"
            );
        }
    }
}
