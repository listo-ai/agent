//! Static-token provider — maps bearer tokens to pre-built `AuthContext`
//! values from config. For two-user local-dev multi-actor scenarios and
//! test fixtures. NOT a long-term identity strategy.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use spi::{Actor, AuthContext, AuthError, AuthProvider, RequestHeaders, Scope, ScopeSet, TenantId};

/// One entry in the static-token table. Shape matches what the config
/// overlay loader ultimately parses from YAML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticTokenEntry {
    pub token: String,
    pub actor: Actor,
    pub tenant: TenantId,
    pub scopes: Vec<Scope>,
}

impl StaticTokenEntry {
    fn into_context(self) -> (String, AuthContext) {
        let ctx = AuthContext {
            actor: self.actor,
            tenant: self.tenant,
            scopes: ScopeSet::from_iter(self.scopes),
        };
        (self.token, ctx)
    }
}

/// Resolves `Authorization: Bearer <token>` against a fixed table.
pub struct StaticTokenProvider {
    table: HashMap<String, AuthContext>,
}

impl StaticTokenProvider {
    pub fn new(entries: impl IntoIterator<Item = StaticTokenEntry>) -> Self {
        let table = entries
            .into_iter()
            .map(StaticTokenEntry::into_context)
            .collect();
        Self { table }
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

fn parse_bearer(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    let (scheme, rest) = raw.split_once(char::is_whitespace)?;
    if scheme.eq_ignore_ascii_case("Bearer") {
        Some(rest.trim())
    } else {
        None
    }
}

#[async_trait]
impl AuthProvider for StaticTokenProvider {
    async fn resolve(&self, headers: &dyn RequestHeaders) -> Result<AuthContext, AuthError> {
        let raw = headers
            .get("authorization")
            .ok_or(AuthError::MissingCredentials)?;
        let token = parse_bearer(raw).ok_or_else(|| AuthError::InvalidCredentials {
            reason: "expected `Bearer <token>`".into(),
        })?;
        self.table
            .get(token)
            .cloned()
            .ok_or_else(|| AuthError::InvalidCredentials {
                reason: "unknown token".into(),
            })
    }

    fn id(&self) -> &'static str {
        "static_token"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::{NodeId, Scope};

    fn fixture() -> StaticTokenProvider {
        StaticTokenProvider::new([
            StaticTokenEntry {
                token: "alice-token".into(),
                actor: Actor::User {
                    id: NodeId::new(),
                    display_name: "Alice".into(),
                },
                tenant: TenantId::default_tenant(),
                scopes: vec![Scope::ReadNodes, Scope::WriteSlots],
            },
            StaticTokenEntry {
                token: "reader-token".into(),
                actor: Actor::Machine {
                    id: NodeId::new(),
                    label: "reader".into(),
                },
                tenant: TenantId::default_tenant(),
                scopes: vec![Scope::ReadNodes],
            },
        ])
    }

    #[tokio::test]
    async fn resolves_known_bearer() {
        let p = fixture();
        let hs: &[(&str, &str)] = &[("authorization", "Bearer alice-token")];
        let ctx = p.resolve(&hs).await.unwrap();
        assert!(ctx.require(Scope::WriteSlots).is_ok());
    }

    #[tokio::test]
    async fn scope_enforcement_is_per_entry() {
        let p = fixture();
        let hs: &[(&str, &str)] = &[("authorization", "bearer reader-token")];
        let ctx = p.resolve(&hs).await.unwrap();
        assert!(ctx.require(Scope::ReadNodes).is_ok());
        assert!(ctx.require(Scope::WriteSlots).is_err());
    }

    #[tokio::test]
    async fn missing_header_is_missing_credentials() {
        let p = fixture();
        let hs: &[(&str, &str)] = &[];
        let err = p.resolve(&hs).await.unwrap_err();
        assert!(matches!(err, AuthError::MissingCredentials));
    }

    #[tokio::test]
    async fn wrong_scheme_is_rejected() {
        let p = fixture();
        let hs: &[(&str, &str)] = &[("authorization", "Basic dXNlcjpwYXNz")];
        let err = p.resolve(&hs).await.unwrap_err();
        assert!(matches!(err, AuthError::InvalidCredentials { .. }));
    }

    #[tokio::test]
    async fn unknown_token_is_rejected() {
        let p = fixture();
        let hs: &[(&str, &str)] = &[("authorization", "Bearer nope")];
        let err = p.resolve(&hs).await.unwrap_err();
        assert!(matches!(err, AuthError::InvalidCredentials { .. }));
    }
}
