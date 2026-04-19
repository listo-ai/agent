use async_trait::async_trait;

use crate::MessagingError;

/// Subject string. Dot-separated, NATS-compatible taxonomy
/// (`tenant.{id}.graph.{path}.slot.{slot}.changed`, etc.). The exact
/// grammar is frozen in Stage 7 per RUNTIME.md and EVERYTHING-AS-NODE.md.
pub type Subject = String;

/// A live subscription. Dropping the handle unsubscribes.
pub trait Subscription: Send + 'static {
    /// Fetch the next message, or `None` if the subscription is closed.
    fn recv(&mut self) -> futures_recv::BoxFuture<'_, Option<Vec<u8>>>;
}

/// Trait-object-friendly future alias so `Subscription` stays object-safe
/// without dragging async-trait into the public shape. The real NATS
/// impl can use this same signature.
pub mod futures_recv {
    use std::future::Future;
    use std::pin::Pin;

    pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
}

#[async_trait]
pub trait MessageBus: Send + Sync + 'static {
    async fn publish(&self, subject: &str, payload: Vec<u8>) -> Result<(), MessagingError>;
    async fn subscribe(&self, subject: &str) -> Result<Box<dyn Subscription>, MessagingError>;
}
