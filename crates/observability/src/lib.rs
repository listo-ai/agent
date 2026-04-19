#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Structured logging, tracing, and span correlation.
//!
//! Thin wrapper over [`tracing`] + [`tracing_subscriber`] that enforces
//! the canonical log-field contract from `docs/design/LOGGING.md`. This
//! crate owns **transport** for log events; field names come from
//! [`spi::log`] via the re-export in [`fields`].
//!
//! Dep arrow: `observability → spi + tracing + serde + thiserror`. It
//! must never depend on `graph`, `engine`, or any transport crate —
//! those pull `observability`, not the other way round.
//!
//! # Quick start
//!
//! ```no_run
//! use observability::{init, InitRole, prelude::*};
//!
//! // Once, at process start.
//! init(InitRole::Cli, "info").expect("logger init");
//!
//! info!(msg_id = "abc", "queued retry");
//! ```
//!
//! # What this pass ships
//!
//! - [`init()`] — one-call subscriber setup with JSON formatter, env
//!   filter, and redaction layer.
//! - [`prelude`] — the macros callers should `use`.
//! - [`fields`] — canonical field-name constants re-exported from
//!   [`spi::log`].
//! - [`redact`] — automatic and extensible secret scrubbing.
//! - [`span`] — span helpers with mandatory correlation fields.
//! - [`error::ObservabilityError`] — init-time errors.
//!
//! # What this pass deliberately omits
//!
//! - NATS shipper (needs messaging wiring — later stage).
//! - OTLP export (off by default per LOGGING.md; later stage).
//! - Runtime filter reload via API (needs transport-rest — later).
//! - Plugin host-function bindings (Stage 3).

pub mod error;
pub mod fields;
pub mod init;
pub mod prelude;
pub mod redact;
pub mod span;

pub use error::ObservabilityError;
pub use init::{init, InitRole, Role};
pub use redact::{register_extra, RedactLayer};
pub use span::Span;
