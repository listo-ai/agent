//! Author-facing prelude.
//!
//! `use extensions_sdk::prelude::*;` brings every name a plugin author
//! typically needs into scope. Kept narrow — contract surface only; no
//! runtime types.

pub use crate::error::NodeError;
pub use crate::node::{InputPort, NodeBehavior, NodeCtx, NodeKind};
pub use crate::{
    Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest,
    MessageId, Msg, NodeId, NodePath, ParentMatcher, SlotRole, SlotSchema,
};
// The derive and the trait share a name — different namespaces, so
// `use extensions_sdk::prelude::*;` gives you both.
pub use extensions_sdk_macros::NodeKind;
