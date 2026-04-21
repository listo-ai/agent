//! Per-node AST cache.
//!
//! Rhai script compilation is O(script length) and happens per
//! invocation if naively implemented. For steady-state flows that fire
//! many times a second, that's wasteful: the script doesn't change
//! between ticks. We cache the compiled AST keyed by NodeId, with a
//! small hash of the source so a config edit (script rewrite) is
//! automatically picked up on the next dispatch.
//!
//! Why NodeId and not kind: every Function node has its own script.
//! NodeId is stable across the node's lifetime, guaranteed unique by
//! the graph, and already stitched through `NodeCtx`.
//!
//! Why a hash and not a slot generation: the engine's config-dispatch
//! path already calls `dispatch_init` on a settings change, and that
//! doesn't help us — `on_init` is a good hook for *connection*
//! lifecycle but the actual compile happens lazily on the first
//! `on_message`. The script-hash check keeps the fast path one
//! comparison without needing to hook lifecycle events.
//!
//! Eviction: currently none. Memory is bounded by "number of active
//! Function nodes × average script size", which for realistic loads
//! is trivial. When hot-unload lands (Stage 10 per BLOCKS.md) the
//! shutdown hook can drop the entry.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{OnceLock, RwLock};

use rhai::AST;
use spi::NodeId;

pub(crate) struct CachedScript {
    pub(crate) hash: u64,
    pub(crate) ast: AST,
}

fn cache() -> &'static RwLock<HashMap<NodeId, CachedScript>> {
    static CACHE: OnceLock<RwLock<HashMap<NodeId, CachedScript>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// FxHash-style fast non-crypto hash of the script. We only need to
/// detect "script changed since last compile", collisions are fine —
/// a collision means we skip a recompile when we shouldn't, but the
/// next actual edit will flip the hash and recompile. Worst-case
/// perf, not correctness.
pub(crate) fn hash_script(script: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    script.hash(&mut h);
    h.finish()
}

/// Fetch the cached AST for a node if its script hash matches.
pub(crate) fn get(node: NodeId, expected_hash: u64) -> Option<AST> {
    let guard = cache().read().ok()?;
    guard.get(&node).and_then(|c| {
        if c.hash == expected_hash {
            Some(c.ast.clone())
        } else {
            None
        }
    })
}

pub(crate) fn put(node: NodeId, hash: u64, ast: AST) {
    if let Ok(mut guard) = cache().write() {
        guard.insert(node, CachedScript { hash, ast });
    }
}

/// Drop a node's cached entry. Called from `on_shutdown` so deleted
/// nodes don't pin their ASTs forever.
pub(crate) fn evict(node: NodeId) {
    if let Ok(mut guard) = cache().write() {
        guard.remove(&node);
    }
}
