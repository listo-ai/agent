//! Runtime configuration for [`crate::ZitadelProvider`].
//!
//! Mirrors the resolved `config::AuthConfig::Zitadel` shape plus
//! knobs that are provider-internal (refresh cadence, cache path,
//! claim name overrides). Kept as plain data so the composition root
//! can build it in one place and the provider never reaches into
//! global config.

use std::path::PathBuf;
use std::time::Duration;

/// Default Zitadel claim carrying the organisation (tenant) id.
/// Configurable via [`ZitadelConfig::tenant_claim`] because some
/// Zitadel installations expose tenancy under an alternate URN
/// (e.g. `orgId`, `listo_org`, or a project-owner claim).
pub const DEFAULT_TENANT_CLAIM: &str = "urn:zitadel:iam:user:resourceowner:id";

/// Default Zitadel claim carrying an array of platform scopes
/// (`["read_nodes", "write_slots", ...]`). Absent → [`ReadNodes`].
/// Operators who want richer mappings (Zitadel roles → platform
/// scopes) extend their Zitadel action + OIDC custom claim config;
/// the provider stays dumb and reads whatever claim the operator
/// populates.
///
/// [`ReadNodes`]: spi::Scope::ReadNodes
pub const DEFAULT_SCOPES_CLAIM: &str = "listo_scopes";

/// Default JWKS refresh interval. Short enough to pick up key
/// rotation within an hour, long enough not to hammer Zitadel.
pub const DEFAULT_REFRESH: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub struct ZitadelConfig {
    /// OIDC `iss` — the Zitadel tenant URL (e.g.
    /// `https://acme.zitadel.cloud`). Must match the `iss` claim in
    /// inbound tokens exactly.
    pub issuer: String,
    /// Expected `aud` claim value — the Zitadel application / client
    /// id the agent is authenticating for. Rejected tokens with the
    /// wrong audience belong to a different relying party.
    pub audience: String,
    /// Where to fetch the JWKS document. Usually
    /// `<issuer>/oauth/v2/keys`.
    pub jwks_url: String,
    /// `Some(tenant)` pins the provider to a single org id (edge
    /// install). A token with any other `tenant_claim` value is
    /// rejected with [`crate::ZitadelError::TenantMismatch`].
    /// `None` = multi-tenant cloud; tenancy is taken from the claim
    /// as-is.
    pub tenant_id: Option<String>,
    /// Claim name carrying the tenant (org) id. See
    /// [`DEFAULT_TENANT_CLAIM`].
    pub tenant_claim: String,
    /// Claim name carrying the scopes array. See
    /// [`DEFAULT_SCOPES_CLAIM`].
    pub scopes_claim: String,
    /// JWKS refresh cadence. Keys rotated at the Zitadel side become
    /// usable here within this interval. See [`DEFAULT_REFRESH`].
    pub refresh_interval: Duration,
    /// Optional on-disk persistence of the latest JWKS so a cold
    /// boot without cloud access can still verify tokens signed with
    /// previously-fetched keys. `None` = in-memory only.
    pub disk_cache: Option<PathBuf>,
}

impl ZitadelConfig {
    /// Minimal construction with sensible defaults on the
    /// provider-internal knobs.
    pub fn new(issuer: impl Into<String>, audience: impl Into<String>, jwks_url: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            jwks_url: jwks_url.into(),
            tenant_id: None,
            tenant_claim: DEFAULT_TENANT_CLAIM.to_string(),
            scopes_claim: DEFAULT_SCOPES_CLAIM.to_string(),
            refresh_interval: DEFAULT_REFRESH,
            disk_cache: None,
        }
    }

    /// Pin to a single tenant (edge install). Tokens carrying any
    /// other org id are rejected.
    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    pub fn with_disk_cache(mut self, path: PathBuf) -> Self {
        self.disk_cache = Some(path);
        self
    }

    pub fn with_refresh_interval(mut self, interval: Duration) -> Self {
        self.refresh_interval = interval;
        self
    }

    pub fn with_tenant_claim(mut self, claim: impl Into<String>) -> Self {
        self.tenant_claim = claim.into();
        self
    }

    pub fn with_scopes_claim(mut self, claim: impl Into<String>) -> Self {
        self.scopes_claim = claim.into();
        self
    }
}
