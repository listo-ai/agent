#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Service Provider Interface — cross-cutting contracts.
//!
//! This crate is the contract surface shared by every other crate and by
//! third-party extensions. It contains:
//!
//! - `proto/extension.proto` — gRPC schema for extensions
//! - `schemas/flow.schema.json` — flow document format (`schema_version: 1`)
//! - `schemas/node.schema.json` — node manifest format
//! - [`Msg`] — Node-RED-compatible message envelope carried on wires
//! - Identifier, facet, containment, slot-schema, and manifest types —
//!   the author-facing names referenced through the SDK prelude. Kept
//!   here (not in `graph`) so the SDK dep arrow is `extensions-sdk →
//!   spi`, never into the runtime. See NODE-SCOPE rule #1.
//!
//! Rust-side re-exports of generated types land here in later stages.

pub mod auth;
pub mod capabilities;
mod containment;
mod facets;
pub mod fleet;
mod ids;
pub mod log;
mod manifest;
mod msg;
mod slot_schema;
pub mod subject;

pub use auth::{
    Actor, AuthContext, AuthError, AuthProvider, NoHeaders, RequestHeaders, Scope, ScopeSet,
    TenantId,
};
pub use containment::{Cardinality, CascadePolicy, ContainmentSchema, ParentMatcher};
pub use facets::{Facet, FacetSet};
pub use fleet::{
    FleetError, FleetHandler, FleetMessage, FleetScope, FleetTransport, HealthStatus, HealthStream,
    NullTransport, Payload, Server, ServerHandle, SubscriptionStream,
};
pub use ids::{KindId, NodeId, NodePath};
pub use manifest::{KindManifest, TriggerPolicy};
pub use msg::{MessageId, Msg};
pub use slot_schema::{SlotRole, SlotSchema, SlotValueKind};
pub use subject::{Subject, SubjectBuilder, SubjectToken};

/// Schema version for flow documents. Breaking changes bump this.
pub const FLOW_SCHEMA_VERSION: u32 = 1;

/// Schema version for node manifests. Breaking changes bump this.
pub const NODE_SCHEMA_VERSION: u32 = 1;
