//! In-memory graph store.
//!
//! The store owns the tree. Every mutation goes through one of a handful
//! of methods, each one of which runs the same placement/cardinality
//! validation against the kind registry, applies the change
//! transactionally, and emits the matching [`GraphEvent`] via the
//! configured [`EventSink`].
//!
//! Persistent backing via `data-repos` is wired through the optional
//! [`GraphRepo`] passed to [`GraphStore::with_repo`]. Mutations are
//! write-through: the DB write happens before the in-memory change, so a
//! backend failure leaves the store untouched. In-memory-only stores
//! (tests, ephemeral roles) skip the repo path entirely.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use data_repos::GraphRepo;
use serde_json::Value as JsonValue;
use spi::{Cardinality, CascadePolicy, KindId, KindManifest, NodeId, NodePath, SlotSchema};

use crate::error::GraphError;
use crate::event::{EventSink, GraphEvent};
use crate::kind::KindRegistry;
use crate::lifecycle::Lifecycle;
use crate::link::{Link, LinkId, SlotRef};
use crate::node::{NodeRecord, NodeSnapshot};
use crate::persist;

pub struct GraphStore {
    kinds: KindRegistry,
    pub(crate) sink: Arc<dyn EventSink>,
    pub(crate) repo: Option<Arc<dyn GraphRepo>>,
    pub(crate) inner: RwLock<StoreInner>,
}

pub(crate) struct StoreInner {
    pub(crate) by_id: HashMap<NodeId, NodeRecord>,
    pub(crate) by_path: HashMap<NodePath, NodeId>,
    pub(crate) links: HashMap<LinkId, Link>,
}

impl GraphStore {
    pub fn new(kinds: KindRegistry, sink: Arc<dyn EventSink>) -> Self {
        Self {
            kinds,
            sink,
            repo: None,
            inner: RwLock::new(StoreInner {
                by_id: HashMap::new(),
                by_path: HashMap::new(),
                links: HashMap::new(),
            }),
        }
    }

    /// Construct a store backed by a durable repo and restore any
    /// pre-existing graph state from it.
    ///
    /// Kinds must already be registered: the restore phase rejects
    /// nodes whose `kind_id` isn't present in the registry, so a DB
    /// with extension-contributed nodes is only safely restored after
    /// every block has registered its kinds.
    pub fn with_repo(
        kinds: KindRegistry,
        sink: Arc<dyn EventSink>,
        repo: Arc<dyn GraphRepo>,
    ) -> Result<Self, GraphError> {
        let store = Self {
            kinds,
            sink,
            repo: Some(repo.clone()),
            inner: RwLock::new(StoreInner {
                by_id: HashMap::new(),
                by_path: HashMap::new(),
                links: HashMap::new(),
            }),
        };
        store.restore(repo.as_ref())?;
        Ok(store)
    }

    fn restore(&self, repo: &dyn GraphRepo) -> Result<(), GraphError> {
        let snap = persist::load_snapshot(repo)?;
        let mut g = self.write_inner();
        for n in &snap.nodes {
            let kind_id = KindId::new(&n.kind_id);
            let manifest = self
                .kinds
                .get(&kind_id)
                .ok_or_else(|| GraphError::UnknownKind(kind_id.clone()))?;
            let id = NodeId(n.id);
            let path = persist::snapshot_to_path(&n.path);
            let parent = n.parent_id.map(NodeId);
            let mut rec = NodeRecord::new(id, kind_id, path.clone(), parent);
            rec.lifecycle = persist::lifecycle_from_str(&n.lifecycle)?;
            for slot in &manifest.slots {
                rec.slots.insert(slot.name.clone(), JsonValue::Null);
            }
            if let Some(pid) = parent {
                if let Some(p) = g.by_id.get_mut(&pid) {
                    p.children.push(id);
                }
            }
            g.by_id.insert(id, rec);
            g.by_path.insert(path, id);
        }
        for s in snap.slots {
            let id = NodeId(s.node_id);
            if let Some(rec) = g.by_id.get_mut(&id) {
                rec.slots.restore(s.name, s.value, s.generation as u64);
            }
        }
        for l in snap.links {
            let link = persist::snapshot_to_link(l);
            g.links.insert(link.id, link);
        }
        tracing::info!(
            nodes = g.by_id.len(),
            links = g.links.len(),
            "graph restored from repo",
        );
        Ok(())
    }

    pub(crate) fn repo_save_node(&self, rec: &NodeRecord) -> Result<(), GraphError> {
        if let Some(repo) = &self.repo {
            persist::repo_call(repo.save_node(&persist::node_to_persisted(rec)))?;
        }
        Ok(())
    }

    fn repo_delete_nodes(&self, ids: &[NodeId]) -> Result<(), GraphError> {
        if let Some(repo) = &self.repo {
            let raw: Vec<_> = ids.iter().map(|n| n.0).collect();
            persist::repo_call(repo.delete_nodes(&raw))?;
        }
        Ok(())
    }

