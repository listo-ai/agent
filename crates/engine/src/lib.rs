#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Flow engine \u{2014} live-wire executor, state machine, safe-state
//! enforcement, and (Stage 2b) crossflow integration for flow-document
//! execution.
//!
//! See `docs/design/RUNTIME.md` for the runtime model and
//! `docs/sessions/STEPS.md` § "Stage 2" for the current scope.
//!
//! ## Wiring
//!
//! ```ignore
//! use std::sync::Arc;
//! use engine::{queue, Engine, kinds as engine_kinds};
//! use graph::{GraphStore, KindRegistry, seed};
//!
//! let (sink, events) = queue::channel();
//! let kinds = KindRegistry::new();
//! seed::register_builtins(&kinds);
//! engine_kinds::register(&kinds);
//! let graph = Arc::new(GraphStore::new(kinds, sink));
//! let engine = Engine::new(graph, events);
//! engine.start().await?;
//! // \u{2026} run \u{2026}
//! engine.shutdown().await?;
//! # Ok::<(), engine::EngineError>(())
//! ```

mod engine;
mod error;
pub mod kinds;
mod live_wire;
pub mod queue;
pub mod safe_state;
mod state;

pub use crate::engine::Engine;
pub use error::EngineError;
pub use safe_state::{
    NoopOutputDriver, OutputDriver, SafeStateBinding, SafeStateError, SafeStatePolicy,
};
pub use state::EngineState;
