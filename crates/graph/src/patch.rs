//! Structural mutation of nodes — rename, move, and generic patch.
//!
//! See `docs/design/NODE-MUTATION.md` for the contract. This module owns
//! the only way a node's identity (its path / its parent) can change
//! after creation. Slot writes, lifecycle transitions, and link edits
//! live elsewhere.
//!
//! Three public verbs are exposed on [`GraphStore`]:
//!
//! | Verb          | Purpose                                     |
//! |---------------|---------------------------------------------|
//! | `rename_node` | Change last-segment name; parent stays.     |
//! | `move_node`   | Change parent; name stays.                  |
//! | `patch_node`  | Sparse `NodePatch { name?, parent? }`.      |
//!
//! All three funnel into the same internal `apply_repath` so subtree
//! re-pathing, repo write-through, and event emission share one code
//! path.

use spi::{NodeId, NodePath};

use crate::error::GraphError;
use crate::event::GraphEvent;
use crate::node::NodeRecord;
use crate::store::{collect_subtree, GraphStore, StoreInner};

/// Sparse patch for a node's identity. At least one field must be
/// `Some`; an empty patch is rejected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodePatch {
    /// New last-segment name. Non-empty, no `/`.
    pub name: Option<String>,
    /// New parent path. Must exist and accept this node's kind.
    pub parent: Option<NodePath>,
}

impl NodePatch {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.parent.is_none()
    }
}

impl GraphStore {
    /// Rename a node — change its path's last segment. Descendants are
    /// re-pathed so the subtree keeps its shape. The root cannot be
    /// renamed; a name collision at the new location is rejected.
    pub fn rename_node(
        &self,
        path: &NodePath,
        new_name: &str,
    ) -> Result<NodePath, GraphError> {
        validate_name(new_name)?;
        let parent = path
            .parent()
            .ok_or_else(|| GraphError::InvalidNodeName(new_name.to_string()))?;
        let new_path = parent.child(new_name);
        self.apply_repath(path, new_path)
    }

    /// Move a node under a new parent. The node keeps its own name.
    /// Descendants follow. The new parent must exist.
    pub fn move_node(
        &self,
        path: &NodePath,
        new_parent: &NodePath,
    ) -> Result<NodePath, GraphError> {
        let name = path.name();
        if name == "/" {
            return Err(GraphError::InvalidNodeName("/".to_string()));
        }
        let new_path = new_parent.child(name);
        self.apply_repath(path, new_path)
    }

    /// Apply a sparse [`NodePatch`]. Composes `rename_node` and/or
    /// `move_node` based on which fields are set. An empty patch is
    /// rejected.
    pub fn patch_node(
        &self,
        path: &NodePath,
        patch: NodePatch,
    ) -> Result<NodePath, GraphError> {
        match (patch.name, patch.parent) {
            (None, None) => Err(GraphError::InvalidNodeName(String::new())),
            (Some(name), None) => self.rename_node(path, &name),
            (None, Some(parent)) => self.move_node(path, &parent),
            (Some(name), Some(parent)) => {
                validate_name(&name)?;
                let new_path = parent.child(&name);
                self.apply_repath(path, new_path)
            }
        }
    }

    // Internal — single entry point shared by the three public verbs.
    fn apply_repath(
        &self,
        old_path: &NodePath,
        new_path: NodePath,
    ) -> Result<NodePath, GraphError> {
        if new_path == *old_path {
            return Ok(new_path);
        }
        let mut g = self.write_inner();
        guard_target_exists(&g, old_path)?;
        guard_new_path_free(&g, &new_path)?;
        let plan = plan_repath(&g, old_path, &new_path)?;
        self.commit_repath(&mut g, &plan)?;
        drop(g);
        self.emit_renamed(&plan);
        Ok(new_path)
    }

    // Step 1 of apply_repath: persist every affected record with its new
    // path (repo-first). Then update in-memory maps. Errors bail out
    // before any memory mutation so a half-applied state is impossible.
    fn commit_repath(
        &self,
        g: &mut StoreInner,
        plan: &[RepathEntry],
    ) -> Result<(), GraphError> {
        let mut staged: Vec<NodeRecord> = Vec::with_capacity(plan.len());
        for entry in plan {
            let mut rec = g
                .by_id
                .get(&entry.id)
                .cloned()
                .ok_or_else(|| GraphError::NotFound(entry.old.clone()))?;
            rec.path = entry.new.clone();
            self.repo_save_node(&rec)?;
            staged.push(rec);
        }
        for rec in staged {
            g.by_id.insert(rec.id, rec);
        }
        for entry in plan {
            g.by_path.remove(&entry.old);
        }
        for entry in plan {
            g.by_path.insert(entry.new.clone(), entry.id);
        }
        Ok(())
    }

    fn emit_renamed(&self, plan: &[RepathEntry]) {
        for entry in plan {
            self.sink.emit(GraphEvent::NodeRenamed {
                id: entry.id,
                old_path: entry.old.clone(),
                new_path: entry.new.clone(),
            });
        }
    }
}

// ---- helpers ----------------------------------------------------------

#[derive(Debug)]
struct RepathEntry {
    id: NodeId,
    old: NodePath,
    new: NodePath,
}

fn validate_name(name: &str) -> Result<(), GraphError> {
    if name.is_empty() || name.contains('/') {
        return Err(GraphError::InvalidNodeName(name.to_string()));
    }
    Ok(())
}

fn guard_target_exists(g: &StoreInner, path: &NodePath) -> Result<(), GraphError> {
    if g.by_path.contains_key(path) {
        Ok(())
    } else {
        Err(GraphError::NotFound(path.clone()))
    }
}

fn guard_new_path_free(g: &StoreInner, new_path: &NodePath) -> Result<(), GraphError> {
    if !g.by_path.contains_key(new_path) {
        return Ok(());
    }
    let parent = new_path
        .parent()
        .unwrap_or_else(NodePath::root);
    Err(GraphError::NameCollision {
        parent,
        name: new_path.name().to_string(),
    })
}

/// Build the full (old → new) mapping for the target and every
/// descendant. Descendant paths are derived by prefix substitution.
fn plan_repath(
    g: &StoreInner,
    old_root: &NodePath,
    new_root: &NodePath,
) -> Result<Vec<RepathEntry>, GraphError> {
    let id = *g
        .by_path
        .get(old_root)
        .ok_or_else(|| GraphError::NotFound(old_root.clone()))?;
    let mut ids: Vec<NodeId> = Vec::new();
    collect_subtree(&g.by_id, id, &mut ids);

    let old_prefix = old_root.as_str().to_string();
    let new_prefix = new_root.as_str().to_string();

    let mut plan = Vec::with_capacity(ids.len());
    for nid in ids {
        let rec = g
            .by_id
            .get(&nid)
            .ok_or_else(|| GraphError::NotFound(old_root.clone()))?;
        let old = rec.path.clone();
        let new = if old == *old_root {
            new_root.clone()
        } else {
            let suffix = &old.as_str()[old_prefix.len()..];
            let composed = format!("{new_prefix}{suffix}");
            composed
                .parse::<NodePath>()
                .map_err(|_| GraphError::InvalidNodeName(composed))?
        };
        plan.push(RepathEntry { id: nid, old, new });
    }
    Ok(plan)
}

// Tests live in `crates/graph/tests/rename.rs` following the existing
// integration-test convention for this crate.
