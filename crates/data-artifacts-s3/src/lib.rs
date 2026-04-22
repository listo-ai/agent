//! S3-compatible `ArtifactStore` implementation.
//!
//! Backed by the `object_store` crate. One binary speaks to every
//! credible S3-compatible endpoint:
//!
//! - **Garage** (self-hosted reference) — pure-Rust, geo-distributed.
//! - **Cloudflare R2** — managed, zero egress fees.
//! - **AWS S3** — enterprise.
//! - **Backblaze B2** — cost-optimised.
//!
//! Restricts itself to the compatibility subset in ARTIFACTS.md § 5.3
//! (PUT/GET/HEAD/DELETE/LIST + presign + Object Lock + lifecycle) —
//! no SSE-KMS, no Intelligent-Tiering, no Inventory.
//!
//! See `agent/docs/design/ARTIFACTS.md`.
//!
//! STATUS: scaffolding — struct + trait impl with `todo!()` bodies.

#![forbid(unsafe_code)]

use std::time::Duration;

use async_trait::async_trait;
use spi::artifacts::{
    ArtifactError, ArtifactKey, ArtifactStore, ByteStream, Integrity, PresignDirection,
    PresignedUrl,
};

/// Connection + credentials for an S3-compatible endpoint.
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    // TODO: credentials_from (env / kms / static)
    // TODO: path_style bool (Garage/MinIO vs AWS)
    // TODO: presign_ttl_max
}

/// S3-backed `ArtifactStore`.
pub struct S3ArtifactStore {
    // TODO: object_store::aws::AmazonS3
    // TODO: presigner
    _cfg: S3Config,
}

impl S3ArtifactStore {
    pub fn new(_cfg: S3Config) -> Result<Self, ArtifactError> {
        todo!("scaffolding")
    }
}

#[async_trait]
impl ArtifactStore for S3ArtifactStore {
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
        "s3"
    }
}
