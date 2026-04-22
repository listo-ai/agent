//! JWKS sources + on-disk cache.
//!
//! The verifier doesn't care where its keys come from — just that it
//! can resolve a `kid` to a [`jsonwebtoken::DecodingKey`]. Everything
//! behind the trait can change without touching verification logic.
//!
//! Sources shipping in this crate:
//!
//! - [`StaticJwksSource`] — a `JwkSet` held in memory. Used by tests
//!   to feed a pre-generated keypair, and by production as a
//!   fallback loaded off disk when the network is unavailable.
//! - [`HttpJwksSource`] — GETs the Zitadel `jwks_url` and parses the
//!   response. Production default.
//!
//! Callers that want a merged view (disk cache + HTTP refresh) chain
//! the two via [`ZitadelProvider`]'s built-in refresh task rather
//! than expressing the composition at this layer. That keeps each
//! source small and inspectable.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jsonwebtoken::jwk::JwkSet;

use crate::error::{ZitadelError, ZitadelResult};

/// Load a JWKS snapshot.
///
/// Implementations must be cheap to `Arc`-share: the provider clones
/// a `Arc<dyn JwksSource>` across refresh tasks and request paths.
/// Errors surface as [`ZitadelError`] so the verifier doesn't have to
/// juggle multiple error shapes.
#[async_trait]
pub trait JwksSource: Send + Sync {
    async fn fetch(&self) -> ZitadelResult<JwkSet>;
}

/// In-memory JWKS. Trivially cheap to fetch — returns a clone of the
/// stored set. Tests inject one of these; production falls back to
/// one if the disk cache loaded but the network is offline at boot.
pub struct StaticJwksSource {
    set: JwkSet,
}

impl StaticJwksSource {
    pub fn new(set: JwkSet) -> Self {
        Self { set }
    }
}

#[async_trait]
impl JwksSource for StaticJwksSource {
    async fn fetch(&self) -> ZitadelResult<JwkSet> {
        Ok(self.set.clone())
    }
}

/// HTTP source — `GET <url>` → parse as `JwkSet`. The `http` client
/// is injected so callers can configure timeouts, proxies, or
/// custom TLS roots at the composition root.
pub struct HttpJwksSource {
    url: String,
    http: reqwest::Client,
    timeout: Duration,
}

impl HttpJwksSource {
    pub fn new(url: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            url: url.into(),
            http,
            timeout: Duration::from_secs(10),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait]
impl JwksSource for HttpJwksSource {
    async fn fetch(&self) -> ZitadelResult<JwkSet> {
        // Short, explicit per-request timeout. Relying on the global
        // client timeout would silently pass requests through when
        // unset, and a 30-second default inside a 10-minute refresh
        // loop would stack pending fetches on a dead peer.
        let bytes = self
            .http
            .get(&self.url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|source| ZitadelError::JwksFetch {
                url: self.url.clone(),
                source,
            })?
            .error_for_status()
            .map_err(|source| ZitadelError::JwksFetch {
                url: self.url.clone(),
                source,
            })?
            .bytes()
            .await
            .map_err(|source| ZitadelError::JwksFetch {
                url: self.url.clone(),
                source,
            })?;
        serde_json::from_slice::<JwkSet>(&bytes).map_err(|source| ZitadelError::JwksParse {
            url: self.url.clone(),
            source,
        })
    }
}

// ── Disk cache ────────────────────────────────────────────────────────────────

/// On-disk persistence of the latest good JWKS. Independent of the
/// live source so a cold boot without network access can still
/// verify tokens signed with previously-seen keys.
///
/// Atomic writes via temp-file + rename — a crash mid-write never
/// leaves a half-written cache.
pub struct DiskCache {
    path: PathBuf,
}

impl DiskCache {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Read the cache if it exists. `Ok(None)` on first boot (no
    /// file). `Err` on parse / IO failure — caller typically logs and
    /// falls through to a live fetch.
    pub async fn read(&self) -> ZitadelResult<Option<JwkSet>> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => {
                let set: JwkSet = serde_json::from_slice(&bytes).map_err(|source| {
                    ZitadelError::JwksParse {
                        url: format!("disk://{}", self.path.display()),
                        source,
                    }
                })?;
                Ok(Some(set))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(ZitadelError::CacheRead {
                path: self.path.clone(),
                source,
            }),
        }
    }

    /// Atomic write. Serialises to a sibling `.tmp` then renames.
    pub async fn write(&self, set: &JwkSet) -> ZitadelResult<()> {
        let body = serde_json::to_vec(set).map_err(|source| ZitadelError::JwksParse {
            url: format!("disk://{}", self.path.display()),
            source,
        })?;
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|source| ZitadelError::CacheWrite {
                        path: parent.to_path_buf(),
                        source,
                    })?;
            }
        }
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &body)
            .await
            .map_err(|source| ZitadelError::CacheWrite {
                path: tmp.clone(),
                source,
            })?;
        tokio::fs::rename(&tmp, &self.path)
            .await
            .map_err(|source| ZitadelError::CacheWrite {
                path: self.path.clone(),
                source,
            })
    }
}

/// Convenience: wrap a `JwksSource` as `Arc<dyn JwksSource>` when
/// callers need trait-object erasure. Kept public so integration
/// tests can compose sources without repeating the cast.
#[must_use]
#[allow(dead_code)] // Re-exported for test/integration use; crate-local callers don't need it yet.
pub fn boxed<T: JwksSource + 'static>(source: T) -> Arc<dyn JwksSource> {
    Arc::new(source)
}
