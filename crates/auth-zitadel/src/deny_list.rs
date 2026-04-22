//! Revocation — a post-verification gate that rejects tokens whose
//! subject has been revoked since issuance.
//!
//! JWTs are self-contained: once signed, they're valid until `exp`
//! regardless of what happens on the IdP side. A deny-list lets the
//! agent refuse a compromised user's token inside its 15-minute TTL
//! window without waiting for the token to expire naturally.
//!
//! # Sources
//!
//! - [`StaticDenyList`] — a `HashSet<String>` built from config. Good
//!   for small edges and tests. Compile-time equivalent of a
//!   hand-maintained allow/deny file.
//! - HTTP-polled deny list — lands with the cloud-side endpoint that
//!   publishes revoked subjects (tracked as B.3/B.4 in
//!   `docs/design/SYSTEM-BOOTSTRAP.md`). Shape already fits through
//!   this trait so nothing here changes when it arrives.
//!
//! # Matching
//!
//! The match is on the JWT `sub` claim — Zitadel's stable user id.
//! Matching on anything softer (email, name) invites drift: a user
//! renamed in the IdP should not un-revoke their old tokens. `sub`
//! stays stable across profile edits.
//!
//! # Performance
//!
//! `is_denied` sits on the authenticated request hot path. It must be
//! cheap — ideally an in-memory hash lookup. Implementations that
//! need I/O (HTTP poll, SQL) should cache and refresh on their own
//! schedule, never on the request path.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

/// Revocation predicate. Called after JWT signature + claims have
/// verified, before the [`spi::AuthContext`] is returned.
///
/// Implementations must be `Send + Sync + 'static` and suitable for
/// `Arc` sharing — one instance is consulted by every authenticated
/// request across the process.
#[async_trait]
pub trait DenyList: Send + Sync {
    /// `true` if `subject` (JWT `sub`) is revoked. Fast path: most
    /// subjects are not denied; return `false` without I/O.
    async fn is_denied(&self, subject: &str) -> bool;
}

/// In-memory deny list built from a fixed set of subjects. Zero
/// runtime I/O. Suitable for tests, small deployments that express
/// revocations in config, and as a fallback when a network-polled
/// source is unavailable.
///
/// The set is immutable by construction — swapping the deny-list
/// surface (e.g. after a cloud refresh) should go through
/// [`ProviderCell`]-style hot-swap at the provider level rather than
/// mutating this in place. That keeps every `is_denied` call
/// lock-free.
///
/// [`ProviderCell`]: crate::ProviderCell
pub struct StaticDenyList {
    denied: HashSet<String>,
}

impl StaticDenyList {
    pub fn new<I, S>(subjects: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            denied: subjects.into_iter().map(Into::into).collect(),
        }
    }

    /// Empty list — every request passes. Handy sentinel for boot
    /// paths that don't want to special-case "no deny list
    /// configured".
    pub fn empty() -> Self {
        Self {
            denied: HashSet::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.denied.len()
    }

    pub fn is_empty(&self) -> bool {
        self.denied.is_empty()
    }
}

#[async_trait]
impl DenyList for StaticDenyList {
    async fn is_denied(&self, subject: &str) -> bool {
        self.denied.contains(subject)
    }
}

/// Convenience: wrap a concrete [`DenyList`] as `Arc<dyn DenyList>`
/// without the caller having to spell out the cast.
#[must_use]
pub fn boxed<T: DenyList + 'static>(list: T) -> Arc<dyn DenyList> {
    Arc::new(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn static_list_matches_declared_subject() {
        let dl = StaticDenyList::new(["user-123", "bot-evil"]);
        assert!(dl.is_denied("user-123").await);
        assert!(dl.is_denied("bot-evil").await);
    }

    #[tokio::test]
    async fn static_list_passes_unknown_subject() {
        let dl = StaticDenyList::new(["user-123"]);
        assert!(!dl.is_denied("user-456").await);
    }

    #[tokio::test]
    async fn empty_list_never_denies() {
        let dl = StaticDenyList::empty();
        assert!(!dl.is_denied("anyone").await);
    }

    #[tokio::test]
    async fn is_case_sensitive() {
        // Deliberate — Zitadel `sub` values are case-sensitive
        // opaque ids. Soft-matching would let a capitalisation
        // differ from the denied form sneak through.
        let dl = StaticDenyList::new(["User-ABC"]);
        assert!(dl.is_denied("User-ABC").await);
        assert!(!dl.is_denied("user-abc").await);
    }
}
