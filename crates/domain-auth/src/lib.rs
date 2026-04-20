//! Auth-entity node kinds — `sys.auth.tenant` and `sys.auth.user`.
//!
//! Registers graph node kinds that mirror Zitadel identity entities per
//! `docs/sessions/AUTH-SEAM.md § "Auth-as-nodes"`. The `config.tags`
//! slot on `sys.auth.user` enables Studio bulk-action filtering without
//! touching the trust model (see `docs/design/AUTH.md § "No teams"`).

mod error;
mod kinds;

pub use error::DomainAuthError;
pub use kinds::{TenantNode, UserNode};

use graph::KindRegistry;

/// Register `sys.auth.tenant` and `sys.auth.user` kinds.
///
/// Call once at agent startup alongside the other `domain_*::register_kinds`
/// invocations.
pub fn register_kinds(kinds: &KindRegistry) {
    kinds.register(<TenantNode as blocks_sdk::NodeKind>::manifest());
    kinds.register(<UserNode as blocks_sdk::NodeKind>::manifest());
}
