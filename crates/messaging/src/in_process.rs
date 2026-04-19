use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{broadcast, Mutex};

use crate::bus::{futures_recv, MessageBus, Subscription};
use crate::MessagingError;

/// In-process bus. Exact-match subjects only (no wildcards yet — wildcards
/// land with the NATS impl where the semantics are defined).
///
/// Capacity is per-subject; slow subscribers lag and drop oldest.
pub struct InProcessBus {
    capacity: usize,
    channels: Mutex<HashMap<String, broadcast::Sender<Vec<u8>>>>,
}

impl InProcessBus {
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            capacity,
            channels: Mutex::new(HashMap::new()),
        })
    }

    async fn sender(&self, subject: &str) -> broadcast::Sender<Vec<u8>> {
        let mut map = self.channels.lock().await;
        map.entry(subject.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .clone()
    }
}

#[async_trait]
impl MessageBus for InProcessBus {
    async fn publish(&self, subject: &str, payload: Vec<u8>) -> Result<(), MessagingError> {
        let tx = self.sender(subject).await;
        // No receivers is not an error — fire-and-forget semantics.
        let _ = tx.send(payload);
        Ok(())
    }

    async fn subscribe(&self, subject: &str) -> Result<Box<dyn Subscription>, MessagingError> {
        let rx = self.sender(subject).await.subscribe();
        Ok(Box::new(InProcessSubscription { rx }))
    }
}

struct InProcessSubscription {
    rx: broadcast::Receiver<Vec<u8>>,
}

impl Subscription for InProcessSubscription {
    fn recv(&mut self) -> futures_recv::BoxFuture<'_, Option<Vec<u8>>> {
        Box::pin(async move {
            loop {
                match self.rx.recv().await {
                    Ok(v) => return Some(v),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_reaches_subscriber() {
        let bus = InProcessBus::new(16);
        let mut sub = bus.subscribe("graph.test.created").await.unwrap();
        bus.publish("graph.test.created", b"hello".to_vec())
            .await
            .unwrap();
        let msg = sub.recv().await.unwrap();
        assert_eq!(msg, b"hello");
    }

    #[tokio::test]
    async fn publish_with_no_subscribers_is_ok() {
        let bus = InProcessBus::new(16);
        bus.publish("graph.nobody", b"x".to_vec()).await.unwrap();
    }
}
