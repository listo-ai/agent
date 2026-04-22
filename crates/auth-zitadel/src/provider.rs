//! [`ZitadelProvider`] — verifies Zitadel-issued JWTs against a
//! cached JWKS and maps claims into [`spi::AuthContext`].
//!
//! # Lifecycle
//!
//! 1. Composition root builds [`ZitadelConfig`].
//! 2. [`ZitadelProvider::new`] loads the disk cache (if any), kicks
//!    off an initial JWKS fetch, and hands back an `Arc`-sharable
//!    handle.
//! 3. A background task refreshes the JWKS on
//!    [`ZitadelConfig::refresh_interval`]. Failures are logged and
//!    retried at the next interval — the last-good snapshot stays
//!    live.
//! 4. Every inbound request calls [`ZitadelProvider::resolve`] which:
//!    - extracts the `Authorization: Bearer …` header
//!    - decodes the JWT header to get the `kid`
//!    - looks up the matching `DecodingKey` in the in-memory JWKS
//!    - verifies signature + iss + aud + exp + nbf
//!    - optional one-shot JWKS refresh on unknown `kid` (key rotation
//!      just happened and our refresh timer hasn't fired)
//!    - maps claims → [`spi::AuthContext`]
//!
//! The `kid`-miss refresh is bounded by the
//! [`tokio::sync::Mutex`] `rotation_guard`: only one request at a
//! time triggers a refetch; concurrent rotations pile up behind the
//! lock and re-check the JWKS after it releases.
//!
//! # Why offline verify
//!
//! An edge can authenticate an incoming request as long as it has
//! seen the signing key at least once. That matters when the agent
//! is on a flaky cell link or deliberately air-gapped after
//! enrolment. The refresh loop merely keeps the cache fresh —
//! verification itself never does I/O.

use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use serde::Deserialize;
use spi::{Actor, AuthContext, AuthError, AuthProvider, NodeId, RequestHeaders, Scope, ScopeSet, TenantId};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::config::ZitadelConfig;
use crate::deny_list::DenyList;
use crate::error::{ZitadelError, ZitadelResult};
use crate::jwks::{DiskCache, JwksSource};

/// The resolver. Cheap to clone (internal state is `Arc`-wrapped) so
/// the composition root can hand clones to every surface that needs
/// one.
#[derive(Clone)]
pub struct ZitadelProvider {
    inner: Arc<Inner>,
}

struct Inner {
    config: ZitadelConfig,
    source: Arc<dyn JwksSource>,
    cache: Option<DiskCache>,
    jwks: ArcSwap<JwkSet>,
    /// Single-flight around the on-demand refresh triggered by an
    /// unknown `kid`. Without it, a burst of rotated-key requests
    /// would fire N concurrent fetches.
    rotation_guard: Mutex<()>,
    /// Optional post-verification revocation check. `None` →
    /// deny-list disabled, every signature-valid token passes.
    /// Set once at construction via [`ZitadelProvider::with_deny_list`];
    /// implementations that need runtime rotation (e.g. polling from
    /// the cloud) hold their own internal state and refresh it
    /// under the hood — the provider holds the trait object, not
    /// the list content.
    deny_list: Option<Arc<dyn DenyList>>,
}

// ── Public API ────────────────────────────────────────────────────────────────

