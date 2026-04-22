//! Thin REST handlers for the artefact subsystem.
//!
//! Route inventory (paths to be wired into `routes.rs`):
//!
//! ```text
//!   POST /api/v1/artifacts/presign-upload     → mint PUT URL for bundle upload
//!   POST /api/v1/artifacts/presign-download   → mint GET URL for an existing artefact
//!   POST /api/v1/artifacts/receipt            → record a completed edge→cloud upload
//!   GET  /api/v1/artifacts/:kind/:id          → HEAD / metadata lookup
//! ```
//!
//! Each handler is ≤ 20 lines per Rule I
//! ([HOW-TO-ADD-CODE.md](../../../docs/design/HOW-TO-ADD-CODE.md)):
//! extract → call core fn in `domain-artifacts` → map to DTO → return.
//! Any logic (quota check, key derivation, JWT→tenant resolution)
//! lives in the domain crate.
//!
//! STATUS: scaffolding — signatures only, bodies to follow.

// TODO: pub async fn presign_upload(...) -> Result<Json<PresignUploadResponse>, ApiError>
// TODO: pub async fn presign_download(...) -> Result<Json<PresignDownloadResponse>, ApiError>
// TODO: pub async fn receipt(...) -> Result<StatusCode, ApiError>
// TODO: pub async fn head(...) -> Result<Json<HeadResponse>, ApiError>
//
// TODO: pub fn router() -> Router<AppState>
