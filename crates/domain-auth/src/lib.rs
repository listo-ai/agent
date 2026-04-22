//! Auth-entity node kinds + the first-boot `SetupService`.
//!
//! **Node kinds** — `sys.auth.tenant`, `sys.auth.user`, `sys.auth.setup`.
//! Mirrors Zitadel identity entities per `docs/sessions/AUTH-SEAM.md`.
//! The `config.tags` slot on `sys.auth.user` supports Studio
//! bulk-action filtering without touching the trust model (see
//! `docs/design/AUTH.md § "No teams"`).
//!
//! **SetupService** — the domain-layer orchestrator for first-boot
//! setup and edge enrollment. Owns the single-flight mutex, token
//! generation, graph slot writes, config writeback, and provider
//! hot-swap. Every transport (REST, CLI, future gRPC / fleet) calls
//! `SetupService::complete_local`; none of them own any of this
//! logic. See `docs/design/SYSTEM-BOOTSTRAP.md` for the data flow.

mod error;
mod kinds;
mod setup;

pub use error::DomainAuthError;
pub use kinds::{SetupNode, TenantNode, UserNode};
pub use setup::{
    setup_node_path, OrgInfo, SetupError, SetupMode, SetupOutcome, SetupService, SetupWriteback,
};

use graph::KindRegistry;

/// Register `sys.auth.tenant`, `sys.auth.user`, and `sys.auth.setup`
/// kinds.
///
/// Call once at agent startup alongside the other `domain_*::register_kinds`
/// invocations.
pub fn register_kinds(kinds: &KindRegistry) {
    kinds.register(<TenantNode as blocks_sdk::NodeKind>::manifest());
    kinds.register(<UserNode as blocks_sdk::NodeKind>::manifest());
    kinds.register(<SetupNode as blocks_sdk::NodeKind>::manifest());
}