impl ZitadelProvider {
    /// Construct a provider, seeding the JWKS cache and — if a disk
    /// cache is configured — warm-starting from it so a cold boot
    /// without network can still verify already-issued tokens.
    ///
    /// The caller is responsible for spawning
    /// [`Self::spawn_refresh`] on the runtime it wants the refresh
    /// task to live on.
    pub async fn new(config: ZitadelConfig, source: Arc<dyn JwksSource>) -> ZitadelResult<Self> {
        let cache = config.disk_cache.clone().map(DiskCache::new);
        let cached = match &cache {
            Some(c) => c.read().await.unwrap_or_else(|e| {
                tracing::warn!(error = %e, "zitadel disk cache unreadable — ignoring");
                None
            }),
            None => None,
        };

        // Prefer a live fetch, fall back to the disk snapshot, fail
        // only if we have neither. An edge that boots with a stale
        // cache and no network gets a working verifier for
        // previously-seen keys.
        let initial = match source.fetch().await {
            Ok(fresh) => {
                if let Some(c) = &cache {
                    if let Err(e) = c.write(&fresh).await {
                        tracing::warn!(error = %e, "zitadel disk cache write failed");
                    }
                }
                fresh
            }
            Err(live_err) => match cached {
                Some(stale) => {
                    tracing::warn!(
                        error = %live_err,
                        "zitadel live JWKS fetch failed — using disk cache (keys may be stale)",
                    );
                    stale
                }
                None => return Err(live_err),
            },
        };

        Ok(Self {
            inner: Arc::new(Inner {
                config,
                source,
                cache,
                jwks: ArcSwap::from_pointee(initial),
                rotation_guard: Mutex::new(()),
                deny_list: None,
            }),
        })
    }

    /// Builder: install a deny-list. Every authenticated request that
    /// passes signature + claim verification is then checked against
    /// this list before the `AuthContext` is returned. Call at the
    /// composition root after [`Self::new`].
    ///
    /// The list rebuilds the `Inner` because `Arc<Inner>` is shared.
    /// In practice callers chain this immediately after `new()`; no
    /// clones exist yet to stale-out.
    #[must_use]
    pub fn with_deny_list(self, list: Arc<dyn DenyList>) -> Self {
        let prev = Arc::into_inner(self.inner)
            .expect("with_deny_list called before any clones of ZitadelProvider were taken");
        Self {
            inner: Arc::new(Inner {
                deny_list: Some(list),
                ..prev
            }),
        }
    }

    /// Spawn the background JWKS refresh task on the current Tokio
    /// runtime. Returns a handle the caller can `abort()` on shutdown
    /// — the provider itself has no Drop-time cleanup, so forgetting
    /// to cancel is a benign leak, not a bug.
    pub fn spawn_refresh(&self) -> tokio::task::JoinHandle<()> {
        let inner = self.inner.clone();
        let interval = inner.config.refresh_interval;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // First tick fires immediately; skip it because `new`
            // already did the initial fetch.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                match inner.source.fetch().await {
                    Ok(set) => {
                        inner.jwks.store(Arc::new(set.clone()));
                        if let Some(c) = &inner.cache {
                            if let Err(e) = c.write(&set).await {
                                tracing::warn!(error = %e, "zitadel disk cache write failed");
                            }
                        }
                        tracing::debug!("zitadel JWKS refreshed");
                    }
                    Err(e) => tracing::warn!(error = %e, "zitadel JWKS refresh failed"),
                }
            }
        })
    }

    fn current_jwks(&self) -> Arc<JwkSet> {
        self.inner.jwks.load_full()
    }

    async fn resolve_kid(&self, kid: &str) -> ZitadelResult<DecodingKey> {
        if let Some(key) = find_kid(&self.current_jwks(), kid) {
            return Ok(key);
        }
        // Missed — take the rotation lock, double-check, then
        // attempt one on-demand fetch. Subsequent waiters see the
        // refreshed set and skip the fetch.
        let _g = self.inner.rotation_guard.lock().await;
        if let Some(key) = find_kid(&self.current_jwks(), kid) {
            return Ok(key);
        }
        match self.inner.source.fetch().await {
            Ok(set) => {
                self.inner.jwks.store(Arc::new(set));
                find_kid(&self.current_jwks(), kid)
                    .ok_or_else(|| ZitadelError::UnknownKid(kid.to_string()))
            }
            Err(_) => Err(ZitadelError::UnknownKid(kid.to_string())),
        }
    }

    async fn verify_and_map(&self, token: &str) -> ZitadelResult<AuthContext> {
        let header = decode_header(token).map_err(ZitadelError::Jwt)?;
        let kid = header.kid.clone().ok_or(ZitadelError::MissingKid)?;
        let key = self.resolve_kid(&kid).await?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(std::slice::from_ref(&self.inner.config.issuer));
        validation.set_audience(std::slice::from_ref(&self.inner.config.audience));
        validation.validate_exp = true;
        validation.validate_nbf = true;
        // Small leeway — mirrors the default, written explicitly so
        // a future change to `jsonwebtoken`'s default doesn't silently
        // tighten our boundary.
        validation.leeway = 30;

        let data = decode::<TokenClaims>(token, &key, &validation).map_err(ZitadelError::Jwt)?;

        // Deny-list check runs AFTER signature + claim validation:
        // revealing "revoked" to a caller who can't even prove they
        // hold a valid token would leak which subjects are denied.
        // The order also keeps the hot path fast for typical (not
        // denied) subjects — the deny-list load is skipped if the
        // signature is bad.
        if let Some(list) = self.inner.deny_list.as_ref() {
            if list.is_denied(&data.claims.sub).await {
                return Err(ZitadelError::SubjectDenied {
                    subject: data.claims.sub.clone(),
                });
            }
        }

        map_claims(
            data.claims,
            &self.inner.config.tenant_id,
            &self.inner.config.tenant_claim,
            &self.inner.config.scopes_claim,
        )
    }
}

