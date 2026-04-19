//! Widget-type registry.
//!
//! Extensions register widget types via `contributions.widgets[]` in
//! their manifest (extension-host machinery lands in later plumbing).
//! The dashboard layer cares only about the set of known ids + a
//! monotonic version that feeds the cache key.
//!
//! See DASHBOARD.md § "Backend responsibilities" #6 and the cache-key
//! spec (`widget_registry_version`).

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

#[derive(Debug, Default)]
pub struct WidgetRegistry {
    types: RwLock<BTreeSet<String>>,
    version: AtomicU64,
}

impl WidgetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a widget type id. Idempotent — re-registering the same
    /// id is a no-op and does not bump the version.
    pub fn register(&self, id: impl Into<String>) {
        let id = id.into();
        let inserted = self
            .types
            .write()
            .expect("WidgetRegistry poisoned")
            .insert(id);
        if inserted {
            self.version.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.types
            .read()
            .map(|s| s.contains(id))
            .unwrap_or(false)
    }

    /// Monotonic version bumped whenever a *new* type is added. Feeds
    /// the resolver's cache key so re-registration busts cached
    /// resolves.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// Snapshot of currently known types. Diagnostic helper; not on
    /// the resolve hot path.
    pub fn list(&self) -> Vec<String> {
        self.types
            .read()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_lookup() {
        let r = WidgetRegistry::new();
        assert_eq!(r.version(), 0);
        r.register("acme.card");
        assert!(r.contains("acme.card"));
        assert!(!r.contains("acme.unknown"));
        assert_eq!(r.version(), 1);
    }

    #[test]
    fn re_register_is_idempotent_and_does_not_bump_version() {
        let r = WidgetRegistry::new();
        r.register("acme.card");
        r.register("acme.card");
        assert_eq!(r.version(), 1);
    }

    #[test]
    fn distinct_registrations_bump_version() {
        let r = WidgetRegistry::new();
        r.register("a");
        r.register("b");
        assert_eq!(r.version(), 2);
    }
}
