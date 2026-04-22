//! Edge-side content-addressed cache.
//!
//! Fetched artefacts land under `var/cache/artifacts/<sha256>` and are
//! immutable. Eviction is LRU with a configurable byte cap
//! (`artifacts.cache_bytes`, default 2 GB).
//!
//! STATUS: scaffolding.

// TODO: pub struct ArtifactCache { root: PathBuf, cap_bytes: u64 }
// TODO: impl ArtifactCache {
//           pub async fn get(&self, sha256: &[u8; 32]) -> Option<PathBuf>
//           pub async fn put(&self, sha256: &[u8; 32], bytes: ByteStream) -> Result<PathBuf>
//           pub async fn evict_until(&self, target_free: u64) -> Result<u64>
//       }
