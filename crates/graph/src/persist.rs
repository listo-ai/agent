//! Mapping between the graph's internal types and the persistence DTOs
//! in `data-repos`, plus the write-through shim used by [`GraphStore`].
//!
//! Kept out of `store.rs` so the core mutation paths stay readable and
//! the persistence concern is self-contained.

use data_repos::{GraphRepo, GraphSnapshot, PersistedLink, PersistedNode, PersistedSlot};
use serde_json::Value as JsonValue;

use crate::error::GraphError;
use crate::ids::{NodeId, NodePath};
use crate::lifecycle::Lifecycle;
use crate::link::{Link, LinkId, SlotRef};
use crate::node::NodeRecord;
use crate::slot::SlotRole;

pub(crate) fn node_to_persisted(rec: &NodeRecord) -> PersistedNode {
    PersistedNode {
        id: rec.id.0,
        parent_id: rec.parent.map(|p| p.0),
        kind_id: rec.kind.as_str().to_string(),
        path: rec.path.as_str().to_string(),
        name: rec.path.name().to_string(),
        lifecycle: lifecycle_to_str(rec.lifecycle).to_string(),
    }
}

pub(crate) fn link_to_persisted(link: &Link) -> PersistedLink {
    PersistedLink {
        id: link.id.0,
        source_node: link.source.node.0,
        source_slot: link.source.slot.clone(),
        target_node: link.target.node.0,
        target_slot: link.target.slot.clone(),
    }
}

pub(crate) fn slot_to_persisted(
    node_id: NodeId,
    name: &str,
    role: SlotRole,
    value: &JsonValue,
    generation: u64,
) -> PersistedSlot {
    PersistedSlot {
        node_id: node_id.0,
        name: name.to_string(),
        role: slot_role_to_str(role).to_string(),
        value: value.clone(),
        generation: generation as i64,
    }
}

pub(crate) fn lifecycle_to_str(l: Lifecycle) -> &'static str {
    match l {
        Lifecycle::Created => "created",
        Lifecycle::Active => "active",
        Lifecycle::Disabled => "disabled",
        Lifecycle::Stale => "stale",
        Lifecycle::Fault => "fault",
        Lifecycle::Removing => "removing",
        Lifecycle::Removed => "removed",
    }
}

pub(crate) fn lifecycle_from_str(s: &str) -> Result<Lifecycle, GraphError> {
    match s {
        "created" => Ok(Lifecycle::Created),
        "active" => Ok(Lifecycle::Active),
        "disabled" => Ok(Lifecycle::Disabled),
        "stale" => Ok(Lifecycle::Stale),
        "fault" => Ok(Lifecycle::Fault),
        "removing" => Ok(Lifecycle::Removing),
        "removed" => Ok(Lifecycle::Removed),
        other => Err(GraphError::Restore(format!("unknown lifecycle `{other}`"))),
    }
}

pub(crate) fn slot_role_to_str(r: SlotRole) -> &'static str {
    match r {
        SlotRole::Config => "config",
        SlotRole::Input => "input",
        SlotRole::Output => "output",
        SlotRole::Status => "status",
    }
}

pub(crate) fn snapshot_to_link(l: PersistedLink) -> Link {
    Link {
        id: LinkId(l.id),
        source: SlotRef::new(NodeId(l.source_node), l.source_slot),
        target: SlotRef::new(NodeId(l.target_node), l.target_slot),
    }
}

pub(crate) fn snapshot_to_path(s: &str) -> NodePath {
    s.parse().unwrap_or_else(|_| NodePath::root())
}

/// Thin wrapper so store.rs can stringify repo errors into
/// `GraphError::Backend` without seeing the trait's Result type.
pub(crate) fn repo_call<T>(result: Result<T, data_repos::RepoError>) -> Result<T, GraphError> {
    result.map_err(|e| GraphError::Backend(e.to_string()))
}

pub(crate) fn load_snapshot(repo: &dyn GraphRepo) -> Result<GraphSnapshot, GraphError> {
    repo_call(repo.load())
}
