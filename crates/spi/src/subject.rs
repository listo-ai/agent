//! Fleet transport subject namespace.
//!
//! Canonical shape per `docs/design/FLEET-TRANSPORT.md`:
//!
//! ```text
//! fleet.<tenant>.<agent-id>.<kind>.<...>
//! ```
//!
//! Any backend (NATS subjects, Zenoh key-expressions, MQTT topics) wraps
//! this same structure; the wire separator is a backend concern, but the
//! hierarchy and the dot-escape rule apply uniformly.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::auth::TenantId;

/// One parsed token in a subject path. Stored already-escaped so
/// `join` / `Display` never surprise.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectToken(String);

impl SubjectToken {
    /// Encode a raw string into a valid token. The escape rule matches
    /// `docs/design/PLUGINS.md § "Path-segment encoding"`: dots in the
    /// raw input become underscores in the token (e.g. node kind
    /// `com.acme.hello` → token `com_acme_hello`). Whitespace and other
    /// unusual bytes collapse to `_` so operators never see ambiguous
    /// subjects.
    pub fn encode(raw: &str) -> Self {
        let mut out = String::with_capacity(raw.len());
        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                out.push(ch);
            } else {
                out.push('_');
            }
        }
        Self(out)
    }

    /// Accept a token that is already in subject form (no dots, no
    /// whitespace). Returns `None` if the token would need escaping —
    /// callers should route those through `encode`.
    pub fn verbatim(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }
        if s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            Some(Self(s.to_string()))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SubjectToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A fleet subject. Opaque to callers: construct with the builder, pass
/// to a `FleetTransport`, let the backend render it into its native
/// separator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Subject {
    /// Canonical dotted form. Backends that use `/` (Zenoh) translate
    /// on the edge of the trait.
    dotted: String,
}

impl Subject {
    /// Start building a subject for a specific `(tenant, agent-id)` pair.
    pub fn for_agent(tenant: &TenantId, agent_id: &str) -> SubjectBuilder {
        SubjectBuilder {
            tokens: vec![
                SubjectToken::verbatim("fleet").expect("literal"),
                SubjectToken::encode(tenant.as_str()),
                SubjectToken::encode(agent_id),
            ],
        }
    }

    /// Wildcard across the whole tenant — every agent, every kind.
    pub fn tenant_wildcard(tenant: &TenantId) -> Self {
        Self {
            dotted: format!("fleet.{}.>", SubjectToken::encode(tenant.as_str())),
        }
    }

    pub fn as_dotted(&self) -> &str {
        &self.dotted
    }

    /// Construct a subject directly from an already-canonical dotted
    /// string. Used by transports to round-trip an inbound subject back
    /// into the type — the string must already follow the escape rules,
    /// typically because it came from `as_dotted()` or was translated
    /// from a backend-native form (e.g. Zenoh `/` → `.`).
    pub fn from_dotted(dotted: impl Into<String>) -> Self {
        Self {
            dotted: dotted.into(),
        }
    }

    /// Render with a custom separator (e.g. `/` for Zenoh). Each token
    /// is already-escaped so the separator appears only where intended.
    pub fn render(&self, sep: char) -> String {
        if sep == '.' {
            self.dotted.clone()
        } else {
            self.dotted.replace('.', &sep.to_string())
        }
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.dotted)
    }
}

/// Builder for a fully-qualified subject. Forces you to compose in
/// order: prefix already set from `Subject::for_agent`, then kind, then
/// any further tokens.
pub struct SubjectBuilder {
    tokens: Vec<SubjectToken>,
}

impl SubjectBuilder {
    /// Append an already-escaped kind segment chain such as
    /// `"api.v1.nodes.list"`. Each dot-separated piece becomes its own
    /// token; pieces are validated.
    pub fn kind(mut self, chain: &str) -> Self {
        for piece in chain.split('.') {
            let tok = SubjectToken::verbatim(piece).unwrap_or_else(|| SubjectToken::encode(piece));
            self.tokens.push(tok);
        }
        self
    }

    /// Append a raw segment — escapes if necessary.
    pub fn segment(mut self, raw: &str) -> Self {
        self.tokens.push(SubjectToken::encode(raw));
        self
    }

    pub fn build(self) -> Subject {
        let dotted = self
            .tokens
            .into_iter()
            .map(|t| t.0)
            .collect::<Vec<_>>()
            .join(".");
        Subject { dotted }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_escapes_dots_and_whitespace() {
        assert_eq!(
            SubjectToken::encode("com.acme.hello").as_str(),
            "com_acme_hello"
        );
        assert_eq!(SubjectToken::encode("edge 42").as_str(), "edge_42");
    }

    #[test]
    fn verbatim_rejects_dots() {
        assert!(SubjectToken::verbatim("api.v1").is_none());
        assert!(SubjectToken::verbatim("api").is_some());
    }

    #[test]
    fn builder_produces_canonical_form() {
        let s = Subject::for_agent(&TenantId::new("acme"), "edge-42")
            .kind("api.v1.nodes.list")
            .build();
        assert_eq!(s.as_dotted(), "fleet.acme.edge-42.api.v1.nodes.list");
    }

    #[test]
    fn builder_escapes_tenant_with_dot() {
        // Tenant names coming from config could legitimately contain dots;
        // they must be escaped so the namespace hierarchy stays intact.
        let s = Subject::for_agent(&TenantId::new("acme.prod"), "edge-42")
            .kind("cmd.plugin.install")
            .build();
        assert_eq!(s.as_dotted(), "fleet.acme_prod.edge-42.cmd.plugin.install");
    }

    #[test]
    fn render_swaps_separator_for_zenoh_style() {
        let s = Subject::for_agent(&TenantId::new("acme"), "edge-42")
            .kind("event.graph.slot")
            .build();
        assert_eq!(s.render('/'), "fleet/acme/edge-42/event/graph/slot");
    }

    #[test]
    fn tenant_wildcard_matches_everything_in_tenant() {
        let w = Subject::tenant_wildcard(&TenantId::new("acme"));
        assert_eq!(w.as_dotted(), "fleet.acme.>");
    }
}