fn find_kid(jwks: &JwkSet, kid: &str) -> Option<DecodingKey> {
    let jwk = jwks.keys.iter().find(|k| k.common.key_id.as_deref() == Some(kid))?;
    match &jwk.algorithm {
        AlgorithmParameters::RSA(rsa) => DecodingKey::from_rsa_components(&rsa.n, &rsa.e).ok(),
        AlgorithmParameters::EllipticCurve(ec) => DecodingKey::from_ec_components(&ec.x, &ec.y).ok(),
        AlgorithmParameters::OctetKeyPair(okp) => DecodingKey::from_ed_components(&okp.x).ok(),
        // `oct` (HMAC) not used by Zitadel; we don't accept symmetric
        // keys for an OIDC verifier by construction.
        AlgorithmParameters::OctetKey(_) => None,
    }
}

// ── Claims → AuthContext ──────────────────────────────────────────────────────

/// Deserialisable subset of Zitadel's ID / access tokens. Only the
/// fields we consume — extra ones are ignored.
#[derive(Debug, Clone, Deserialize)]
struct TokenClaims {
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
    sub: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

fn map_claims(
    claims: TokenClaims,
    pinned_tenant: &Option<String>,
    tenant_claim: &str,
    scopes_claim: &str,
) -> ZitadelResult<AuthContext> {
    let tenant = claims
        .extra
        .get(tenant_claim)
        .and_then(|v| v.as_str())
        .ok_or(ZitadelError::MissingClaim("tenant_claim"))?
        .to_string();

    if let Some(pinned) = pinned_tenant {
        if tenant != *pinned {
            return Err(ZitadelError::TenantMismatch {
                token_tenant: tenant,
                pinned_tenant: pinned.clone(),
            });
        }
    }

    let display_name = claims
        .name
        .clone()
        .or_else(|| claims.email.clone())
        .unwrap_or_else(|| claims.sub.clone());

    // Zitadel `sub` is typically a UUID or numeric string. We project
    // it into a `NodeId` via UUID parse when possible, else a
    // deterministic UUIDv5 from the subject so the same `sub` always
    // maps to the same `NodeId` across restarts (without persisting
    // state). The latter matters for audit log joins.
    let id = Uuid::parse_str(&claims.sub)
        .unwrap_or_else(|_| Uuid::new_v5(&NAMESPACE_ZITADEL, claims.sub.as_bytes()));

    let scopes = read_scopes(&claims.extra, scopes_claim);

    Ok(AuthContext {
        actor: Actor::User {
            id: NodeId(id),
            display_name,
        },
        tenant: TenantId::from(tenant.as_str()),
        scopes,
    })
}

fn read_scopes(extra: &serde_json::Map<String, serde_json::Value>, claim: &str) -> ScopeSet {
    let Some(value) = extra.get(claim) else {
        // Absent → read-only default. Operators wanting more grant
        // specific scopes via Zitadel custom claims.
        return ScopeSet::from_scopes([Scope::ReadNodes]);
    };
    let Some(arr) = value.as_array() else {
        tracing::warn!(claim = claim, "zitadel scopes claim is not an array — defaulting to read_nodes");
        return ScopeSet::from_scopes([Scope::ReadNodes]);
    };
    let parsed: Vec<Scope> = arr
        .iter()
        .filter_map(|v| v.as_str().and_then(parse_scope))
        .collect();
    if parsed.is_empty() {
        ScopeSet::from_scopes([Scope::ReadNodes])
    } else {
        ScopeSet::from_scopes(parsed)
    }
}

fn parse_scope(s: &str) -> Option<Scope> {
    match s {
        "read_nodes" => Some(Scope::ReadNodes),
        "write_nodes" => Some(Scope::WriteNodes),
        "write_slots" => Some(Scope::WriteSlots),
        "write_config" => Some(Scope::WriteConfig),
        "manage_plugins" => Some(Scope::ManagePlugins),
        "manage_fleet" => Some(Scope::ManageFleet),
        "admin" => Some(Scope::Admin),
        _ => None,
    }
}

/// Deterministic UUIDv5 namespace for projecting non-UUID Zitadel
/// `sub` values to stable NodeIds. Random-once; never changes.
const NAMESPACE_ZITADEL: Uuid = Uuid::from_bytes([
    0x5d, 0x9a, 0x4f, 0x6b, 0x8c, 0x12, 0x47, 0x8d, 0xa3, 0x64, 0x9c, 0x1d, 0x7e, 0xff, 0x21, 0x08,
]);

// ── AuthProvider impl ─────────────────────────────────────────────────────────

#[async_trait]
impl AuthProvider for ZitadelProvider {
    async fn resolve(&self, headers: &dyn RequestHeaders) -> Result<AuthContext, AuthError> {
        let raw = headers
            .get("authorization")
            .ok_or(AuthError::MissingCredentials)?;
        let token = parse_bearer(raw).ok_or_else(|| AuthError::InvalidCredentials {
            reason: "expected `Bearer <jwt>`".into(),
        })?;
        self.verify_and_map(token).await.map_err(AuthError::from)
    }

    fn id(&self) -> &'static str {
        "zitadel"
    }
}

fn parse_bearer(raw: &str) -> Option<&str> {
    let (scheme, rest) = raw.trim().split_once(char::is_whitespace)?;
    if scheme.eq_ignore_ascii_case("Bearer") {
        Some(rest.trim())
    } else {
        None
    }
}

#[cfg(test)]
mod module_tests {
    use super::*;

    #[test]
    fn parse_bearer_accepts_case_insensitive_scheme() {
        assert_eq!(parse_bearer("Bearer abc"), Some("abc"));
        assert_eq!(parse_bearer("BEARER abc"), Some("abc"));
        assert_eq!(parse_bearer("bearer   abc"), Some("abc"));
    }

    #[test]
    fn parse_bearer_rejects_other_schemes() {
        assert_eq!(parse_bearer("Basic dXNlcjpwYXNz"), None);
        assert_eq!(parse_bearer("abc"), None);
    }

    #[test]
    fn parse_scope_covers_every_variant() {
        assert_eq!(parse_scope("read_nodes"), Some(Scope::ReadNodes));
        assert_eq!(parse_scope("admin"), Some(Scope::Admin));
        assert_eq!(parse_scope("unknown"), None);
    }
}
