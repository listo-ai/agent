//! Entity types for flow / node-settings revision history.
//!
//! Physical DDL lives in `data-sqlite` / `data-postgres`; these are
//! the logical projections that domain code works with.

use serde::{Deserialize, Serialize};

use crate::ids::{FlowId, NodeId, RevisionId};

/// The operation that produced a revision row.
///
/// `op` is load-bearing for undo/redo reconstruction — the server
/// walks the log and branches on this field when computing the next
/// undo / redo target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionOp {
    Create,
    Edit,
    Undo,
    Redo,
    Revert,
    Import,
    Duplicate,
    Paste,
}

impl RevisionOp {
    /// True for ops that represent a forward (non-undo/redo) edit.
    pub fn is_forward(&self) -> bool {
        matches!(
            self,
            RevisionOp::Create
                | RevisionOp::Edit
                | RevisionOp::Import
                | RevisionOp::Duplicate
                | RevisionOp::Paste
                | RevisionOp::Revert
        )
    }
}

impl std::fmt::Display for RevisionOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Create => "create",
            Self::Edit => "edit",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Revert => "revert",
            Self::Import => "import",
            Self::Duplicate => "duplicate",
            Self::Paste => "paste",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for RevisionOp {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "create" => Ok(Self::Create),
            "edit" => Ok(Self::Edit),
            "undo" => Ok(Self::Undo),
            "redo" => Ok(Self::Redo),
            "revert" => Ok(Self::Revert),
            "import" => Ok(Self::Import),
            "duplicate" => Ok(Self::Duplicate),
            "paste" => Ok(Self::Paste),
            other => Err(format!("unknown op: {other}")),
        }
    }
}

/// A single entry in the `flow_revisions` append-only log.
///
/// Phase 1: `patch` is always an empty RFC 6902 array (`[]`); every
/// revision carries a full `snapshot`. Differential patching is a
/// Phase 2 optimisation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowRevision {
    pub id: RevisionId,
    pub flow_id: FlowId,
    /// Previous revision in the chain for this flow; `None` on first revision.
    pub parent_id: Option<RevisionId>,
    /// Monotonic per-flow counter used for optimistic-concurrency checks.
    pub seq: i64,
    /// User / principal that authored this revision.
    pub author: String,
    pub op: RevisionOp,
    /// For `undo` / `redo` / `revert`: the revision whose content this
    /// revision restores.  `None` for forward edits.
    ///
    /// **For `undo`**: points to the most-recent forward revision that was
    /// stepped over — i.e. the revision that redo can bring back.
    /// **For `redo`**: same semantics — the undo entry being reversed.
    pub target_rev_id: Option<RevisionId>,
    /// Short human label, e.g. "added 2 nodes, 1 link".
    pub summary: String,
    /// RFC 6902 JSON Patch against the parent's materialised document.
    /// Phase 1: always `[]`.
    pub patch: serde_json::Value,
    /// Full document snapshot. Phase 1: always `Some(_)`.  Phase 2+: every
    /// N-th revision.
    pub snapshot: Option<serde_json::Value>,
    pub created_at: String,
}

/// The live "current" row for a flow — denormalised for fast reads.
///
/// Reads of the live document never touch `flow_revisions`; they just
/// read this row. Revisions are only consulted for history, undo, and
/// revert endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDocument {
    pub id: FlowId,
    pub name: String,
    /// Serialised JSON document (the materialised current state).
    pub document: serde_json::Value,
    /// Most recently appended revision. `None` only before the first edit
    /// after row creation.
    pub head_revision_id: Option<RevisionId>,
    /// Per-flow monotonic counter; mirrors the latest revision's `seq`.
    pub head_seq: i64,
}

/// A single entry in the `node_setting_revisions` table.
/// Phase 2; struct defined here so the schema can be migrated in Phase 1.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSettingRevision {
    pub id: RevisionId,
    pub flow_id: FlowId,
    pub node_id: NodeId,
    pub parent_id: Option<RevisionId>,
    pub seq: i64,
    pub author: String,
    pub op: RevisionOp,
    pub target_rev_id: Option<RevisionId>,
    pub schema_version: String,
    pub patch: serde_json::Value,
    pub snapshot: Option<serde_json::Value>,
    pub created_at: String,
}
