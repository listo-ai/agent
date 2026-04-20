//! Error type for the domain-auth crate.

/// Errors produced by domain-auth operations.
#[derive(Debug, thiserror::Error)]
pub enum DomainAuthError {
    #[error("graph error: {0}")]
    Graph(#[from] graph::GraphError),
}
