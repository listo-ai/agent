//! `BehaviorRegistry` — kind→behaviour dispatch table.
//!
//! Stage 3a-2 wires the imperative side of the SDK to the running
//! engine. The registry is a pure dispatch table: it holds one
//! [`Arc<dyn DynBehavior>`] per [`KindId`] plus the kind's manifest
//! (cached for slot-role lookups on the hot path). **Per-node config is
//! not held here** — the `settings` slot on the node itself is the
//! source of truth, injected automatically by
//! [`graph::KindRegistry::register`] for every kind whose manifest
//! declares a `settings_schema`. See
//! [`docs/design/EVERYTHING-AS-NODE.md`] § "No parallel state" for why.
//!
//! On every `GraphEvent::SlotChanged` the dispatcher looks up the
//! target node's kind, checks the schema for that slot, and only fires
//! `on_message` when the slot is `role: input` AND `trigger: true`.
//! Status / config writes therefore *do not* re-enter the behaviour —
//! that's the contract the slot-source regression test pins down.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use blocks_sdk::{DynBehavior, EmitSink, GraphAccess, NodeCtx, TimerHandle, TimerScheduler};
use graph::{GraphEvent, GraphStore};
use serde_json::Value as JsonValue;
use spi::{KindId, KindManifest, Msg, NodeId, NodePath, SlotRole};
use tokio::sync::mpsc;

use crate::error::EngineError;
use crate::scheduler::{Scheduler, TimerFired};

#[derive(Clone)]
struct BehaviorEntry {
    behavior: Arc<dyn DynBehavior>,
    manifest: Arc<KindManifest>,
}

#[derive(Default)]
struct Inner {
    behaviors: HashMap<KindId, BehaviorEntry>,
}

/// Canonical name of the auto-injected config slot that holds a
/// behaviour's settings blob. See
/// [`graph::KindRegistry::register`] for the injection site.
pub const SETTINGS_SLOT: &str = "settings";

/// Behaviour dispatch table. Cheap to clone (an `Arc`).
#[derive(Clone)]
pub struct BehaviorRegistry {
    inner: Arc<RwLock<Inner>>,
    graph: Arc<GraphStore>,
    graph_access: Arc<dyn GraphAccess>,
    emit_sink: Arc<dyn EmitSink>,
    scheduler: Arc<Scheduler>,
}

impl BehaviorRegistry {
    pub fn new(graph: Arc<GraphStore>) -> (Self, mpsc::UnboundedReceiver<TimerFired>) {
        let adapter = Arc::new(GraphAdapter {
            graph: graph.clone(),
        });
        let (scheduler, rx) = Scheduler::new();
        let reg = Self {
            inner: Arc::new(RwLock::new(Inner::default())),
            graph,
            graph_access: adapter.clone(),
            emit_sink: adapter,
            scheduler,
        };
        (reg, rx)
    }

    /// Register a behaviour for a kind. The manifest must already be
    /// registered with the graph's [`KindRegistry`] — it's looked up
    /// here so dispatch doesn't pay for it on every message.
    pub fn register(
        &self,
        kind: KindId,
        behavior: Arc<dyn DynBehavior>,
    ) -> Result<(), EngineError> {
        let manifest = self
            .graph
            .kinds()
            .get(&kind)
            .ok_or_else(|| EngineError::UnknownKind(kind.clone()))?;
        let mut g = self.write_inner();
        g.behaviors.insert(
            kind,
            BehaviorEntry {
                behavior,
                manifest: Arc::new(manifest),
            },
        );
        Ok(())
    }

    /// Replace a node's settings blob. Writes to the node's synthesised
    /// `settings` config slot via [`GraphStore::write_slot`], so the
    /// change persists, fires `SlotChanged`, and is visible to every
    /// subscriber — no parallel state.
    ///
    /// Does **not** re-run `on_init` on its own; callers that want that
    /// semantics (e.g. the REST `POST /api/v1/config` handler) invoke
    /// [`Self::dispatch_init`] explicitly. Direct slot writes via
    /// [`GraphStore::write_slot`] to `"settings"` are equally valid —
    /// the behaviour picks up the new value on its next dispatch
    /// because [`Self::config_for`] reads the slot every call.
    pub fn set_config(&self, node: NodeId, config: JsonValue) -> Result<(), EngineError> {
        let snap = self
            .graph
            .get_by_id(node)
            .ok_or(EngineError::UnknownNode(node))?;
        self.graph
            .write_slot(&snap.path, SETTINGS_SLOT, config)
            .map(|_| ())
            .map_err(|e| EngineError::Behavior(format!("write settings slot: {e}")))
    }

