//! Integrity + signature verification.
//!
//! Fetched bytes are verified against (a) the SHA-256 hash in the
//! control message and (b) the ed25519 signature over the manifest.
//! Mismatch = reject, no partial apply.
//!
//! STATUS: scaffolding.

use thiserror::Error;

/// Result of a successful verification — caller can hand the bytes to
/// the installer.
#[derive(Debug)]
pub struct VerifiedArtifact {
    // TODO: bytes + manifest + key
}

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("hash mismatch")]
    HashMismatch,
    #[error("signature invalid")]
    SignatureInvalid,
    #[error("manifest malformed: {0}")]
    ManifestMalformed(String),
}

// TODO: pub fn verify_manifest(...) -> Result<VerifiedManifest, VerificationError>
// TODO: pub async fn verify_stream(...) -> Result<VerifiedArtifact, VerificationError>
