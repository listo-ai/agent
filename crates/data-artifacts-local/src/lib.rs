//! Filesystem-backed `ArtifactStore` — for dev loops, single-node
//! deployments, air-gapped sites, and integration tests.
//!
//! Backed by `object_store`'s local backend. "Presigned URLs" are
//! path tokens the in-process REST handler redeems; they are **not**
//! cryptographic and this backend must not be used where real
//! tenant-scoped isolation is required.
//!
//! See `agent/docs/design/ARTIFACTS.md`.
//!
//! STATUS: scaffolding — struct + trait impl with `todo!()` bodies.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use spi::artifacts::{
    ArtifactError, ArtifactKey, ArtifactStore, ByteStream, Integrity, PresignDirection,
    PresignedUrl,
};

#[derive(Debug, Clone)]
pub struct LocalConfig {
    pub root: PathBuf,
    // TODO: token_ttl_default
}

pub struct LocalArtifactStore {
    _cfg: LocalConfig,
    // TODO: object_store::local::LocalFileSystem
    // TODO: token signer (HMAC over path + expiry)
}

impl LocalArtifactStore {
    pub fn new(_cfg: LocalConfig) -> Result<Self, ArtifactError> {
        todo!("scaffolding")
    }
}

#[async_trait]
impl ArtifactStore for LocalArtifactStore {
    async fn put(&self, _key: &ArtifactKey, _bytes: ByteStream) -> Result<(), ArtifactError> {
        todo!()
    }

    async fn get(&self, _key: &ArtifactKey) -> Result<ByteStream, ArtifactError> {
        todo!()
    }

    async fn head(&self, _key: &ArtifactKey) -> Result<Option<Integrity>, ArtifactError> {
        todo!()
    }

    async fn presign(
        &self,
        _key: &ArtifactKey,
        _direction: PresignDirection,
        _ttl: Duration,
    ) -> Result<PresignedUrl, ArtifactError> {
        todo!()
    }

    fn id(&self) -> &'static str {
        "local"
    }
}
