//! Composite [`EventSink`] — fans every graph event out to the engine
//! worker AND to the REST SSE broadcast. One write, two listeners, no
//! ordering guarantee between them (but each listener sees events in
//! the order they committed to the graph).

use graph::{EventSink, GraphEvent};
use tokio::sync::{broadcast, mpsc};

pub struct AgentSink {
    engine_tx: mpsc::UnboundedSender<GraphEvent>,
    bcast_tx: broadcast::Sender<GraphEvent>,
}

impl AgentSink {
    pub fn new(
        engine_tx: mpsc::UnboundedSender<GraphEvent>,
        bcast_tx: broadcast::Sender<GraphEvent>,
    ) -> Self {
        Self {
            engine_tx,
            bcast_tx,
        }
    }
}

impl EventSink for AgentSink {
    fn emit(&self, event: GraphEvent) {
        if self.engine_tx.send(event.clone()).is_err() {
            tracing::trace!("engine receiver gone \u{2014} dropping event");
        }
        // send returns Err(SendError) when there are *no* subscribers;
        // that's expected when nobody is watching SSE, not an error.
        let _ = self.bcast_tx.send(event);
    }
}
