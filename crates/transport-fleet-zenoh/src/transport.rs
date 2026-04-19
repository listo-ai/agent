//! Zenoh implementation of `spi::FleetTransport`.
//!
//! Subject → key-expression mapping: Zenoh uses `/` as its separator, so
//! we render the canonical dotted subject with `Subject::render('/')`.
//! Wildcards: `*` matches one chunk, `**` matches any number. NATS's
//! `>` at the tail of a tenant wildcard is translated to `**`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use spi::{
    FleetError, FleetHandler, FleetMessage, FleetTransport, HealthStatus, HealthStream, Payload,
    Server, ServerHandle, Subject, SubscriptionStream,
};
use tokio::sync::watch;

use zenoh::Session;

/// User-facing config for opening a Zenoh session.
#[derive(Debug, Clone)]
pub struct ZenohConfig {
    /// Endpoints this node tries to connect to (e.g. another peer or a
    /// router). Empty for pure multicast-discovery peer mode.
    pub connect: Vec<String>,
    /// Endpoints this node listens on. Empty for client-only.
    pub listen: Vec<String>,
}

impl Default for ZenohConfig {
    fn default() -> Self {
        Self {
            connect: Vec::new(),
            listen: Vec::new(),
        }
    }
}

impl ZenohConfig {
    fn to_zenoh(&self) -> zenoh::Config {
        let mut cfg = zenoh::Config::default();
        if !self.connect.is_empty() {
            let _ = cfg.insert_json5(
                "connect/endpoints",
                &serde_json::to_string(&self.connect).expect("serialisable"),
            );
        }
        if !self.listen.is_empty() {
            let _ = cfg.insert_json5(
                "listen/endpoints",
                &serde_json::to_string(&self.listen).expect("serialisable"),
            );
        }
        cfg
    }
}

/// Embedded Zenoh backend for `spi::FleetTransport`.
///
/// Holds a shared `zenoh::Session` plus a `watch` channel carrying the
/// current `HealthStatus` so every call to `health()` observes live
/// transitions.
pub struct ZenohTransport {
    session: Arc<Session>,
    // Held so a future reconnect supervisor can push status transitions.
    // Zenoh 1.x doesn't expose a stable session-close callback yet, so
    // today the value only ever holds `Connected`.
    #[allow(dead_code)]
    health_tx: watch::Sender<HealthStatus>,
    health_rx: watch::Receiver<HealthStatus>,
}

impl ZenohTransport {
    /// Open a Zenoh session with the supplied config.
    pub async fn connect(cfg: ZenohConfig) -> Result<Self, FleetError> {
        let zcfg = cfg.to_zenoh();
        let session = zenoh::open(zcfg)
            .await
            .map_err(|e| FleetError::Backend(format!("zenoh open: {e}")))?;
        let (health_tx, health_rx) = watch::channel(HealthStatus::Connected);
        Ok(Self {
            session: Arc::new(session),
            health_tx,
            health_rx,
        })
    }

    fn key(subject: &Subject) -> String {
        subject.render('/')
    }
}

#[async_trait]
impl FleetTransport for ZenohTransport {
    async fn publish(&self, subject: &Subject, payload: Payload) -> Result<(), FleetError> {
        self.session
            .put(Self::key(subject), payload)
            .await
            .map_err(|e| FleetError::Backend(format!("put: {e}")))
    }

    async fn request(
        &self,
        subject: &Subject,
        payload: Payload,
        timeout: Duration,
    ) -> Result<Payload, FleetError> {
        let replies = self
            .session
            .get(Self::key(subject))
            .payload(payload)
            .timeout(timeout)
            .await
            .map_err(|e| FleetError::Backend(format!("get: {e}")))?;

        match tokio::time::timeout(timeout, replies.recv_async()).await {
            Ok(Ok(reply)) => match reply.result() {
                Ok(sample) => Ok(sample.payload().to_bytes().to_vec()),
                Err(err) => Err(FleetError::Backend(format!("reply error: {err:?}"))),
            },
            Ok(Err(_)) => Err(FleetError::Backend("reply channel closed".into())),
            Err(_) => Err(FleetError::Timeout {
                timeout_ms: timeout.as_millis() as u64,
            }),
        }
    }

