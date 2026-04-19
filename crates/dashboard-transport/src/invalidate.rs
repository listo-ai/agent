//! `ui.invalidate` events — signal mid-session state changes that
//! require the client to re-resolve the page.
//!
//! DASHBOARD.md § "Subscriptions ↔ resolve reconciliation" calls for:
//! * node *moved* (path changes) or *retyped* → `ui.invalidate`
//! * node *deleted* → `ui.invalidate`; next resolve returns
//!   `ui.widget.dangling`.
//!
//! The messaging crate is currently a stub; this module defines the
//! seam so the resolver and future graph-event bridges have a stable
//! target. When real NATS wiring lands, swap the default sink for one
//! that publishes on a page-scoped subject.

use serde::{Deserialize, Serialize};
use spi::NodeId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum InvalidateReason {
    /// A node referenced by a bound widget was deleted.
    NodeDeleted { node: NodeId },
    /// A node was moved in the graph (path changed).
    NodeMoved { node: NodeId },
    /// A node's kind changed — bindings may no longer resolve.
    NodeRetyped { node: NodeId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidateEvent {
    pub page_id: NodeId,
    #[serde(flatten)]
    pub reason: InvalidateReason,
}

pub trait InvalidateSink: Send + Sync {
    fn emit(&self, event: InvalidateEvent);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TracingInvalidate;

impl InvalidateSink for TracingInvalidate {
    fn emit(&self, event: InvalidateEvent) {
        tracing::info!(
            target: "dashboard.invalidate",
            page = %event.page_id,
            reason = ?event.reason,
            "ui.invalidate",
        );
    }
}

#[cfg(any(test, feature = "test-helpers"))]
#[derive(Debug, Default)]
pub struct RecordingInvalidate {
    events: std::sync::Mutex<Vec<InvalidateEvent>>,
}

#[cfg(any(test, feature = "test-helpers"))]
impl RecordingInvalidate {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn events(&self) -> Vec<InvalidateEvent> {
        self.events
            .lock()
            .expect("RecordingInvalidate poisoned")
            .clone()
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl InvalidateSink for RecordingInvalidate {
    fn emit(&self, event: InvalidateEvent) {
        self.events
            .lock()
            .expect("RecordingInvalidate poisoned")
            .push(event);
    }
}
