//! Core author traits — `NodeKind` (declarative) and `NodeBehavior`
//! (imperative).
//!
//! In Stage 3a-1 only [`NodeKind`] is wired end-to-end. [`NodeBehavior`]
//! is declared so plugin crates can pin their impl signatures against
//! it; dispatch through the engine lands in Stage 3a-2 together with
//! the real [`NodeCtx`].

use crate::error::NodeError;
use crate::{KindId, KindManifest, Msg};

/// Declarative half of a kind — kind id and manifest.
///
/// Implemented by `#[derive(NodeKind)]`. Authors do not write this by
/// hand; the derive reads the YAML manifest at compile time and emits
/// the impl. See the crate-level docs for an example.
pub trait NodeKind {
    fn kind_id() -> KindId;
    fn manifest() -> KindManifest;
}

/// Port identifier — the named input that fired. Owned string so
/// authors can do free-form matching; cheaply comparable against the
/// manifest's slot names.
pub type InputPort = String;

/// Context handed to a [`NodeBehavior`] on every entry point.
///
/// Stage 3a-1 ships the type as an opaque marker so `NodeBehavior`
/// signatures are stable across the SDK right now. The real surface
/// (`emit`, `read_slot`, `update_status`, `schedule`, `resolve_settings`)
/// lands in Stage 3a-2 when the engine's behaviour dispatcher does.
#[derive(Debug)]
pub struct NodeCtx {
    // Fields land in 3a-2 together with the BehaviorRegistry.
    _private: (),
}

impl NodeCtx {
    /// Construct a placeholder context. Only useful for documentation
    /// examples in 3a-1; real construction is the dispatcher's job.
    #[doc(hidden)]
    pub fn __stub() -> Self {
        Self { _private: () }
    }
}

/// Imperative half of a kind — runtime behaviour on lifecycle events
/// and on each inbound message.
///
/// Manifest-only (container) kinds do **not** implement this trait.
/// They are declared via `#[node(..., behavior = "none")]` so the
/// distinction is explicit — omitting the attribute is a compile error.
///
/// Stage 3a-1 pins the trait shape; Stage 3a-2 wires the dispatcher.
pub trait NodeBehavior {
    type Config: serde::de::DeserializeOwned + Send + 'static;

    fn on_init(&mut self, _ctx: &NodeCtx, _cfg: &Self::Config) -> Result<(), NodeError> {
        Ok(())
    }

    fn on_message(&mut self, ctx: &NodeCtx, port: InputPort, msg: Msg) -> Result<(), NodeError>;

    fn on_config_change(&mut self, _ctx: &NodeCtx, _cfg: &Self::Config) -> Result<(), NodeError> {
        Ok(())
    }

    fn on_shutdown(&mut self, _ctx: &NodeCtx) -> Result<(), NodeError> {
        Ok(())
    }
}
