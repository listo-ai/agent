//! In-memory graph store.
//!
//! The store owns the tree. Every mutation goes through one of a handful
//! of methods, each one of which runs the same placement/cardinality
//! validation against the kind registry, applies the change
//! transactionally, and emits the matching [`GraphEvent`] via the
//! configured [`EventSink`].
//!
//! Persistent backing via `data-repos` lands in Stage 5 behind this
//! same public surface.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde_json::Value as JsonValue;

use crate::containment::{Cardinality, CascadePolicy};
use crate::error::GraphError;
use crate::event::{EventSink, GraphEvent};
use crate::ids::{KindId, NodeId, NodePath};
use crate::kind::{KindManifest, KindRegistry};
use crate::lifecycle::Lifecycle;
use crate::link::{Link, LinkId, SlotRef};
use crate::node::{NodeRecord, NodeSnapshot};

pub struct GraphStore {
    kinds: KindRegistry,
    sink: Arc<dyn EventSink>,
    inner: RwLock<StoreInner>,
}

struct StoreInner {
    by_id: HashMap<NodeId, NodeRecord>,
    by_path: HashMap<NodePath, NodeId>,
    links: HashMap<LinkId, Link>,
}

impl GraphStore {
    pub fn new(kinds: KindRegistry, sink: Arc<dyn EventSink>) -> Self {
        Self {
            kinds,
            sink,
            inner: RwLock::new(StoreInner {
                by_id: HashMap::new(),
                by_path: HashMap::new(),
                links: HashMap::new(),
            }),
        }
    }

    pub fn kinds(&self) -> &KindRegistry {
        &self.kinds
    }

    /// Create the root node (`/`). Must be a kind whose containment is
    /// *free* — typically `acme.core.station`.
    pub fn create_root(&self, kind: KindId) -> Result<NodeId, GraphError> {
        let manifest = self.require_kind(&kind)?;
        let mut g = self.write_inner();
        if g.by_path.contains_key(&NodePath::root()) {
            return Err(GraphError::RootAlreadyExists);
        }
        let id = NodeId::new();
        let record = NodeRecord::new(id, kind.clone(), NodePath::root(), None);
        // Pre-materialise declared slots.
        let mut record = record;
        for slot in &manifest.slots {
            record.slots.insert(slot.name.clone(), JsonValue::Null);
        }
        g.by_id.insert(id, record);
        g.by_path.insert(NodePath::root(), id);
        drop(g);
        self.sink.emit(GraphEvent::NodeCreated {
            id,
            kind,
            path: NodePath::root(),
        });
        Ok(id)
    }

    /// Create a child node of the given kind under `parent`.
    pub fn create_child(
        &self,
        parent: &NodePath,
        kind: KindId,
        name: &str,
    ) -> Result<NodeId, GraphError> {
        if name.is_empty() || name.contains('/') {
            return Err(GraphError::InvalidNodeName(name.to_string()));
        }
        let manifest = self.require_kind(&kind)?;
        let path = parent.child(name);

        let mut g = self.write_inner();
        let parent_id = *g
            .by_path
            .get(parent)
            .ok_or_else(|| GraphError::NotFound(parent.clone()))?;
        let parent_rec = g
            .by_id
            .get(&parent_id)
            .cloned()
            .ok_or_else(|| GraphError::NotFound(parent.clone()))?;
        let parent_manifest = self.require_kind(&parent_rec.kind)?;

        // Placement check: parent may contain this kind.
        if !parent_manifest.containment.may_contain.is_empty() {
            let ok = parent_manifest.containment.may_contain.iter().any(|m| match m {
                crate::containment::ParentMatcher::Kind(k) => k == &kind,
                crate::containment::ParentMatcher::Facet(f) => manifest.facets.contains(*f),
            });
            if !ok {
                return Err(GraphError::PlacementRejected {
                    kind,
                    parent: parent.clone(),
                    parent_kind: parent_rec.kind.clone(),
                });
            }
        }

        // Placement check: this kind may live under that parent.
        if !manifest.containment.must_live_under.is_empty() {
            let ok = manifest
                .containment
                .must_live_under
                .iter()
                .any(|m| m.matches(&parent_rec.kind, &parent_manifest.facets));
            if !ok {
                return Err(GraphError::PlacementRejected {
                    kind,
                    parent: parent.clone(),
                    parent_kind: parent_rec.kind.clone(),
                });
            }
        }

        // Cardinality check.
        match manifest.containment.cardinality_per_parent {
            Cardinality::ManyPerParent => {}
            Cardinality::OnePerParent | Cardinality::ExactlyOne => {
                let existing = parent_rec
                    .children
                    .iter()
                    .filter_map(|cid| g.by_id.get(cid))
                    .any(|c| c.kind == kind);
                if existing {
                    return Err(GraphError::CardinalityExceeded {
                        kind,
                        parent: parent.clone(),
                    });
                }
            }
        }

        // Name collision under this parent.
        if g.by_path.contains_key(&path) {
            return Err(GraphError::NameCollision {
                parent: parent.clone(),
                name: name.to_string(),
            });
        }

        let id = NodeId::new();
        let mut record = NodeRecord::new(id, kind.clone(), path.clone(), Some(parent_id));
        for slot in &manifest.slots {
            record.slots.insert(slot.name.clone(), JsonValue::Null);
        }
        g.by_id.insert(id, record);
        g.by_path.insert(path.clone(), id);
        if let Some(p) = g.by_id.get_mut(&parent_id) {
            p.children.push(id);
        }
        drop(g);

        self.sink.emit(GraphEvent::NodeCreated { id, kind, path });
        Ok(id)
    }

