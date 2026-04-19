//! Service Provider Interface — cross-cutting contracts.
//!
//! This crate is the contract surface shared by every other crate and by
//! third-party extensions. It contains:
//!
//! - `proto/extension.proto` — gRPC schema for extensions
//! - `schemas/flow.schema.json` — flow document format (`schema_version: 1`)
//! - `schemas/node.schema.json` — node manifest format
//! - [`Msg`] — Node-RED-compatible message envelope carried on wires
//!   between slots. See `docs/design/EVERYTHING-AS-NODE.md`.
//!
//! Rust-side re-exports of generated types land here in later stages.

pub mod capabilities;
mod msg;

pub use msg::{MessageId, Msg};

/// Schema version for flow documents. Breaking changes bump this.
pub const FLOW_SCHEMA_VERSION: u32 = 1;

/// Schema version for node manifests. Breaking changes bump this.
pub const NODE_SCHEMA_VERSION: u32 = 1;
