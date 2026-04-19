//! Macros and helpers callers should `use`.
//!
//! ```no_run
//! use observability::prelude::*;
//!
//! info!(msg_id = "abc", "queued retry");
//! warn!("backoff limit exceeded");
//! ```
//!
//! Re-exports the five level macros plus `span!` from [`tracing`]. All
//! events flow through the subscriber installed by
//! [`crate::init()`] which applies the JSON formatter, env filter, and
//! redaction layer.

pub use tracing::{debug, error, info, span, trace, warn};
