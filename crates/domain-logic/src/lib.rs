#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! `domain-logic` — control-flow node kinds.
//!
//! Stage 3a-3 ships [`Trigger`], the `acme.logic.trigger` node — the
//! first behaviour kind to use the SDK's timer surface
//! ([`extensions_sdk::TimerScheduler`] + `NodeBehavior::on_timer`).

use std::sync::Arc;

use extensions_sdk::prelude::*;

pub mod heartbeat;
pub mod trigger;

pub use heartbeat::{Heartbeat, HeartbeatConfig};
pub use trigger::{Trigger, TriggerConfig, TriggerMode};

extensions_sdk::requires! {
    "spi.msg" => "1",
}

pub fn register_kinds(kinds: &graph::KindRegistry) {
    kinds.register(<Trigger as NodeKind>::manifest());
    kinds.register(<Heartbeat as NodeKind>::manifest());
}

/// Dispatch table for the `Trigger` kind.
pub fn behavior() -> Arc<dyn DynBehavior> {
    Arc::new(TypedBehavior(Trigger))
}

/// Dispatch table for the `Heartbeat` kind.
pub fn heartbeat_behavior() -> Arc<dyn DynBehavior> {
    Arc::new(TypedBehavior(Heartbeat))
}
