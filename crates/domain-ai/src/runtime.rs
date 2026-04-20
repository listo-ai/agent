//! Process-wide AI runtime handle.
//!
//! The behavior for `sys.ai.run` can't receive an `Arc<Registry>` through
//! `NodeCtx` (no DI slot at dispatch time), so the agent bootstrap plants
//! the registry + defaults here once via [`init`]. Behaviors read it back
//! with [`get`]. A single process has one AI runtime — this is fine for
//! the POC; if we ever need per-tenant runtimes it moves onto an engine
//! context.

use std::sync::Arc;
use std::sync::OnceLock;

use ai_runner::{AiDefaults, Registry};

static RUNTIME: OnceLock<(Arc<Registry>, AiDefaults)> = OnceLock::new();

/// Install the shared registry + defaults. Idempotent: subsequent calls
/// are ignored with a trace log (the first installation wins).
pub fn init(registry: Arc<Registry>, defaults: AiDefaults) {
    if RUNTIME.set((registry, defaults)).is_err() {
        tracing::debug!("domain_ai::runtime::init called more than once — keeping first value");
    }
}

/// Borrow the installed registry + defaults, if any.
pub fn get() -> Option<(Arc<Registry>, AiDefaults)> {
    RUNTIME.get().cloned()
}
