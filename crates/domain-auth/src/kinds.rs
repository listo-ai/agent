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

/// First-boot setup status. Seeded at `/agent/setup` when
/// `AuthConfig::SetupRequired` is resolved.
///
/// The `status` slot drives the REST 503 gate and the `SetupService`
/// single-flight check. Slots are `writable: false` on the tenant
/// surface — operators and flows cannot PATCH them through the normal
/// slot API. The bootstrapper and `SetupService` write them internally
/// via `GraphStore` (which does not enforce tenant-surface
/// writability).
///
/// See `docs/design/SYSTEM-BOOTSTRAP.md` for the full state-machine
/// transitions.
#[derive(blocks_sdk::NodeKind)]
#[node(
    kind = "sys.auth.setup",
    manifest = "manifests/setup.yaml",
    behavior = "none"
)]
pub struct SetupNode;
