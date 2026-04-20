#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! The graph service — THE CORE of the platform.
//!
//! See `docs/design/EVERYTHING-AS-NODE.md` for the model. This crate
//! owns the substrate: the node tree, placement enforcement, lifecycle,
//! events, slots, links, and cascading delete. Every domain crate is in
//! effect a kind-registration plus rules; persistence goes through
//! `data-repos`; messaging goes through the [`EventSink`] trait here,
//! which the agent wires to the real `MessageBus` in its composition
//! root.
//!
//! The crate is synchronous on purpose. Pushing events over the wire is
//! the caller's problem — graph mutations don't own an async runtime.
//!
//! Declarative types (`KindManifest`, `KindId`, `NodeId`, `NodePath`,
//! `ContainmentSchema`, `Facet`/`FacetSet`, `SlotSchema`, `SlotRole`)
//! live in the [`spi`] crate — block authors reach them through the
//! SDK prelude without pulling in the graph runtime.

mod error;
mod event;
mod kind;
mod lifecycle;
mod link;
mod node;
mod patch;
mod persist;
pub mod seed;
mod slot;
mod store;

pub use error::GraphError;
pub use event::{EventSink, GraphEvent, NullSink, VecSink};
pub use kind::KindRegistry;
pub use lifecycle::Lifecycle;
pub use link::{Link, LinkId, SlotRef};
pub use node::NodeSnapshot;
pub use patch::NodePatch;
pub use slot::{SlotMap, SlotValue};
pub use store::GraphStore;