    /// Fire `on_timer` for a node. Called by the engine worker loop
    /// when a [`TimerFired`] event arrives on the scheduler channel.
    pub fn dispatch_timer(&self, node: NodeId, handle: TimerHandle) -> Result<(), EngineError> {
        self.scheduler.mark_fired(handle);
        let snap = self
            .graph
            .get_by_id(node)
            .ok_or(EngineError::UnknownNode(node))?;
        let entry = self
            .lookup(&snap.kind)
            .ok_or_else(|| EngineError::UnknownKind(snap.kind.clone()))?;
        let cfg = self.config_for(node);
        let ctx = self.build_ctx(node, snap.path, snap.kind, entry.manifest.clone(), cfg);
        entry
            .behavior
            .on_timer(&ctx, handle)
            .map_err(|e| EngineError::Behavior(e.to_string()))
    }

    /// Run the behaviour's `on_init` for the named node. Caller looks
    /// up `NodeId` and `path` from the graph store.
    pub fn dispatch_init(&self, node: NodeId) -> Result<(), EngineError> {
        let snap = self
            .graph
            .get_by_id(node)
            .ok_or(EngineError::UnknownNode(node))?;
        let entry = self
            .lookup(&snap.kind)
            .ok_or_else(|| EngineError::UnknownKind(snap.kind.clone()))?;
        let cfg = self.config_for(node);
        let ctx = self.build_ctx(
            node,
            snap.path,
            snap.kind,
            entry.manifest.clone(),
            cfg.clone(),
        );
        entry
            .behavior
            .on_init(&ctx, &cfg)
            .map_err(|e| EngineError::Behavior(e.to_string()))
    }

    /// Process a graph event. Two triggers drive dispatch:
    ///
    ///   1. `NodeCreated` for any kind with a registered behaviour —
    ///      auto-runs `on_init` so source nodes (heartbeat, timer
    ///      generators, …) start producing without requiring the
    ///      caller to poke settings first. Fires exactly once per
    ///      create event; idempotent for sink-only kinds.
    ///   2. `SlotChanged` for trigger inputs or the synthesised
    ///      `settings` slot — per [`EVERYTHING-AS-NODE.md`] and
    ///      [`NODE-SCOPE.md`].
    ///
    /// Everything else is a live-wire / lifecycle concern handled elsewhere.
    pub fn handle(&self, event: &GraphEvent) {
        match event {
            GraphEvent::NodeCreated { id, kind, .. } => {
                // Only init when this crate has a behaviour for the kind —
                // pure data kinds (folders, stations, user-contributed
                // manifest-only kinds) are no-ops.
                if self.lookup(kind).is_none() {
                    return;
                }
                if let Err(err) = self.dispatch_init(*id) {
                    tracing::warn!(
                        node = %id, kind = %kind, error = %err,
                        "on_init on NodeCreated failed",
                    );
                }
            }
            GraphEvent::SlotChanged {
                id,
                path,
                slot,
                value,
                ..
            } => {
                if let Err(err) = self.try_dispatch(*id, path, slot, value) {
                    tracing::warn!(node = %id, slot, error = %err, "behaviour dispatch error");
                }
            }
            _ => {}
        }
    }

