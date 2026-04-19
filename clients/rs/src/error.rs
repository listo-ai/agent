//! Error types for the agent client.

use thiserror::Error;

/// Top-level error returned by every client method.
#[derive(Debug, Error)]
pub enum ClientError {
    /// HTTP-level error (non-2xx status).
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    /// The agent's capability manifest is missing a required capability.
    #[error("capability mismatch: {0}")]
    CapabilityMismatch(String),

    /// Network or transport error from reqwest.
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),

    /// Failed to parse the response body.
    #[error("parse: {0}")]
    Parse(String),
}

impl ClientError {
    /// True when the server returned 404.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Http { status: 404, .. })
    }
}
