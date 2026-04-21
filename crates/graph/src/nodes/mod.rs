//! Nodes search scope — the graph's live node snapshots as a
//! searchable, paginated projection.
//!
//! Same ownership story as [`crate::kinds`]: DTO + RSQL schema + scope
//! all live in `graph`, so REST, fleet, CLI, and MCP share one
//! implementation. The transport handler is a dispatcher — nothing
//! more.

mod dto;
mod schema;
mod scope;

pub use dto::{NodeDto, SlotDto};
pub use schema::node_query_schema;
pub use scope::{NodesQuery, NodesScope};
