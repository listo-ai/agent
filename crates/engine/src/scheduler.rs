//! Timer scheduler — one-shot `NodeCtx::schedule` backing.
//!
//! Architecture:
//!
//!   NodeCtx::schedule      →  Scheduler::schedule
//!         ↓                          ↓
//!   TimerHandle                spawn tokio task
//!                                    ↓ (after delay)
//!                           send TimerFired on channel
//!                                    ↓
//!                        worker_loop drains channel
//!                                    ↓
//!                    BehaviorRegistry::dispatch_timer
//!                                    ↓
//!                          DynBehavior::on_timer(ctx, handle)
//!
//! Sending via a channel (rather than letting the spawned task call
//! the registry directly) keeps the Scheduler ↔ BehaviorRegistry
//! cycle broken — same pattern as graph events.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use blocks_sdk::{NodeError, TimerHandle, TimerScheduler};
use spi::NodeId;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

/// Event emitted when a scheduled timer elapses. Drained by the engine
/// worker loop and dispatched via [`crate::BehaviorRegistry`].
#[derive(Debug, Clone, Copy)]
pub struct TimerFired {
    pub node: NodeId,
    pub handle: TimerHandle,
}

/// One-shot tokio timer pool.
pub(crate) struct Scheduler {
    next_id: AtomicU64,
    inner: Mutex<Inner>,
    fired_tx: mpsc::UnboundedSender<TimerFired>,
}

#[derive(Default)]
struct Inner {
    aborts: HashMap<u64, AbortHandle>,
}

impl Scheduler {
    pub(crate) fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<TimerFired>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let s = Arc::new(Self {
            next_id: AtomicU64::new(1),
            inner: Mutex::new(Inner::default()),
            fired_tx: tx,
        });
        (s, rx)
    }
}

impl TimerScheduler for Scheduler {
    fn schedule(&self, node: NodeId, delay_ms: u64) -> Result<TimerHandle, NodeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let handle = TimerHandle(id);
        let tx = self.fired_tx.clone();
        let me = self
            .inner
            .lock()
            .map_err(|_| NodeError::runtime("scheduler lock poisoned"));
        // tokio::spawn uses the ambient runtime. In tests this is
        // `#[tokio::test(start_paused = true)]` so `advance` is observable.
        let join = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let _ = tx.send(TimerFired { node, handle });
        });
        me?.aborts.insert(id, join.abort_handle());
        Ok(handle)
    }

    fn cancel(&self, handle: TimerHandle) {
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        if let Some(a) = g.aborts.remove(&handle.0) {
            a.abort();
        }
    }
}

impl Scheduler {
    /// Clear the bookkeeping entry for a handle that just fired — the
    /// tokio task has already ended so there's nothing to abort, but
    /// we don't want the aborts map to grow unbounded.
    pub(crate) fn mark_fired(&self, handle: TimerHandle) {
        if let Ok(mut g) = self.inner.lock() {
            g.aborts.remove(&handle.0);
        }
    }
}