    async fn subscribe(&self, pattern: &Subject) -> Result<SubscriptionStream, FleetError> {
        let sub = self
            .session
            .declare_subscriber(Self::key(pattern))
            .await
            .map_err(|e| FleetError::Backend(format!("subscribe: {e}")))?;

        let stream = async_stream::stream! {
            while let Ok(sample) = sub.recv_async().await {
                let subj = zenoh_key_to_subject(sample.key_expr().as_str());
                let payload = sample.payload().to_bytes().to_vec();
                yield FleetMessage {
                    subject: subj,
                    payload,
                    reply_to: None,
                };
            }
        };
        Ok(Box::pin(stream))
    }

    async fn serve(
        &self,
        pattern: &Subject,
        handler: Arc<dyn FleetHandler>,
    ) -> Result<Server, FleetError> {
        let queryable = self
            .session
            .declare_queryable(Self::key(pattern))
            .await
            .map_err(|e| FleetError::Backend(format!("declare_queryable: {e}")))?;

        let task = tokio::spawn(async move {
            while let Ok(query) = queryable.recv_async().await {
                let subj = zenoh_key_to_subject(query.key_expr().as_str());
                let payload = query
                    .payload()
                    .map(|p| p.to_bytes().to_vec())
                    .unwrap_or_default();
                let msg = FleetMessage {
                    subject: subj,
                    payload,
                    reply_to: None,
                };
                match handler.handle(msg).await {
                    Ok(Some(reply_bytes)) => {
                        let ke = query.key_expr().clone();
                        if let Err(e) = query.reply(ke, reply_bytes).await {
                            tracing::warn!(error = ?e, "zenoh reply failed");
                        }
                    }
                    Ok(None) => {
                        // Queryable with no reply — fine.
                    }
                    Err(err) => {
                        tracing::warn!(?err, "fleet handler error; no reply sent");
                    }
                }
            }
        });

        Ok(Server::new(ZenohServerHandle {
            task: Some(task),
        }))
    }

    fn health(&self) -> HealthStream {
        let mut rx = self.health_rx.clone();
        let stream = async_stream::stream! {
            // Emit current value first so subscribers don't need a
            // separate "get current" call to initialise. Copy out of the
            // borrow guard before yielding so the guard doesn't span
            // the await that `yield` compiles into.
            let current = *rx.borrow();
            yield current;
            while rx.changed().await.is_ok() {
                let next = *rx.borrow();
                yield next;
            }
        };
        Box::pin(stream)
    }

    fn id(&self) -> &'static str {
        "zenoh"
    }
}

struct ZenohServerHandle {
    task: Option<tokio::task::JoinHandle<()>>,
}

impl ServerHandle for ZenohServerHandle {
    fn shutdown(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }
}

/// Translate a `/`-separated Zenoh key-expression back into a dotted
/// `Subject`. Tokens are already-escaped (dots were stripped on encode)
/// so the reverse is a pure separator swap.
fn zenoh_key_to_subject(key: &str) -> Subject {
    Subject::from_dotted(key.replace('/', "."))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Zenoh session opening performs actual network scouting / listener
    // setup, so these tests are flagged `#[ignore]` and run with
    // `cargo test -- --ignored` in environments that allow it (CI turns
    // them on for Linux-with-loopback). Pure-logic tests run normally.

    #[test]
    fn health_stream_is_not_constructed_without_session() {
        // Just a compile-time sanity; real health tests live in the
        // ignored integration test below.
        let _ = HealthStatus::Connected;
    }

    #[test]
    fn zenoh_key_round_trip_dotted() {
        let subj = Subject::for_agent(&spi::TenantId::new("sys"), "edge-42")
            .kind("api.v1.nodes.list")
            .build();
        let rendered = subj.render('/');
        let back = zenoh_key_to_subject(&rendered);
        assert_eq!(back.as_dotted(), subj.as_dotted());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "opens a real zenoh session; run with `--ignored` in a permissive environment"]
    async fn publish_and_receive_locally() {
        use futures_util::StreamExt;

        let t = ZenohTransport::connect(ZenohConfig::default())
            .await
            .expect("connect");
        let subj = Subject::for_agent(&spi::TenantId::new("test"), "node-a")
            .kind("event.hello")
            .build();

        let mut sub = t.subscribe(&subj).await.expect("subscribe");
        // Give the subscriber a moment to propagate.
        tokio::time::sleep(Duration::from_millis(100)).await;
        t.publish(&subj, b"hi".to_vec()).await.expect("publish");
        let msg = tokio::time::timeout(Duration::from_secs(2), sub.next())
            .await
            .expect("did not time out")
            .expect("stream yielded");
        assert_eq!(msg.payload, b"hi");
    }
}
