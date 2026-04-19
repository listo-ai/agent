//! Shared state passed to every request handler.

use std::sync::Arc;

use engine::BehaviorRegistry;
use extensions_host::PluginRegistry;
use graph::{GraphEvent, GraphStore};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub graph: Arc<GraphStore>,
    pub behaviors: BehaviorRegistry,
    pub events: broadcast::Sender<GraphEvent>,
    pub plugins: PluginRegistry,
}

impl AppState {
    pub fn new(
        graph: Arc<GraphStore>,
        behaviors: BehaviorRegistry,
        events: broadcast::Sender<GraphEvent>,
        plugins: PluginRegistry,
    ) -> Self {
        Self {
            graph,
            behaviors,
            events,
            plugins,
        }
    }
}
