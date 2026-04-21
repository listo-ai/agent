//! Auth seam — cross-cutting identity types threaded through every
//! mutating code path.
//!
//! See `docs/sessions/AUTH-SEAM.md` for the shipping plan. This module
//! holds the *shape*: types + trait. Concrete providers live in the
//! `auth` crate; handler wiring lives in `transport-rest`.

use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::ids::NodeId;

/// Tenant the request operates against. Today always `"default"`; the
/// column exists so every mutation path already validates.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TenantId(String);

impl TenantId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn default_tenant() -> Self {
        Self("default".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TenantId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Who is acting on a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    /// Human user backed by an identity-provider session.
    User { id: NodeId, display_name: String },
    /// Machine identity — service account, block publisher, edge agent.
    Machine { id: NodeId, label: String },
    /// Dev-null default. NEVER valid in production — the agent refuses to
    /// boot with a dev-null provider when `role == cloud` and the build
    /// is `--release`.
    DevNull,
}

impl Actor {
    /// The stable node id, if the actor has one.
    pub fn node_id(&self) -> Option<NodeId> {
        match self {
            Actor::User { id, .. } | Actor::Machine { id, .. } => Some(*id),
            Actor::DevNull => None,
        }
    }

    pub fn display(&self) -> &str {
        match self {
            Actor::User { display_name, .. } => display_name,
            Actor::Machine { label, .. } => label,
            Actor::DevNull => "local-dev-null",
        }
    }
}

/// Coarse-grained permission atoms. Finer RBAC is a later landing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    ReadNodes,
    WriteNodes,
    WriteSlots,
    WriteConfig,
    ManagePlugins,
    ManageFleet,
    /// Implies all others. Reserved for bootstrap + emergency.
    Admin,
}

impl Scope {
    const fn bit(self) -> u32 {
        1u32 << (self as u32)
    }
}

/// Bitflag-backed set of scopes. Membership check is O(1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScopeSet(u32);

impl ScopeSet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn admin() -> Self {
        Self::empty().with(Scope::Admin)
    }

    pub const fn with(self, scope: Scope) -> Self {
        Self(self.0 | scope.bit())
    }

    pub const fn contains(self, scope: Scope) -> bool {
        // Admin implies every other scope.
        (self.0 & Scope::Admin.bit()) != 0 || (self.0 & scope.bit()) != 0
    }

    pub fn from_scopes<I: IntoIterator<Item = Scope>>(iter: I) -> Self {
        iter.into_iter().fold(Self::empty(), Self::with)
    }
}

/// Identity stamp attached to every inbound request. Small, cheap to
/// clone, serialisable for test fixtures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub actor: Actor,
    pub tenant: TenantId,
    pub scopes: ScopeSet,
}

impl AuthContext {
    /// Stamp used by `DevNullProvider`. Never valid in prod cloud.
    pub fn dev_null() -> Self {
        Self {
            actor: Actor::DevNull,
            tenant: TenantId::default_tenant(),
            scopes: ScopeSet::admin(),
        }
    }

    /// Return `Ok(())` if the context may perform `scope`, else a
    /// structured error that transport layers can map to 403.
    pub fn require(&self, scope: Scope) -> Result<(), AuthError> {
        if self.scopes.contains(scope) {
            Ok(())
        } else {
            Err(AuthError::MissingScope {
                required: scope,
                actor: self.actor.display().to_string(),
            })
        }
    }

    /// `true` if this context is authorised for the given tenant.
    pub fn owns(&self, tenant: &TenantId) -> bool {
        &self.tenant == tenant
    }
}

/// Structured auth failure. Transports map these to HTTP status / fleet
/// error codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AuthError {
    #[error("missing credentials")]
    MissingCredentials,
    #[error("credentials rejected: {reason}")]
    InvalidCredentials { reason: String },
    #[error("scope `{required:?}` required (actor `{actor}`)")]
    MissingScope { required: Scope, actor: String },
    #[error("wrong tenant")]
    WrongTenant,
    #[error("provider error: {0}")]
    Provider(String),
}

/// Backend-agnostic header accessor. Kept tiny on purpose so neither
/// `spi` nor `auth` have to depend on `http`. Transports (axum, NATS,
/// gRPC) wrap their native header bag in a thin impl.
pub trait RequestHeaders: Send + Sync {
    fn get(&self, name: &str) -> Option<&str>;
}

/// No headers at all — useful for bootstrap paths and tests.
pub struct NoHeaders;

impl RequestHeaders for NoHeaders {
    fn get(&self, _name: &str) -> Option<&str> {
        None
    }
}

impl RequestHeaders for &[(&str, &str)] {
    fn get(&self, name: &str) -> Option<&str> {
        self.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
    }
}

/// Auth provider: resolves an `AuthContext` from raw request metadata.
///
/// One trait, many impls: dev-null, static-token, future Zitadel. Every
/// transport resolves through this before dispatching to handlers.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn resolve(&self, headers: &dyn RequestHeaders) -> Result<AuthContext, AuthError>;

    /// Stable provider id — surfaced in `GET /api/v1/capabilities` as
    /// `auth.<id>.v1` so blocks can require a given identity backend.
    fn id(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_scope_implies_all() {
        let ctx = AuthContext::dev_null();
        assert!(ctx.require(Scope::WriteSlots).is_ok());
        assert!(ctx.require(Scope::ManageFleet).is_ok());
    }

    #[test]
    fn missing_scope_reports_required() {
        let ctx = AuthContext {
            actor: Actor::Machine {
                id: NodeId::new(),
                label: "reader".into(),
            },
            tenant: TenantId::default_tenant(),
            scopes: ScopeSet::empty().with(Scope::ReadNodes),
        };
        assert!(ctx.require(Scope::ReadNodes).is_ok());
        let err = ctx.require(Scope::WriteSlots).unwrap_err();
        assert!(matches!(
            err,
            AuthError::MissingScope {
                required: Scope::WriteSlots,
                ..
            }
        ));
    }

    #[test]
    fn header_slice_accessor_is_case_insensitive() {
        let hs: &[(&str, &str)] = &[("Authorization", "Bearer abc")];
        assert_eq!(
            RequestHeaders::get(&hs, "authorization"),
            Some("Bearer abc")
        );
        assert_eq!(RequestHeaders::get(&hs, "x-missing"), None);
    }

    #[test]
    fn auth_context_json_round_trip() {
        let ctx = AuthContext::dev_null();
        let s = serde_json::to_string(&ctx).unwrap();
        let back: AuthContext = serde_json::from_str(&s).unwrap();
        assert_eq!(back.tenant, ctx.tenant);
    }
}
