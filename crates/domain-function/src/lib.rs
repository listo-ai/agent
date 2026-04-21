#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! `domain-function` — Node-RED-style Function node backed by Rhai.
//!
//! Ships the core kind `sys.logic.function`. Scripts get a pre-bound
//! `msg` map (Node-RED parity: `msg.payload`, `msg.topic`, …), a small
//! surface of helpers (log/time/JSON/new_msg), and read-only
//! cross-node slot access via `get_slot(path, slot)`. Compiled ASTs
//! are cached per node, so a running flow only pays the compile cost
//! once per script edit. Error handling is policy-driven: drop, emit
//! on a dedicated `err` port, or bubble as a NodeError.
//!
//! Wiring lives in the composition root (`apps/agent`):
//!
//! ```ignore
//! use std::sync::Arc;
//! domain_function::register_kinds(graph.kinds());
//! engine.behaviors().register(
//!     <domain_function::Function as blocks_sdk::NodeKind>::kind_id(),
//!     domain_function::behavior(),
//! )?;
//! ```
//!
//! See `crates/domain-function/manifests/function.yaml` for the kind
//! contract and `src/function.rs` for the orchestration layer.

use std::sync::Arc;

use blocks_sdk::prelude::*;

mod cache;
mod convert;
mod error;
mod function;
mod helpers;

pub use function::{Function, FunctionConfig, OnError};

blocks_sdk::requires! {
    "spi.msg" => "1",
}

/// Register every kind manifest this crate contributes.
pub fn register_kinds(kinds: &graph::KindRegistry) {
    kinds.register(<Function as NodeKind>::manifest());
}

/// Construct the dispatchable behaviour for [`Function`].
pub fn behavior() -> Arc<dyn DynBehavior> {
    Arc::new(TypedBehavior(Function))
}
