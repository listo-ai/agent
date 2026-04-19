#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! `domain-compute` — first concrete behaviour kind.
//!
//! Houses [`Count`], the `sys.compute.count` node. The kind is the
//! end-to-end exercise for Stage 3a-2: a real `NodeBehavior` impl
//! dispatched through `engine::BehaviorRegistry` against the SDK's
//! `NodeCtx` surface.
//!
//! Wiring lives in the composition root (`apps/agent`):
//!
//! ```ignore
//! use std::sync::Arc;
//! domain_compute::register_kinds(graph.kinds());
//! engine.behaviors().register(
//!     domain_compute::Count::kind_id(),
//!     domain_compute::behavior(),
//! )?;
//! ```
//!
//! See `docs/sessions/NODE-SCOPE.md` § "Example — `sys.compute.count`".

use std::sync::Arc;

use extensions_sdk::prelude::*;

pub mod count;

pub use count::{Count, CountConfig};

extensions_sdk::requires! {
    "spi.msg" => "1",
}

/// Register every kind manifest this crate contributes.
pub fn register_kinds(kinds: &graph::KindRegistry) {
    kinds.register(<Count as NodeKind>::manifest());
}

/// Construct the dispatchable behaviour for [`Count`]. Cheap; behaviour
/// structs hold no state.
pub fn behavior() -> Arc<dyn DynBehavior> {
    Arc::new(TypedBehavior(Count))
}
