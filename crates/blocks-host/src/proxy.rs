//! `ProcessProxyBehavior` — adapter that lets the engine dispatch
//! `on_message` into a supervised process block over gRPC.
//!
//! Lifecycle:
//!
//!   1. The supervisor task spawns the block binary and finishes a
//!      successful `Describe` RPC. It then (once) constructs a proxy
//!      per declared kind and calls [`BehaviorRegistry::register`].
//!   2. The proxy holds an `Arc<RwLock<Option<ExtensionClient>>>` the
//!      supervisor task updates on each connect / disconnect. When
//!      `on_message` fires and the slot is `None` the message is
//!      logged and dropped — the next successful restart picks up
//!      future traffic.
//!   3. On final supervisor shutdown (cancel) the registered kinds are
//!      torn back down via [`BehaviorRegistry::unregister`] so stale
//!      proxies don't swallow writes after the block goes away.
//!
//! `on_message` is synchronous (the `NodeBehavior` trait is), so the
//! proxy spawns a tokio task for the actual gRPC call and returns
//! immediately. Emitted outputs land in the engine's `EmitSink` when
//! the RPC completes — asynchronous but causally ordered per-proxy.

use std::sync::Arc;

use blocks_sdk::error::NodeError;
use blocks_sdk::{DynBehavior, NodeCtx, TimerHandle};
use spi::{KindId, Msg};
use tokio::sync::RwLock;
use transport_grpc::{ExtensionClient, OnInitRequest, OnMessageRequest};

use crate::manifest::BlockId;

/// Shared handle the supervise_loop updates as the child connects /
/// disconnects. `None` during backoff / after shutdown; `Some` while
/// the block is live. Proxies hold an `Arc`-clone.
pub type ClientSlot = Arc<RwLock<Option<ExtensionClient<tonic::transport::Channel>>>>;

/// `DynBehavior` impl that proxies `on_message` to a supervised
/// process block.
pub struct ProcessProxyBehavior {
    block_id: BlockId,
    kind_id: KindId,
    client: ClientSlot,
}

impl ProcessProxyBehavior {
    pub fn new(block_id: BlockId, kind_id: KindId, client: ClientSlot) -> Self {
        Self {
            block_id,
            kind_id,
            client,
        }
    }
}

impl DynBehavior for ProcessProxyBehavior {
    fn on_init(&self, ctx: &NodeCtx, cfg: &serde_json::Value) -> Result<(), NodeError> {
        let block_id = self.block_id.clone();
        let kind_id = self.kind_id.clone();
        let client_slot = self.client.clone();
        let emit_sink = ctx.emit_sink();
        let node_id = ctx.node_id();
        let node_path = ctx.node_path().clone();

        let config_json = if cfg.is_null() {
            String::new()
        } else {
            cfg.to_string()
        };

        tokio::spawn(async move {
            let mut client = match client_slot.read().await.clone() {
                Some(c) => c,
                None => {
                    tracing::warn!(
                        block = %block_id, kind = %kind_id,
                        "process block not running — on_init dropped",
                    );
                    return;
                }
            };
            let req = OnInitRequest {
                node_path: node_path.as_str().to_string(),
                kind_id: kind_id.to_string(),
                config_json,
            };
            match client.on_init(req).await {
                Ok(resp) => {
                    let resp = resp.into_inner();
                    if !resp.ok {
                        tracing::warn!(
                            block = %block_id, kind = %kind_id, error = %resp.error,
                            "process on_init returned error",
                        );
                    }
                    for emit in resp.emitted {
                        let Ok(out_msg) = serde_json::from_str::<Msg>(&emit.msg_json) else {
                            continue;
                        };
                        let _ = emit_sink.emit(node_id, &emit.port, out_msg);
                    }
                }
                Err(status) => {
                    tracing::warn!(
                        block = %block_id, kind = %kind_id, error = %status,
                        "process on_init RPC failed",
                    );
                }
            }
        });
        Ok(())
    }

    fn on_message(&self, ctx: &NodeCtx, port: String, msg: Msg) -> Result<(), NodeError> {
        let block_id = self.block_id.clone();
        let kind_id = self.kind_id.clone();
        let client_slot = self.client.clone();

        // Data lifted off the borrowed ctx so the spawned task owns
        // everything it needs.
        let emit_sink = ctx.emit_sink();
        let node_id = ctx.node_id();
        let node_path = ctx.node_path().clone();
        let config = ctx.config().clone();

        let msg_json = serde_json::to_string(&msg)
            .map_err(|e| NodeError::runtime(format!("msg serialise: {e}")))?;
        let config_json = if config.is_null() {
            String::new()
        } else {
            config.to_string()
        };

        tokio::spawn(async move {
            let mut client = match client_slot.read().await.clone() {
                Some(c) => c,
                None => {
                    tracing::warn!(
                        block = %block_id, kind = %kind_id,
                        "process block not running — message dropped",
                    );
                    return;
                }
            };

            let req = OnMessageRequest {
                node_path: node_path.as_str().to_string(),
                kind_id: kind_id.to_string(),
                port,
                msg_json,
                config_json,
            };

            match client.on_message(req).await {
                Ok(resp) => {
                    let resp = resp.into_inner();
                    if !resp.ok {
                        tracing::warn!(
                            block = %block_id, kind = %kind_id, error = %resp.error,
                            "process on_message returned error",
                        );
                    }
                    for emit in resp.emitted {
                        let Ok(out_msg) = serde_json::from_str::<Msg>(&emit.msg_json) else {
                            tracing::warn!(
                                block = %block_id, kind = %kind_id, port = %emit.port,
                                "could not decode emit.msg_json from process block",
                            );
                            continue;
                        };
                        if let Err(e) = emit_sink.emit(node_id, &emit.port, out_msg) {
                            tracing::warn!(
                                block = %block_id, kind = %kind_id, port = %emit.port,
                                error = %e,
                                "emit back to engine failed",
                            );
                        }
                    }
                }
                Err(status) => {
                    tracing::warn!(
                        block = %block_id, kind = %kind_id, error = %status,
                        "process on_message RPC failed",
                    );
                }
            }
        });

        Ok(())
    }

    fn on_timer(&self, _ctx: &NodeCtx, _handle: TimerHandle) -> Result<(), NodeError> {
        // Timer scheduling across the wire isn't plumbed yet.
        Ok(())
    }

    fn on_shutdown(&self, _ctx: &NodeCtx) -> Result<(), NodeError> {
        // Shutdown RPC isn't plumbed yet; supervisor kills the process
        // directly when it cancels.
        Ok(())
    }
}
