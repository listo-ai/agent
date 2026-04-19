//! Span helpers over [`macro@tracing::span`].
//!
//! The subscriber installed by [`fn@crate::init`] attaches `trace_id`
//! and `span_id` automatically to every event emitted inside an
//! active span — this module is mostly a re-export with ergonomic
//! wrappers and documentation that ties the tracing API to the
//! canonical correlation-field contract.
//!
//! ```no_run
//! use observability::prelude::*;
//! use observability::span::Span;
//!
//! let span = Span::info("flow.run");
//! let _guard = span.entered();
//! info!(flow_id = "f-42", "starting");
//! ```
//!
//! For call sites that need custom fields on the span itself, use
//! [`tracing::span!`] directly:
//!
//! ```no_run
//! use observability::prelude::*;
//!
//! let _g = span!(tracing::Level::INFO, "flow.run", flow_id = "f-42").entered();
//! ```

use tracing::Level;

/// Lightweight newtype over [`tracing::Span`]. The wrapper exists so
/// the call-site idiom is consistent across the codebase; all the
/// correlation-field plumbing lives in the subscriber.
#[derive(Debug)]
pub struct Span {
    inner: tracing::Span,
}

impl Span {
    /// Create a span at `INFO` with a static name.
    pub fn info(name: &'static str) -> Self {
        Self::at(Level::INFO, name)
    }

    /// Create a span at `DEBUG` with a static name.
    pub fn debug(name: &'static str) -> Self {
        Self::at(Level::DEBUG, name)
    }

    /// Create a span at an explicit level.
    pub fn at(level: Level, name: &'static str) -> Self {
        let inner = match level {
            Level::ERROR => tracing::error_span!(target: "observability::span", "span", name),
            Level::WARN => tracing::warn_span!(target: "observability::span", "span", name),
            Level::INFO => tracing::info_span!(target: "observability::span", "span", name),
            Level::DEBUG => tracing::debug_span!(target: "observability::span", "span", name),
            Level::TRACE => tracing::trace_span!(target: "observability::span", "span", name),
        };
        Self { inner }
    }

    /// Enter the span. Drop the returned guard to exit.
    #[must_use = "the guard must be held for the span's lifetime"]
    pub fn entered(self) -> tracing::span::EnteredSpan {
        self.inner.entered()
    }

    /// Access the underlying [`tracing::Span`] for interop with other
    /// `tracing` APIs.
    pub fn into_inner(self) -> tracing::Span {
        self.inner
    }
}