    fn repo_save_slot(
        &self,
        node_id: NodeId,
        schema: &SlotSchema,
        value: &JsonValue,
        generation: u64,
    ) -> Result<(), GraphError> {
        if let Some(repo) = &self.repo {
            persist::repo_call(repo.upsert_slot(&persist::slot_to_persisted(
                node_id,
                &schema.name,
                schema.role,
                schema.value_kind,
                value,
                generation,
            )))?;
        }
        Ok(())
    }

    fn repo_save_link(&self, link: &Link) -> Result<(), GraphError> {
        if let Some(repo) = &self.repo {
            persist::repo_call(repo.save_link(&persist::link_to_persisted(link)))?;
        }
        Ok(())
    }

    fn repo_delete_links(&self, ids: &[LinkId]) -> Result<(), GraphError> {
        if let Some(repo) = &self.repo {
            let raw: Vec<_> = ids.iter().map(|l| l.0).collect();
            persist::repo_call(repo.delete_links(&raw))?;
        }
        Ok(())
    }

    pub fn kinds(&self) -> &KindRegistry {
        &self.kinds
    }

    /// Create the root node (`/`). Must be a kind whose containment is
    /// *free* — typically `sys.core.station`.
    pub fn create_root(&self, kind: KindId) -> Result<NodeId, GraphError> {
        let manifest = self.require_kind(&kind)?;
        let mut g = self.write_inner();
        if g.by_path.contains_key(&NodePath::root()) {
            return Err(GraphError::RootAlreadyExists);
        }
        let id = NodeId::new();
        let mut record = NodeRecord::new(id, kind.clone(), NodePath::root(), None);
        for slot in &manifest.slots {
            record.slots.insert(slot.name.clone(), JsonValue::Null);
        }
        // Repo first — failure here leaves both memory and DB clean.
        self.repo_save_node(&record)?;
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
            let ok = parent_manifest
                .containment
                .may_contain
                .iter()
                .any(|m| match m {
                    spi::ParentMatcher::Kind(k) => k == &kind,
                    spi::ParentMatcher::Facet(f) => manifest.facets.contains(*f),
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
        // Repo first — failure aborts before any memory mutation.
        self.repo_save_node(&record)?;
        g.by_id.insert(id, record);
        g.by_path.insert(path.clone(), id);
        if let Some(p) = g.by_id.get_mut(&parent_id) {
            p.children.push(id);
        }
        drop(g);

        self.sink.emit(GraphEvent::NodeCreated { id, kind, path });
        Ok(id)
    }

    /// Like [`create_child`] but automatically appends `-2`, `-3`, … to the
    /// requested name until a free slot is found (all within the same write
    /// lock, so there is no TOCTOU race).
    ///
    /// Returns the `NodeId` and the **actual** name that was used.
    pub fn create_child_unique(
        &self,
        parent: &NodePath,
        kind: KindId,
        name: &str,
    ) -> Result<(NodeId, String), GraphError> {
        const MAX_TRIES: u32 = 1000;

        // Strip an existing numeric suffix so repeated drops of the same
        // kind never pile up: "count" → base "count", "count-3" → base "count".
        let (base, start) = if let Some(pos) = name.rfind('-') {
            let suffix = &name[pos + 1..];
            if let Ok(n) = suffix.parse::<u32>() {
                (&name[..pos], n.max(2))
            } else {
                (name, 2u32)
            }
        } else {
            (name, 2u32)
        };

        // Try the original name first, then base-2, base-3, …
        let candidates = std::iter::once(name.to_string())
            .chain((start..=start + MAX_TRIES).map(|n| format!("{base}-{n}")));

        for candidate in candidates {
            match self.create_child(parent, kind.clone(), &candidate) {
                Ok(id) => return Ok((id, candidate)),
                Err(GraphError::NameCollision { .. }) => continue,
                Err(e) => return Err(e),
            }
        }

        Err(GraphError::NameCollision {
            parent: parent.clone(),
            name: name.to_string(),
        })
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
        let root_rec = g
            .by_id
            .get(&id)
            .cloned()
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;
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

        // Repo: remove links, then nodes. ON DELETE CASCADE handles slot
        // rows via the FK; calling delete_links here is explicit so the
        // repo error path matches the memory-side break list.
        let broken_ids: Vec<LinkId> = broken.iter().map(|(id, _, _)| *id).collect();
        self.repo_delete_links(&broken_ids)?;
        self.repo_delete_nodes(&subtree)?;

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

    /// Write a value to a slot, requiring the slot's current
    /// generation to match `expected`. Returns `GenerationMismatch` on
    /// conflict. Used by the builder's OCC invariant.
    pub fn write_slot_expected(
        &self,
        path: &NodePath,
        slot: &str,
        value: JsonValue,
        expected: u64,
    ) -> Result<u64, GraphError> {
        self.write_slot_inner(path, slot, value, Some(expected))
    }

    /// Write a value to a slot. The slot must exist on the node and be
    /// declared `writable` by the kind manifest.
    pub fn write_slot(
        &self,
        path: &NodePath,
        slot: &str,
        value: JsonValue,
    ) -> Result<u64, GraphError> {
        self.write_slot_inner(path, slot, value, None)
    }

    fn write_slot_inner(
        &self,
        path: &NodePath,
        slot: &str,
        value: JsonValue,
        expected: Option<u64>,
    ) -> Result<u64, GraphError> {
        let mut g = self.write_inner();
        let id = *g
            .by_path
            .get(path)
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;
        let kind = g
            .by_id
            .get(&id)
            .map(|r| r.kind.clone())
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;
        let manifest = self.require_kind(&kind)?;
        let schema = manifest
            .slots
            .iter()
            .find(|s| s.name == slot)
            .cloned()
            .ok_or_else(|| {
                GraphError::BadLink(format!("slot `{slot}` not declared on `{path}`"))
            })?;
        let rec = g
            .by_id
            .get_mut(&id)
            .ok_or_else(|| GraphError::NotFound(path.clone()))?;
        let current = rec.slots.current_generation(slot).ok_or_else(|| {
            GraphError::BadLink(format!("slot `{slot}` not declared on `{path}`"))
        })?;
        if let Some(expected) = expected {
            if expected != current {
                return Err(GraphError::GenerationMismatch { expected, current });
            }
        }
        let new_gen = current + 1;
        // Repo first — commit to memory only if the DB accepts.
        self.repo_save_slot(id, &schema, &value, new_gen)?;
        let gen = rec
            .slots
            .write(slot, value.clone())
            .expect("slot presence checked above");
        debug_assert_eq!(gen, new_gen);
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
        let snapshot = rec.clone();
        self.repo_save_node(&snapshot)?;
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
        self.repo_save_link(&link)?;
        g.links.insert(id, link.clone());
        drop(g);
        self.sink.emit(GraphEvent::LinkAdded(link));
        Ok(id)
    }

    /// Remove a link by id. Emits [`GraphEvent::LinkRemoved`]; no
    /// `LinkBroken` follows, since `LinkBroken` is reserved for the
    /// cascade-delete path where one endpoint ceased to exist.
    pub fn remove_link(&self, id: LinkId) -> Result<(), GraphError> {
        let mut g = self.write_inner();
        let link = g
            .links
            .remove(&id)
            .ok_or_else(|| GraphError::BadLink(format!("no link with id {}", id.0)))?;
        // Repo next — if the DB refuses, put it back so memory + DB stay aligned.
        if let Err(err) = self.repo_delete_links(&[id]) {
            g.links.insert(id, link);
            return Err(err);
        }
        let source = link.source.clone();
        let target = link.target.clone();
        drop(g);
        self.sink
            .emit(GraphEvent::LinkRemoved { id, source, target });
        Ok(())
    }

    /// All links whose source matches the given slot. Used by the engine's
    /// live-wire executor to propagate a `SlotChanged` event to every
    /// downstream target. Cheap enough today (linear over the link map);
    /// Stage 7 replaces the map with an indexed store when fleet scale
    /// arrives.
    pub fn links_from(&self, source: &SlotRef) -> Vec<Link> {
        let g = self.read_inner();
        g.links
            .values()
            .filter(|l| &l.source == source)
            .cloned()
            .collect()
    }

    /// Snapshot every link. Test / introspection helper; the engine uses
    /// [`Self::links_from`] for the hot path.
    pub fn links(&self) -> Vec<Link> {
        self.read_inner().links.values().cloned().collect()
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

    /// Snapshot every node. Linear over the tree. Used by the REST
    /// surface and debug tooling; production hot paths should go
    /// through a subscription instead.
    pub fn snapshots(&self) -> Vec<NodeSnapshot> {
        self.read_inner()
            .by_id
            .values()
            .map(NodeSnapshot::from_record)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn require_kind(&self, id: &KindId) -> Result<KindManifest, GraphError> {
        self.kinds
            .get(id)
            .ok_or_else(|| GraphError::UnknownKind(id.clone()))
    }

    pub(crate) fn write_inner(&self) -> std::sync::RwLockWriteGuard<'_, StoreInner> {
        self.inner.write().expect("GraphStore lock poisoned")
    }

    fn read_inner(&self) -> std::sync::RwLockReadGuard<'_, StoreInner> {
        self.inner.read().expect("GraphStore lock poisoned")
    }
}

pub(crate) fn collect_subtree(
    by_id: &HashMap<NodeId, NodeRecord>,
    root: NodeId,
    out: &mut Vec<NodeId>,
) {
    // Post-order: children first, root last.
    if let Some(rec) = by_id.get(&root) {
        for c in rec.children.clone() {
            collect_subtree(by_id, c, out);
        }
    }
    out.push(root);
}
