//! `ProviderCell` — a reusable hot-swap cell for `Arc<dyn
//! AuthProvider>`.
//!
//! The setup flow needs to replace the live provider (empty-table
//! `StaticTokenProvider` → populated provider) without recreating
//! `AppState`. Future scopes (certificate rotation, Zitadel re-keying,
//! tenant-scoped provider swaps) need the same primitive. Rather than
//! leak `arc_swap::ArcSwap` semantics into every caller, we encapsulate
//! the cell here.
//!
//! # Why not `RwLock<Arc<dyn AuthProvider>>`?
//!
//! The resolver is on the authenticated request hot path. Every
//! incoming request hits `cell.load()`. `RwLock` serialises readers
//! against any in-flight writer; a single provider swap could
//! momentarily block every in-flight request. `ArcSwap` is lock-free
//! on the read side (a single atomic load), so swaps never stall the
//! request path.
//!
//! # Storage shape
//!
//! The payload is `Arc<Arc<dyn AuthProvider>>` — double-wrapped
//! because `arc_swap::ArcSwap<T>` implements `RefCnt` only for
//! `Arc<T: Sized>`, and `dyn AuthProvider` is unsized. Callers never
//! see the double Arc: [`load`] returns a plain `Arc<dyn AuthProvider>`.
//!
//! # Sharing
//!
//! `ProviderCell` wraps the swap cell in an outer `Arc` internally, so
//! `clone()` is cheap and all clones observe the same swaps. Store one
//! in `AppState`; every cloned `AppState` sees the live provider.

use std::sync::Arc;

use arc_swap::ArcSwap;
use spi::AuthProvider;

/// Hot-swap cell for the live identity provider.
#[derive(Clone)]
pub struct ProviderCell {
    inner: Arc<ArcSwap<Arc<dyn AuthProvider>>>,
}

impl ProviderCell {
    /// Construct with an initial provider. Typical call sites pass a
    /// dev-time placeholder (`DevNullProvider`) or the empty-table
    /// `StaticTokenProvider` used during first-boot setup.
    pub fn new(initial: Arc<dyn AuthProvider>) -> Self {
        Self {
            inner: Arc::new(ArcSwap::new(Arc::new(initial))),
        }
    }

    /// Read the currently-installed provider. Cheap — one atomic load
    /// plus one refcount bump. The returned `Arc` is a snapshot;
    /// subsequent [`store`](Self::store) calls do not affect it.
    pub fn load(&self) -> Arc<dyn AuthProvider> {
        // Inner cell stores `Arc<Arc<dyn AuthProvider>>`; we clone the
        // inner Arc so callers hold a plain `Arc<dyn AuthProvider>`
        // across `.await` points without leaking the double-Arc.
        let outer = self.inner.load_full();
        Arc::clone(outer.as_ref())
    }

    /// Hot-swap the provider atomically. No in-flight [`load`] sees a
    /// torn read — readers that started before the swap complete on
    /// the old provider; readers that start after see the new one.
    /// Both are valid; the handoff is not observable as a data race.
    pub fn store(&self, provider: Arc<dyn AuthProvider>) {
        self.inner.store(Arc::new(provider));
    }

    /// Provider id of the currently-installed provider. Convenience
    /// for logging and `whoami`-style endpoints.
    pub fn id(&self) -> &'static str {
        self.load().id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use spi::{AuthContext, AuthError, RequestHeaders};

    struct Named(&'static str);

    #[async_trait]
    impl AuthProvider for Named {
        async fn resolve(&self, _: &dyn RequestHeaders) -> Result<AuthContext, AuthError> {
            Ok(AuthContext::dev_null())
        }
        fn id(&self) -> &'static str {
            self.0
        }
    }

    #[tokio::test]
    async fn load_returns_initial_provider() {
        let cell = ProviderCell::new(Arc::new(Named("first")));
        assert_eq!(cell.load().id(), "first");
    }

    #[tokio::test]
    async fn store_replaces_provider_visible_to_subsequent_loads() {
        let cell = ProviderCell::new(Arc::new(Named("first")));
        cell.store(Arc::new(Named("second")));
        assert_eq!(cell.load().id(), "second");
    }

    #[tokio::test]
    async fn clone_shares_the_swap_cell() {
        // Critical: AppState is Clone, so every cloned state must see
        // swaps made through any other clone. Regressing this would
        // silently defeat hot-swap.
        let a = ProviderCell::new(Arc::new(Named("first")));
        let b = a.clone();
        b.store(Arc::new(Named("second")));
        assert_eq!(a.load().id(), "second");
    }
}
