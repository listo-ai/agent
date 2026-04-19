//! Graph events.
//!
//! Every mutation fires an event. In-process callers plug in any
//! `EventSink` — tests use [`VecSink`], production wires to
//! [`messaging::MessageBus`] via an adapter (lives in the agent).
//! The NATS subject mapping lands in Stage 6 (see RUNTIME.md).

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use spi::{KindId, NodeId, NodePath};

use crate::lifecycle::Lifecycle;
use crate::link::{Link, LinkId, SlotRef};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GraphEvent {
    NodeCreated {
        id: NodeId,
        kind: KindId,
        path: NodePath,
    },
    NodeRemoved {
        id: NodeId,
        kind: KindId,
        path: NodePath,
    },
    NodeRenamed {
        id: NodeId,
        old_path: NodePath,
        new_path: NodePath,
    },
    SlotChanged {
        id: NodeId,
        path: NodePath,
        slot: String,
        value: JsonValue,
        generation: u64,
    },
    LifecycleTransition {
        id: NodeId,
        path: NodePath,
        from: Lifecycle,
        to: Lifecycle,
    },
    LinkAdded(Link),
    LinkRemoved {
        id: LinkId,
        source: SlotRef,
        target: SlotRef,
    },
    LinkBroken {
        id: LinkId,
        /// Endpoint that was deleted.
        broken_end: SlotRef,
        /// Endpoint that survives and receives the notification.
        surviving_end: SlotRef,
    },
}

/// Callback sink for graph events. Kept synchronous so graph mutations
/// don't require an async runtime — the caller chooses how to forward.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: GraphEvent);
}

/// No-op sink. Useful for tests that don't care about events.
#[derive(Debug, Default)]
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: GraphEvent) {}
}

/// Test-friendly sink collecting everything into a vector.
#[derive(Debug, Default)]
pub struct VecSink {
    events: Mutex<Vec<GraphEvent>>,
}

impl VecSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take(&self) -> Vec<GraphEvent> {
        let mut guard = self.events.lock().expect("VecSink lock poisoned");
        std::mem::take(&mut *guard)
    }

    pub fn snapshot(&self) -> Vec<GraphEvent> {
        self.events.lock().expect("VecSink lock poisoned").clone()
    }
}

impl EventSink for VecSink {
    fn emit(&self, event: GraphEvent) {
        if let Ok(mut g) = self.events.lock() {
            g.push(event);
        }
    }
}
