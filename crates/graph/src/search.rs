//! Cross-scope search primitive.
//!
//! The "find things" endpoint every transport shares. One trait
//! ([`SearchScope`]), one error type ([`ScopeError`]), one response
//! shape ([`ScopeHits`]). A scope is anything that can answer "here are
//! the rows matching this query" — kinds today; nodes, flows, audit
//! later. Transports don't implement scopes: they pick the scope by its
//! string id and forward the validated query.
//!
//! Why it lives in `graph`: the first scope ([`crate::kinds::KindsScope`])
//! queries the kind registry, which lives here. New scopes backed by
//! graph state (nodes, slots, links) will land alongside. Scopes that
//! read from repos or a TSDB should implement their own trait in their
//! own crate — this trait is deliberately small and sync.

use serde::Serialize;

use crate::error::GraphError;

/// One scope's slice of a search result.
#[derive(Debug, Clone, Serialize)]
pub struct ScopeHits<T> {
    /// Matched rows, ordered per the scope's semantics (RSQL sort or
    /// scope default).
    pub data: Vec<T>,
    /// Total matches before pagination. Equals `data.len()` when the
    /// scope returned every hit in one page.
    pub total: usize,
}

impl<T> ScopeHits<T> {
    pub fn new(data: Vec<T>, total: usize) -> Self {
        Self { data, total }
    }
}

/// Typed error returned by a scope. Transports map these to their own
/// status codes.
#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("graph error: {0}")]
    Graph(#[from] GraphError),
}

impl ScopeError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
}

/// Contract every searchable scope satisfies.
///
/// `Query` is the scope-specific parsed request (concrete-param
/// shortcuts + RSQL). `Hit` is the row type the scope emits —
/// serialisable so transports can render it without knowing the
/// concrete type.
///
/// Kept sync because every current scope reads from in-memory state.
/// When a DB-backed scope arrives it can either `spawn_blocking` or we
/// introduce `AsyncSearchScope` alongside — not retrofit this trait.
pub trait SearchScope {
    type Query;
    type Hit: Serialize;

    /// Stable scope id (`"kinds"`, `"nodes"`, …). Transports route by
    /// this string.
    fn id(&self) -> &'static str;

    /// Execute the query and return the hits.
    fn query(&self, q: Self::Query) -> Result<ScopeHits<Self::Hit>, ScopeError>;
}
