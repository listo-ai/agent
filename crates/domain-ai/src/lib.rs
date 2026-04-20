#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! `domain-ai` — AI node kinds backed by the shared `ai-runner` registry.
//!
//! Currently ships one kind: [`AiRun`] / `sys.ai.run` — a single-shot
//! prompt-in, text-out compute node that wraps `ai_runner::Runner::run`.
//!
//! Wiring: the agent bootstrap calls [`runtime::init`] once with the
//! process-wide `Arc<Registry>` + `AiDefaults`, then [`register_kinds`]
//! and [`behavior`] are used the same way every other domain crate
//! plugs into the engine.

use std::sync::Arc;

use blocks_sdk::prelude::*;

pub mod ai_run;
pub mod runtime;

pub use ai_run::{AiRun, AiRunConfig};

blocks_sdk::requires! {
    "spi.msg" => "1",
}

pub fn register_kinds(kinds: &graph::KindRegistry) {
    kinds.register(<AiRun as NodeKind>::manifest());
}

pub fn behavior() -> Arc<dyn DynBehavior> {
    Arc::new(TypedBehavior(AiRun))
}
