//! Filesystem-backed `ArtifactStore` — for dev loops, single-node
//! deployments, air-gapped sites, and integration tests.
//!
//! Backed by `object_store`'s local backend. "Presigned URLs" are
//! path tokens the in-process REST handler redeems; they are **not**
//! cryptographic and this backend must not be used where real
//! tenant-scoped isolation is required.
//!
//! See `agent/docs/design/ARTIFACTS.md`.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::TryStreamExt;
use object_store::local::LocalFileSystem;
use object_store::path::Path as OsPath;
use object_store::{ObjectStore, PutPayload};
use sha2::{Digest, Sha256};
use spi::artifacts::{
    ArtifactError, ArtifactKey, ArtifactStore, ByteStream, Integrity, PresignDirection,
    PresignedUrl,
};

#[derive(Debug, Clone)]
pub struct LocalConfig {
    pub root: PathBuf,
}

pub struct LocalArtifactStore {
    fs: LocalFileSystem,
}

impl LocalArtifactStore {
    pub fn new(cfg: LocalConfig) -> Result<Self, ArtifactError> {
        std::fs::create_dir_all(&cfg.root)
            .map_err(|e| ArtifactError::Backend(e.to_string()))?;
        let fs = LocalFileSystem::new_with_prefix(&cfg.root)
            .map_err(|e| ArtifactError::Backend(e.to_string()))?;
        Ok(Self { fs })
    }
}

fn key_to_path(key: &ArtifactKey) -> OsPath {
    OsPath::from(key.as_str())
}

async fn collect_stream(stream: ByteStream) -> Result<Bytes, ArtifactError> {
    let chunks: Vec<Bytes> = stream.try_collect().await?;
    let total = chunks.iter().map(|b| b.len()).sum();
    let mut out = bytes::BytesMut::with_capacity(total);
    for chunk in chunks {
        out.extend_from_slice(&chunk);
    }
    Ok(out.freeze())
}

fn sha256_of(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

fn bytes_to_stream(b: Bytes) -> ByteStream {
    Box::pin(futures_util::stream::once(async move { Ok(b) }))
}

#[async_trait]
impl ArtifactStore for LocalArtifactStore {
    async fn put(&self, key: &ArtifactKey, bytes: ByteStream) -> Result<(), ArtifactError> {
        let path = key_to_path(key);
        let data = collect_stream(bytes).await?;
        let payload = PutPayload::from(data);
        self.fs
            .put(&path, payload)
            .await
            .map_err(|e| ArtifactError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, key: &ArtifactKey) -> Result<ByteStream, ArtifactError> {
        let path = key_to_path(key);
        let result = self.fs.get(&path).await.map_err(|e| {
            if matches!(e, object_store::Error::NotFound { .. }) {
                ArtifactError::NotFound { key: key.clone() }
            } else {
                ArtifactError::Backend(e.to_string())
            }
        })?;
        let data = result
            .bytes()
            .await
            .map_err(|e| ArtifactError::Backend(e.to_string()))?;
        Ok(bytes_to_stream(data))
    }

    async fn head(&self, key: &ArtifactKey) -> Result<Option<Integrity>, ArtifactError> {
        let path = key_to_path(key);
        match self.fs.head(&path).await {
            Ok(meta) => {
                let result = self
                    .fs
                    .get(&path)
                    .await
                    .map_err(|e| ArtifactError::Backend(e.to_string()))?;
                let data = result
                    .bytes()
                    .await
                    .map_err(|e| ArtifactError::Backend(e.to_string()))?;
                let sha256 = sha256_of(&data);
                Ok(Some(Integrity {
                    sha256,
                    size: meta.size as u64,
                }))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(ArtifactError::Backend(e.to_string())),
        }
    }

    async fn presign(
        &self,
        _key: &ArtifactKey,
        _direction: PresignDirection,
        _ttl: Duration,
    ) -> Result<PresignedUrl, ArtifactError> {
        Err(ArtifactError::Backend(
            "presign not supported for local artifact store".into(),
        ))
    }

    fn id(&self) -> &'static str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    #[tokio::test]
    async fn roundtrip_put_get() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalArtifactStore::new(LocalConfig { root: dir.path().to_path_buf() })
            .expect("create store");
        let key = "snapshots/test/123.listo-snapshot".to_string();
        let payload: ByteStream =
            Box::pin(stream::once(async { Ok(Bytes::from_static(b"hello artifact")) }));
        store.put(&key, payload).await.expect("put");
        let mut got = store.get(&key).await.expect("get");
        let chunk = got.try_next().await.unwrap().unwrap();
        assert_eq!(chunk, Bytes::from_static(b"hello artifact"));
    }

    #[tokio::test]
    async fn head_returns_integrity() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalArtifactStore::new(LocalConfig { root: dir.path().to_path_buf() })
            .expect("create store");
        let key = "snapshots/test/99.listo-snapshot".to_string();
        let data = b"integrity check";
        let payload: ByteStream =
            Box::pin(stream::once(async { Ok(Bytes::copy_from_slice(data)) }));
        store.put(&key, payload).await.unwrap();
        let integrity = store.head(&key).await.unwrap().expect("some");
        assert_eq!(integrity.size, data.len() as u64);
        assert_eq!(integrity.sha256, sha256_of(data));
    }

    #[tokio::test]
    async fn head_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalArtifactStore::new(LocalConfig { root: dir.path().to_path_buf() })
            .expect("create store");
        let result = store.head(&"no/such/key".to_string()).await.unwrap();
        assert!(result.is_none());
    }

    /// End-to-end: `tokio::io::duplex()` pair → `upload_stream` → `head` verify.
    ///
    /// This is the integration seam described in BACKUP.md § 6.4: one side of
    /// the duplex is written by the backup exporter; the other side is consumed
    /// by `domain_artifacts::upload_stream` and piped into the store.
    #[tokio::test]
    async fn upload_stream_via_duplex() {
        use tokio::io::AsyncWriteExt;

        let dir = tempfile::tempdir().unwrap();
        let store = LocalArtifactStore::new(LocalConfig { root: dir.path().to_path_buf() })
            .expect("create store");

        let key = "snapshots/e2e/42.listo-snapshot".to_string();
        let payload = b"bundle bytes from duplex writer";

        // Create a duplex pair: `writer` simulates the backup exporter,
        // `reader` is consumed by upload_stream.
        let (mut writer, reader) = tokio::io::duplex(1024);

        // Spawn a task that writes the payload then drops the writer
        // (signals EOF to the reader).
        let write_task = tokio::spawn(async move {
            writer.write_all(payload).await.unwrap();
            // drop(writer) causes EOF on reader side
        });

        domain_artifacts::upload_stream(&store, &key, reader)
            .await
            .expect("upload_stream");
        write_task.await.unwrap();

        let integrity = store.head(&key).await.unwrap().expect("integrity");
        assert_eq!(integrity.size, payload.len() as u64);
        assert_eq!(integrity.sha256, sha256_of(payload));
    }
}
