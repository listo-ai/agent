//! Shared state passed to every request handler.

use std::sync::Arc;

use engine::BehaviorRegistry;
use graph::{GraphEvent, GraphStore};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub graph: Arc<GraphStore>,
    pub behaviors: BehaviorRegistry,
    pub events: broadcast::Sender<GraphEvent>,
}

impl AppState {
    pub fn new(
        graph: Arc<GraphStore>,
        behaviors: BehaviorRegistry,
        events: broadcast::Sender<GraphEvent>,
    ) -> Self {
        Self {
            graph,
            behaviors,
            events,
        }
    }
}
