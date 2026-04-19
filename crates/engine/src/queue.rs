//! Bridge from the graph crate's synchronous [`EventSink`] to the
//! engine's async worker.
//!
//! The graph emits events synchronously under no lock once a mutation
//! has committed. A direct synchronous fan-out would re-enter the
//! graph (live-wire writes trigger more events), so events land on an
//! [`mpsc`] queue instead and the engine worker drains them in order on
//! its own task. Decoupling also keeps the graph free of an async
//! runtime \u{2014} the caller chooses.
//!
//! **Bound.** We use an unbounded channel on purpose in Stage 2:
//! boundedness for fleet-scale backpressure is the job of the outbox
//! introduced in Stage 7 per `docs/design/RUNTIME.md`. When that lands,
//! this channel is replaced by the outbox and the API here collapses
//! into it. Until then, queue depth is a tracing metric.

use std::sync::Arc;

use graph::{EventSink, GraphEvent};
use tokio::sync::mpsc;

/// Sender half, handed to `GraphStore::new`. Implements [`EventSink`]
/// so the graph can call it without knowing about tokio.
#[derive(Clone)]
pub struct QueueSink {
    tx: mpsc::UnboundedSender<GraphEvent>,
}

impl QueueSink {
    pub fn arc(self) -> Arc<dyn EventSink> {
        Arc::new(self)
    }
}

impl EventSink for QueueSink {
    fn emit(&self, event: GraphEvent) {
        if self.tx.send(event).is_err() {
            // The worker has shut down. This is expected during
            // `Stopping` \u{2014} graph mutations committed while we were
            // draining are simply dropped. Surface as trace (not warn)
            // so it doesn't look like a problem in logs.
            tracing::trace!("graph event after engine shutdown \u{2014} dropped");
        }
    }
}

/// Paired sink + receiver.
///
/// Typical wiring in the agent:
/// ```ignore
/// let (sink, rx) = engine::queue::channel();
/// let graph = Arc::new(GraphStore::new(kinds, sink));
/// let engine = Engine::new(graph.clone(), rx);
/// engine.start().await?;
/// ```
pub fn channel() -> (Arc<dyn EventSink>, mpsc::UnboundedReceiver<GraphEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (QueueSink { tx }.arc(), rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::{KindId, NodeId, NodePath};

    #[tokio::test]
    async fn sink_forwards_to_receiver() {
        let (sink, mut rx) = channel();
        sink.emit(GraphEvent::NodeCreated {
            id: NodeId::new(),
            kind: KindId::new("acme.core.station"),
            path: NodePath::root(),
        });
        let ev = rx.recv().await.expect("event");
        assert!(matches!(ev, GraphEvent::NodeCreated { .. }));
    }

    #[tokio::test]
    async fn send_after_receiver_drop_is_silent() {
        let (sink, rx) = channel();
        drop(rx);
        // Must not panic.
        sink.emit(GraphEvent::NodeRemoved {
            id: NodeId::new(),
            kind: KindId::new("x"),
            path: NodePath::root(),
        });
    }
}
