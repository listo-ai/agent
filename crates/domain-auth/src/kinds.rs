//! Node kind declarations for auth entities.

/// The tenant node — top-level container for users and service accounts.
#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.auth.tenant",
    manifest = "manifests/tenant.yaml",
    behavior = "none"
)]
pub struct TenantNode;

/// The user node — one per Zitadel user, synced on sign-in.
///
/// Carries a `config.tags` slot for operator-managed labels and key/value
/// metadata. Tags are NOT part of the trust model — they are saved
/// selections used by Studio bulk-action UX only.
#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.auth.user",
    manifest = "manifests/user.yaml",
    behavior = "none"
)]
pub struct UserNode;
