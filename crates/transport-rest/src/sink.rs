//! Composite [`EventSink`] — fans every graph event out to the engine
//! worker AND to the REST SSE broadcast. One write, two listeners, no
//! ordering guarantee between them (but each listener sees events in
//! the order they committed to the graph).
//!
//! The sink also assigns the monotonic `seq` counter and wall-clock
//! `ts` (ms since Unix epoch) before forwarding, so every downstream
//! subscriber sees a [`SequencedEvent`] rather than a raw
//! [`GraphEvent`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use graph::{EventSink, GraphEvent};
use tokio::sync::{broadcast, mpsc};

use crate::event::SequencedEvent;
use crate::ring::EventRing;

pub struct AgentSink {
    engine_tx: mpsc::UnboundedSender<GraphEvent>,
    bcast_tx: broadcast::Sender<SequencedEvent>,
    ring: EventRing,
    seq: AtomicU64,
}

impl AgentSink {
    pub fn new(
        engine_tx: mpsc::UnboundedSender<GraphEvent>,
        bcast_tx: broadcast::Sender<SequencedEvent>,
        ring: EventRing,
    ) -> Self {
        Self {
            engine_tx,
            bcast_tx,
            ring,
            seq: AtomicU64::new(0),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl EventSink for AgentSink {
    fn emit(&self, event: GraphEvent) {
        // Forward the raw graph event to the engine worker unchanged.
        if self.engine_tx.send(event.clone()).is_err() {
            tracing::trace!("engine receiver gone — dropping event");
        }

        // Assign a monotonic seq and wall-clock ts, then fan out.
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let ts = now_ms();
        let sequenced = SequencedEvent { seq, ts, event };

        // Store in ring first so ?since= replay is consistent.
        self.ring.push(sequenced.clone());

        // send returns Err when there are *no* SSE subscribers — normal
        // when nobody is watching, not an error.
        let _ = self.bcast_tx.send(sequenced);
    }
}
