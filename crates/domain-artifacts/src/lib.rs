//! Pure artefact-distribution logic.
//!
//! This crate decides *what* to fetch, verifies it on arrival, and
//! manages the edge-side cache. It does **not** talk to S3, HTTP, or
//! any backend — those are in `data-artifacts-s3` /
//! `data-artifacts-local` behind Cargo features.
//!
//! Consumes the [`ArtifactStore`] trait from `spi`; every backend
//! satisfies the same contract.
//!
//! See `agent/docs/design/ARTIFACTS.md`.
//!
//! STATUS: scaffolding — module skeletons with TODOs, no logic yet.

#![forbid(unsafe_code)]

pub mod cache;
pub mod distribute;
pub mod keys;
pub mod verify;

pub use verify::{VerificationError, VerifiedArtifact};

use bytes::Bytes;
use futures_util::stream;
use spi::artifacts::{ArtifactError, ArtifactKey, ArtifactStore, ByteStream};
use tokio::io::AsyncReadExt;

/// Stream bytes from an [`AsyncRead`] source into an [`ArtifactStore`].
///
/// This is the integration seam used by the backup exporter: one half
/// of a `tokio::io::duplex()` pair writes the bundle; the read half is
/// passed here and piped into the store without a second copy (beyond
/// the internal read buffer).
///
/// The key must be pre-constructed via the `spi::artifacts::keys`
/// helpers so the layout matches ARTIFACTS.md § 3.
///
/// Returns `Ok(())` when all bytes have been accepted by the store.
pub async fn upload_stream<S, R>(
    store: &S,
    key: &ArtifactKey,
    mut reader: R,
) -> Result<(), ArtifactError>
where
    S: ArtifactStore,
    R: AsyncReadExt + Send + Unpin,
{
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| ArtifactError::Backend(format!("read error: {e}")))?;
    let data: ByteStream = Box::pin(stream::once(async move { Ok(Bytes::from(buf)) }));
    store.put(key, data).await
}
