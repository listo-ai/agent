//! Kind registry — runtime lookup of `KindManifest` values registered by
//! domain crates and extensions. The manifest type itself lives in
//! [`spi::KindManifest`] so the SDK never has to depend on this crate.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use spi::{KindId, KindManifest};

/// Thread-safe kind registry.
///
/// Registrations are cheap and typically happen once at agent startup.
/// Lookups are frequent — placement enforcement reads from here on every
/// mutation. The `RwLock` makes reads cheap and writes exclusive.
#[derive(Debug, Default, Clone)]
pub struct KindRegistry {
    inner: Arc<RwLock<HashMap<KindId, KindManifest>>>,
}

impl KindRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a kind. Replaces any existing manifest with the same id.
    pub fn register(&self, manifest: KindManifest) {
        let mut map = self
            .inner
            .write()
            .expect("KindRegistry lock poisoned — programmer error");
        tracing::debug!(kind = %manifest.id, "registering kind");
        map.insert(manifest.id.clone(), manifest);
    }

    pub fn get(&self, id: &KindId) -> Option<KindManifest> {
        let map = self.inner.read().ok()?;
        map.get(id).cloned()
    }

    pub fn contains(&self, id: &KindId) -> bool {
        self.inner
            .read()
            .map(|m| m.contains_key(id))
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.inner.read().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
