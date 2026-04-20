use async_trait::async_trait;
use std::sync::Arc;

use crate::types::{Event, Provider, RunConfig, RunResult};

/// Shared event callback — cheap to clone across async tasks.
pub type OnEvent = Arc<dyn Fn(Event) + Send + Sync + 'static>;

/// Every AI backend implements this trait.
#[async_trait]
pub trait Runner: Send + Sync {
    fn provider(&self) -> Provider;

    /// `true` if the backend is installed / reachable.
    ///
    /// CLI runners check that the binary is on `PATH`.
    /// REST runners always return `true` (key absence is caught at run-time).
    fn available(&self) -> bool;

    /// Run a prompt, streaming events to `on_event`.
    ///
    /// Blocks (async) until the run completes or the context is cancelled.
    async fn run(&self, cfg: RunConfig, session_id: String, on_event: OnEvent) -> RunResult;
}