    fn try_dispatch(
        &self,
        node: NodeId,
        path: &NodePath,
        slot: &str,
        value: &JsonValue,
    ) -> Result<(), EngineError> {
        let kind = match self.graph.get_by_id(node) {
            Some(s) => s.kind,
            None => return Ok(()),
        };
        let Some(entry) = self.lookup(&kind) else {
            return Ok(());
        };
        let Some(schema) = entry.manifest.slots.iter().find(|s| s.name == slot) else {
            return Ok(());
        };

        // The synthesised `settings` config slot is the canonical
        // mechanism for reconfiguring a behaviour at runtime. Any
        // writer (REST `POST /api/v1/config`, CLI `slots write`, the
        // Studio property panel writing the slot directly) gets the
        // same semantics: changing settings triggers `on_init`.
        // Settings-as-slot is the source of truth per
        // `docs/design/EVERYTHING-AS-NODE.md` — the dispatcher honours
        // the slot, not the caller.
        if schema.role == SlotRole::Config && slot == SETTINGS_SLOT {
            return self.dispatch_init(node);
        }

        if schema.role != SlotRole::Input || !schema.trigger {
            return Ok(());
        }
        let msg = decode_msg(value);
        let cfg = self.config_for(node);
        let ctx = self.build_ctx(node, path.clone(), kind, entry.manifest.clone(), cfg);
        entry
            .behavior
            .on_message(&ctx, slot.to_string(), msg)
            .map_err(|e| EngineError::Behavior(e.to_string()))
    }

    fn build_ctx(
        &self,
        node: NodeId,
        path: NodePath,
        kind: KindId,
        manifest: Arc<KindManifest>,
        config: JsonValue,
    ) -> NodeCtx {
        NodeCtx::new(
            node,
            path,
            kind,
            manifest,
            config,
            self.graph_access.clone(),
            self.emit_sink.clone(),
            self.scheduler.clone() as Arc<dyn TimerScheduler>,
        )
    }

    fn lookup(&self, kind: &KindId) -> Option<BehaviorEntry> {
        self.inner.read().ok()?.behaviors.get(kind).cloned()
    }

    fn config_for(&self, node: NodeId) -> JsonValue {
        let Some(snap) = self.graph.get_by_id(node) else {
            return JsonValue::Null;
        };
        snap.slot_values
            .into_iter()
            .find(|(n, _)| n == SETTINGS_SLOT)
            .map(|(_, sv)| sv.value)
            .unwrap_or(JsonValue::Null)
    }

    fn write_inner(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.inner.write().expect("BehaviorRegistry lock poisoned")
    }
}

fn decode_msg(value: &JsonValue) -> Msg {
    // Wires carry typed `Msg`. Tolerate raw payloads (the live-wire
    // executor passes whatever was written) by promoting them.
    serde_json::from_value::<Msg>(value.clone()).unwrap_or_else(|_| Msg::new(value.clone()))
}

/// Adapter that lets `GraphStore` satisfy both `GraphAccess` and
/// `EmitSink` for the SDK. Emit becomes "write the message JSON to the
/// source's output slot" — the live-wire executor then fans it out to
/// every linked input.
struct GraphAdapter {
    graph: Arc<GraphStore>,
}

impl GraphAccess for GraphAdapter {
    fn read_slot(
        &self,
        path: &NodePath,
        slot: &str,
    ) -> Result<JsonValue, blocks_sdk::NodeError> {
        let snap = self.graph.get(path).ok_or_else(|| {
            blocks_sdk::NodeError::runtime(format!("node `{path}` not found"))
        })?;
        snap.slot_values
            .into_iter()
            .find(|(n, _)| n == slot)
            .map(|(_, sv)| sv.value)
            .ok_or_else(|| blocks_sdk::NodeError::UnknownSlot(slot.to_string()))
    }

    fn write_slot(
        &self,
        path: &NodePath,
        slot: &str,
        value: JsonValue,
    ) -> Result<(), blocks_sdk::NodeError> {
        self.graph
            .write_slot(path, slot, value)
            .map(|_| ())
            .map_err(|e| blocks_sdk::NodeError::runtime(e.to_string()))
    }
}

impl EmitSink for GraphAdapter {
    fn emit(&self, source: NodeId, port: &str, msg: Msg) -> Result<(), blocks_sdk::NodeError> {
        let path = self
            .graph
            .get_by_id(source)
            .map(|s| s.path)
            .ok_or_else(|| {
                blocks_sdk::NodeError::runtime(format!("emit: source node {source} missing"))
            })?;
        let value = serde_json::to_value(&msg)
            .map_err(|e| blocks_sdk::NodeError::runtime(e.to_string()))?;
        self.graph
            .write_slot(&path, port, value)
            .map(|_| ())
            .map_err(|e| blocks_sdk::NodeError::runtime(e.to_string()))
    }
}