    /// Delete the node at `path` (and its subtree if cascade policy allows).
    pub fn delete(&self, path: &NodePath) -> Result<(), GraphError> {
        let mut g = self.write_inner();
        let id = *g
            .by_path
            .get(path)
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;

        // Collect the subtree (self + descendants), depth-first post-order,
        // so children are removed before parents. Cascade policy enforced
        // on the root of the delete.
        let root_rec = g.by_id.get(&id).cloned().ok_or_else(|| GraphError::NotFound(path.clone()))?;
        let root_manifest = self.require_kind(&root_rec.kind)?;

        let mut subtree: Vec<NodeId> = Vec::new();
        collect_subtree(&g.by_id, id, &mut subtree);

        if matches!(root_manifest.containment.cascade, CascadePolicy::Deny) && subtree.len() > 1 {
            return Err(GraphError::CascadeDenied {
                path: path.clone(),
                kind: root_rec.kind.clone(),
            });
        }

        // Links to break: any link whose source or target node is in the subtree.
        let in_subtree: std::collections::HashSet<NodeId> = subtree.iter().copied().collect();
        let mut broken: Vec<(LinkId, SlotRef, SlotRef)> = Vec::new();
        g.links.retain(|lid, link| {
            let src_in = in_subtree.contains(&link.source.node);
            let tgt_in = in_subtree.contains(&link.target.node);
            if src_in || tgt_in {
                broken.push((*lid, link.source.clone(), link.target.clone()));
                false
            } else {
                true
            }
        });

        // Remove nodes depth-first post-order and collect events.
        let mut removed: Vec<(NodeId, KindId, NodePath)> = Vec::new();
        for nid in &subtree {
            if let Some(rec) = g.by_id.remove(nid) {
                g.by_path.remove(&rec.path);
                if let Some(pid) = rec.parent {
                    if let Some(p) = g.by_id.get_mut(&pid) {
                        p.children.retain(|c| c != nid);
                    }
                }
                removed.push((rec.id, rec.kind, rec.path));
            }
        }

        drop(g);

        // Emit events now that state is consistent.
        for (id, kind, path) in removed {
            self.sink.emit(GraphEvent::NodeRemoved { id, kind, path });
        }
        for (link_id, source, target) in broken {
            let (broken_end, surviving_end) = if in_subtree.contains(&source.node) {
                (source.clone(), target.clone())
            } else {
                (target.clone(), source.clone())
            };
            self.sink.emit(GraphEvent::LinkRemoved {
                id: link_id,
                source,
                target,
            });
            self.sink.emit(GraphEvent::LinkBroken {
                id: link_id,
                broken_end,
                surviving_end,
            });
        }
        Ok(())
    }

