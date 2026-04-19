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

mod containment;
mod error;
mod event;
mod facets;
mod ids;
mod kind;
mod lifecycle;
mod link;
mod node;
pub mod seed;
mod slot;
mod store;

pub use containment::{Cardinality, CascadePolicy, ContainmentSchema, ParentMatcher};
pub use error::GraphError;
pub use event::{EventSink, GraphEvent, NullSink, VecSink};
pub use facets::{Facet, FacetSet};
pub use ids::{KindId, NodeId, NodePath};
pub use kind::{KindManifest, KindRegistry};
pub use lifecycle::Lifecycle;
pub use link::{Link, LinkId, SlotRef};
pub use node::NodeSnapshot;
pub use slot::{SlotRole, SlotSchema, SlotValue};
pub use store::GraphStore;
