//! Kind palette: the searchable projection over [`crate::KindRegistry`].
//!
//! Shape and query surface that used to live inside `transport-rest` —
//! moved here so every transport (REST, CLI, MCP, fleet) hits the same
//! DTO, the same RSQL schema, and the same placement filter. The
//! transport crate now only wires the route.

mod dto;
mod schema;
mod scope;

pub use dto::{derive_org, KindDto};
pub use schema::kinds_query_schema;
pub use scope::{KindsQuery, KindsScope};