    /// Write a value to a slot. The slot must exist on the node and be
    /// declared `writable` by the kind manifest.
    pub fn write_slot(
        &self,
        path: &NodePath,
        slot: &str,
        value: JsonValue,
    ) -> Result<u64, GraphError> {
        let mut g = self.write_inner();
        let id = *g
            .by_path
            .get(path)
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;
        let rec = g
            .by_id
            .get_mut(&id)
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;
        let gen = rec.slots.write(slot, value.clone()).ok_or_else(|| {
            GraphError::BadLink(format!("slot `{slot}` not declared on `{path}`"))
        })?;
        drop(g);
        self.sink.emit(GraphEvent::SlotChanged {
            id,
            path: path.clone(),
            slot: slot.to_string(),
            value,
            generation: gen,
        });
        Ok(gen)
    }

    /// Transition a node's lifecycle. Returns the new state on success.
    pub fn transition(&self, path: &NodePath, to: Lifecycle) -> Result<Lifecycle, GraphError> {
        let mut g = self.write_inner();
        let id = *g
            .by_path
            .get(path)
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;
        let rec = g
            .by_id
            .get_mut(&id)
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;
        if !rec.lifecycle.can_transition_to(to) {
            return Err(GraphError::BadLink(format!(
                "illegal lifecycle transition: {:?} → {:?}",
                rec.lifecycle, to
            )));
        }
        let from = rec.lifecycle;
        rec.lifecycle = to;
        drop(g);
        self.sink.emit(GraphEvent::LifecycleTransition {
            id,
            path: path.clone(),
            from,
            to,
        });
        Ok(to)
    }

    /// Add a link between two slots. Both endpoints must resolve to
    /// declared slots on existing nodes.
    pub fn add_link(&self, source: SlotRef, target: SlotRef) -> Result<LinkId, GraphError> {
        let mut g = self.write_inner();
        let src = g
            .by_id
            .get(&source.node)
            .ok_or_else(|| GraphError::BadLink("source node not found".to_string()))?;
        if !src.slots.contains(&source.slot) {
            return Err(GraphError::BadLink(format!(
                "source slot `{}` not declared on node {}",
                source.slot, source.node
            )));
        }
        let tgt = g
            .by_id
            .get(&target.node)
            .ok_or_else(|| GraphError::BadLink("target node not found".to_string()))?;
        if !tgt.slots.contains(&target.slot) {
            return Err(GraphError::BadLink(format!(
                "target slot `{}` not declared on node {}",
                target.slot, target.node
            )));
        }
        let link = Link::new(source, target);
        let id = link.id;
        g.links.insert(id, link.clone());
        drop(g);
        self.sink.emit(GraphEvent::LinkAdded(link));
        Ok(id)
    }

    /// Snapshot a node by path.
    pub fn get(&self, path: &NodePath) -> Option<NodeSnapshot> {
        let g = self.read_inner();
        let id = *g.by_path.get(path)?;
        g.by_id.get(&id).map(NodeSnapshot::from_record)
    }

    /// Snapshot a node by id.
    pub fn get_by_id(&self, id: NodeId) -> Option<NodeSnapshot> {
        let g = self.read_inner();
        g.by_id.get(&id).map(NodeSnapshot::from_record)
    }

    pub fn len(&self) -> usize {
        self.read_inner().by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn require_kind(&self, id: &KindId) -> Result<KindManifest, GraphError> {
        self.kinds
            .get(id)
            .ok_or_else(|| GraphError::UnknownKind(id.clone()))
    }

    fn write_inner(&self) -> std::sync::RwLockWriteGuard<'_, StoreInner> {
        self.inner.write().expect("GraphStore lock poisoned")
    }

    fn read_inner(&self) -> std::sync::RwLockReadGuard<'_, StoreInner> {
        self.inner.read().expect("GraphStore lock poisoned")
    }
}

fn collect_subtree(by_id: &HashMap<NodeId, NodeRecord>, root: NodeId, out: &mut Vec<NodeId>) {
    // Post-order: children first, root last.
    if let Some(rec) = by_id.get(&root) {
        for c in rec.children.clone() {
            collect_subtree(by_id, c, out);
        }
    }
    out.push(root);
}
