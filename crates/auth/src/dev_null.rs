//! Dev-null provider — stamps every request as admin on the default
//! tenant. Useful for local development, CI fixtures, and standalone
//! agents. MUST NOT be the active provider in a cloud/release build —
//! the agent startup check refuses to boot in that combination.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use spi::{AuthContext, AuthError, AuthProvider, RequestHeaders};

/// Stamp every request as `(DevNull, default, Admin)`.
///
/// Logs a loud warning at construction and every 15 minutes thereafter
/// so an operator accidentally running dev-null in a non-dev deployment
/// sees it in their log stream.
pub struct DevNullProvider {
    // Store as nanoseconds-since-`start` so we can CAS it atomically
    // without a Mutex around the instant. `0` means "never logged".
    start: Instant,
    last_nag_ns: AtomicU64,
    nag_interval: Duration,
}

impl DevNullProvider {
    pub fn new() -> Self {
        let p = Self {
            start: Instant::now(),
            last_nag_ns: AtomicU64::new(0),
            nag_interval: Duration::from_secs(15 * 60),
        };
        tracing::warn!(
            provider = "dev-null",
            "auth is dev-null — all requests pass as tenant=default, actor=local"
        );
        p
    }

    fn maybe_nag(&self) {
        let now_ns = self.start.elapsed().as_nanos() as u64;
        let last = self.last_nag_ns.load(Ordering::Relaxed);
        if last != 0 && now_ns.saturating_sub(last) < self.nag_interval.as_nanos() as u64 {
            return;
        }
        // Best-effort CAS: if two threads race, only one logs.
        if self
            .last_nag_ns
            .compare_exchange(last, now_ns, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
            && last != 0
        {
            tracing::warn!(
                provider = "dev-null",
                "auth is still dev-null — do not run this build against production traffic"
            );
        } else if last == 0 {
            // First request; just record without re-logging (we already
            // logged at construction).
            let _ =
                self.last_nag_ns
                    .compare_exchange(0, now_ns, Ordering::Relaxed, Ordering::Relaxed);
        }
    }
}

impl Default for DevNullProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthProvider for DevNullProvider {
    async fn resolve(&self, _headers: &dyn RequestHeaders) -> Result<AuthContext, AuthError> {
        self.maybe_nag();
        Ok(AuthContext::dev_null())
    }

    fn id(&self) -> &'static str {
        "dev_null"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::{NoHeaders, Scope};

    #[tokio::test]
    async fn resolves_admin_context() {
        let p = DevNullProvider::new();
        let ctx = p.resolve(&NoHeaders).await.unwrap();
        assert!(ctx.require(Scope::WriteSlots).is_ok());
        assert_eq!(ctx.tenant.as_str(), "default");
    }

    #[tokio::test]
    async fn id_is_stable() {
        let p = DevNullProvider::new();
        assert_eq!(p.id(), "dev_null");
    }
}
