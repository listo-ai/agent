//! Errors surfaced by this crate.
//!
//! All errors are init-time; once a subscriber is installed, the
//! logger itself never surfaces errors to callers (drop-on-full /
//! fall-back behaviour is the contract — see
//! `docs/design/LOGGING.md` § "Edge constraints").

use std::io;

/// Everything this crate can fail with.
#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    /// The subscriber could not be installed — typically because one
    /// is already installed in this process.
    #[error("logging subsystem init failed: {0}")]
    InitFailed(String),

    /// An `EnvFilter` directive failed to parse.
    #[error("invalid log filter: {0}")]
    BadFilter(String),

    /// File-sink I/O error.
    #[error("log sink I/O: {0}")]
    SinkIo(#[from] io::Error),
}
