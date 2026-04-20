#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Tag validation and shorthand parsing for the `config.tags` slot.
//!
//! Two entry points:
//! - [`validate_tags`] — called by `GraphStore::write_slot` when the
//!   slot id is `config.tags`; accepts raw JSON from the wire.
//! - [`parse_shorthand`] — called by the CLI for `[labels]{kv}` notation.

mod error;
mod normalize;
mod shorthand;
mod tags;

pub use error::TagsError;
pub use shorthand::parse_shorthand;
pub use tags::{validate_tags, Tags};
