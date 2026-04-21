//! Process-block supervisor.
//!
//! Per PLUGINS.md § "Process-block wire in detail":
//!   1. Spawn the binary with a fresh UDS path passed via env.
//!   2. Await the socket, open a gRPC client.
//!   3. Call `Describe` — block id must match the manifest's.
//!   4. Ping `Health` on a cadence; restart with backoff on failure.
//!
//! This module lands (1)–(3) plus manual [`ProcessSupervisor::health`]
//! polling. Cadence-based restart loops are deferred — they need a
//! decision on where the ticker lives (agent-owned or registry-owned)
//! which is out of scope for the first wiring pass.
//!
//! Non-Unix platforms get compile-time stubs so the crate still
//! builds; actual process blocks only run on Unix.
//!
//! # Environment contract with the block binary
//!
//! The supervisor sets **`US_PLUGIN_SOCKET`** to the UDS path. Block
//! authors read this in their `main` via
//! [`blocks_sdk::process::run_process_plugin`] (feature `process`)
//! and serve the `Extension` gRPC service on it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use transport_grpc::{
    DescribeRequest, DescribeResponse, ExtensionClient, HealthRequest, HealthResponse,
};

use crate::manifest::BlockId;

/// Env var the supervisor uses to pass the UDS path to the child.
pub const SOCKET_ENV: &str = "US_PLUGIN_SOCKET";

/// How long [`ProcessSupervisor::spawn`] waits for the child to create
/// its socket before giving up. Five seconds is generous for a Rust
/// binary but safe for a cold-start Python one.
const SOCKET_READY_TIMEOUT: Duration = Duration::from_secs(5);

/// Poll interval while waiting for the socket to appear.
const SOCKET_POLL: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("spawning block binary `{path}`: {source}")]
    Spawn {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("block `{block}` did not create socket `{socket}` within {timeout:?}")]
    SocketTimeout {
        block: String,
        socket: PathBuf,
        timeout: Duration,
    },
    #[error("connecting to block `{block}` over `{socket}`: {source}")]
    Connect {
        block: String,
        socket: PathBuf,
        #[source]
        source: tonic::transport::Error,
    },
    #[error("gRPC call `{rpc}` to block `{block}` failed: {source}")]
    Rpc {
        block: String,
        rpc: &'static str,
        #[source]
        source: tonic::Status,
    },
    #[error(
        "block `{expected}` identified itself as `{actual}` in Describe — manifest/identity mismatch"
    )]
    IdentityMismatch { expected: String, actual: String },
    #[error("process blocks are not supported on this platform")]
    UnsupportedPlatform,
}

/// A live, connected, health-checked process block.
pub struct ProcessSupervisor {
    block_id: BlockId,
    socket_path: PathBuf,
    /// `process-wrap`'s wrapped child. `None` when the supervisor was
    /// built from [`ProcessSupervisor::connect`] (we don't own the
    /// process in that case — tests, systemd-managed blocks).
    ///
    /// The wrapper gives us cross-platform kill-tree semantics:
    /// Unix process groups / Windows Job Objects, so restarting a
    /// block that itself spawned workers doesn't leak zombies.
    child: Option<Box<dyn process_wrap::tokio::TokioChildWrapper>>,
    client: ExtensionClient<tonic::transport::Channel>,
    identity: DescribeResponse,
}

impl std::fmt::Debug for ProcessSupervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessSupervisor")
            .field("block_id", &self.block_id)
            .field("socket_path", &self.socket_path)
            .field("owns_child", &self.child.is_some())
            .field("identity", &self.identity)
            .finish()
    }
}

impl ProcessSupervisor {
    /// Spawn the block binary, wait for its UDS, connect, and verify
    /// identity via `Describe`.
    ///
    /// `socket_dir` must exist and be writable. Callers typically pass
    /// a per-agent runtime dir (e.g. `/run/<app>/blocks/`).
    #[cfg(unix)]
    pub async fn spawn(
        block_id: &BlockId,
        bin_path: &Path,
        args: &[String],
        socket_dir: &Path,
    ) -> Result<Self, SupervisorError> {
        use process_wrap::tokio::{KillOnDrop, TokioCommandWrap};

        let socket_path = socket_dir.join(format!("{}.sock", block_id.as_str()));
        // Stale socket from a crashed predecessor — remove; the child
        // will bind fresh. Ignore errors (may simply not exist).
        let _ = std::fs::remove_file(&socket_path);

        let mut wrap = TokioCommandWrap::with_new(bin_path, |c| {
            c.args(args).env(SOCKET_ENV, &socket_path);
        });
        // Cross-platform "kill the whole subtree on shutdown/restart":
        // Unix → process group leader (setsid), Windows → Job Object.
        #[cfg(unix)]
        {
            wrap.wrap(process_wrap::tokio::ProcessGroup::leader());
        }
        #[cfg(windows)]
        {
            wrap.wrap(process_wrap::tokio::JobObject);
        }
        wrap.wrap(KillOnDrop);

        let child = wrap.spawn().map_err(|e| SupervisorError::Spawn {
            path: bin_path.to_path_buf(),
            source: e,
        })?;

        wait_for_socket(&socket_path, block_id.as_str()).await?;

        let mut sup = Self::connect_inner(block_id, &socket_path, Some(child)).await?;
        sup.verify_identity().await?;
        Ok(sup)
    }

    /// Connect to a block that is already running (e.g. supervised by
    /// systemd or launched for a test). Does **not** own the process.
    #[cfg(unix)]
    pub async fn connect(block_id: &BlockId, socket_path: &Path) -> Result<Self, SupervisorError> {
        let mut sup = Self::connect_inner(block_id, socket_path, None).await?;
        sup.verify_identity().await?;
        Ok(sup)
    }

    #[cfg(unix)]
    async fn connect_inner(
        block_id: &BlockId,
        socket_path: &Path,
        child: Option<Box<dyn process_wrap::tokio::TokioChildWrapper>>,
    ) -> Result<Self, SupervisorError> {
        use tokio::net::UnixStream;
        use tonic::transport::{Endpoint, Uri};

        let socket_path_buf = socket_path.to_path_buf();
        // The URI here is a placeholder — tonic requires one, but the
        // actual transport is the UDS connector below.
        let endpoint = Endpoint::try_from("http://[::]:50051").expect("static uri");
        let sp_for_connector = socket_path_buf.clone();
        let channel = endpoint
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                let p = sp_for_connector.clone();
                async move {
                    let stream = UnixStream::connect(p).await?;
                    Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
                }
            }))
            .await
            .map_err(|e| SupervisorError::Connect {
                block: block_id.as_str().to_string(),
                socket: socket_path_buf.clone(),
                source: e,
            })?;

        let client = ExtensionClient::new(channel);

        Ok(Self {
            block_id: block_id.clone(),
            socket_path: socket_path_buf,
            child,
            client,
            // Filled in by verify_identity().
            identity: DescribeResponse::default(),
        })
    }

    #[cfg(not(unix))]
    pub async fn spawn(
        _block_id: &BlockId,
        _bin_path: &Path,
        _args: &[String],
        _socket_dir: &Path,
    ) -> Result<Self, SupervisorError> {
        Err(SupervisorError::UnsupportedPlatform)
    }

    #[cfg(not(unix))]
    pub async fn connect(
        _block_id: &BlockId,
        _socket_path: &Path,
    ) -> Result<Self, SupervisorError> {
        // Named-pipe transport parity with the UDS path is a deferred
        // Windows story; the gRPC contract itself is unchanged.
        Err(SupervisorError::UnsupportedPlatform)
    }

    async fn verify_identity(&mut self) -> Result<(), SupervisorError> {
        let resp = self
            .client
            .describe(DescribeRequest {})
            .await
            .map_err(|e| SupervisorError::Rpc {
                block: self.block_id.as_str().to_string(),
                rpc: "Describe",
                source: e,
            })?
            .into_inner();

        if resp.extension_id != self.block_id.as_str() {
            return Err(SupervisorError::IdentityMismatch {
                expected: self.block_id.as_str().to_string(),
                actual: resp.extension_id,
            });
        }
        self.identity = resp;
        Ok(())
    }

    pub fn block_id(&self) -> &BlockId {
        &self.block_id
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn identity(&self) -> &DescribeResponse {
        &self.identity
    }

    /// Clone of the live `ExtensionClient`. Tonic clients wrap an `Arc`
    /// internally so cloning is cheap. The proxy behaviour holds a
    /// clone for the lifetime of this supervisor's current child; on
    /// restart the host clears and re-seeds the shared slot.
    pub fn client(&self) -> ExtensionClient<tonic::transport::Channel> {
        self.client.clone()
    }

    /// Ping the block's `Health` RPC. Call from a supervisor tick.
    pub async fn health(&mut self) -> Result<HealthResponse, SupervisorError> {
        let resp = self
            .client
            .health(HealthRequest {})
            .await
            .map_err(|e| SupervisorError::Rpc {
                block: self.block_id.as_str().to_string(),
                rpc: "Health",
                source: e,
            })?
            .into_inner();
        Ok(resp)
    }

    /// Drop the client and kill the child tree (if owned).
    ///
    /// Kills the whole process group / Job Object — blocks that
    /// spawned their own workers go down with the parent. Best-effort:
    /// if `kill` / `wait` fail we still move on and unlink the socket.
    pub async fn shutdown(mut self) {
        if let Some(mut child) = self.child.take() {
            // `Box<dyn TokioChildWrapper>::kill` is async and targets
            // the whole wrapped unit (group on Unix, job on Windows).
            let _ = Box::into_pin(child.kill()).await;
            let _ = Box::into_pin(child.wait()).await;
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }

    /// Has the owned child exited? `None` if we don't own a process
    /// (built via [`connect`](Self::connect)) or if it's still running.
    pub fn try_exit_status(&mut self) -> Option<std::process::ExitStatus> {
        let child = self.child.as_mut()?;
        child.try_wait().ok().flatten()
    }
}

#[cfg(unix)]
async fn wait_for_socket(path: &Path, block: &str) -> Result<(), SupervisorError> {
    let deadline = tokio::time::Instant::now() + SOCKET_READY_TIMEOUT;
    loop {
        if path.exists() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(SupervisorError::SocketTimeout {
                block: block.to_string(),
                socket: path.to_path_buf(),
                timeout: SOCKET_READY_TIMEOUT,
            });
        }
        tokio::time::sleep(SOCKET_POLL).await;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tokio::net::UnixListener;
    use tokio_stream::wrappers::UnixListenerStream;
    use tonic::{transport::Server, Request, Response, Status};
    use transport_grpc::proto::health_response::Status as HStatus;
    use transport_grpc::{
        DiscoverEvent, DiscoverRequest, Extension, ExtensionServer, InvokeRequest, InvokeResponse,
        SlotEvent, SubscribeRequest,
    };

    struct FakePlugin {
        id: String,
    }

    #[tonic::async_trait]
    impl Extension for FakePlugin {
        async fn describe(
            &self,
            _req: Request<DescribeRequest>,
        ) -> Result<Response<DescribeResponse>, Status> {
            Ok(Response::new(DescribeResponse {
                extension_id: self.id.clone(),
                version: "0.1.0".into(),
                kinds: vec![],
                capabilities: vec![],
            }))
        }
        type DiscoverStream = std::pin::Pin<
            Box<dyn futures_core::Stream<Item = Result<DiscoverEvent, Status>> + Send>,
        >;
        async fn discover(
            &self,
            _req: Request<DiscoverRequest>,
        ) -> Result<Response<Self::DiscoverStream>, Status> {
            Err(Status::unimplemented("discover"))
        }
        type SubscribeStream =
            std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<SlotEvent, Status>> + Send>>;
        async fn subscribe(
            &self,
            _req: Request<SubscribeRequest>,
        ) -> Result<Response<Self::SubscribeStream>, Status> {
            Err(Status::unimplemented("subscribe"))
        }
        async fn invoke(
            &self,
            _req: Request<InvokeRequest>,
        ) -> Result<Response<InvokeResponse>, Status> {
            Err(Status::unimplemented("invoke"))
        }
        async fn health(
            &self,
            _req: Request<HealthRequest>,
        ) -> Result<Response<HealthResponse>, Status> {
            Ok(Response::new(HealthResponse {
                status: HStatus::Ready as i32,
                detail: "ok".into(),
            }))
        }
    }

    async fn spawn_fake(id: &str, socket: PathBuf) {
        let listener = UnixListener::bind(&socket).expect("bind uds");
        let stream = UnixListenerStream::new(listener);
        let svc = ExtensionServer::new(FakePlugin { id: id.to_string() });
        tokio::spawn(async move {
            let _ = Server::builder()
                .add_service(svc)
                .serve_with_incoming(stream)
                .await;
        });
    }

    #[tokio::test]
    async fn connect_describe_and_health_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("good.sock");
        spawn_fake("com.acme.hello", socket.clone()).await;
        // Give the listener a tick to bind.
        tokio::time::sleep(Duration::from_millis(20)).await;

        let pid = BlockId::parse("com.acme.hello").unwrap();
        let mut sup = ProcessSupervisor::connect(&pid, &socket).await.unwrap();
        assert_eq!(sup.identity().extension_id, "com.acme.hello");
        assert_eq!(sup.identity().version, "0.1.0");

        let h = sup.health().await.unwrap();
        assert_eq!(h.status, HStatus::Ready as i32);
    }

    #[tokio::test]
    async fn identity_mismatch_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("bad.sock");
        spawn_fake("com.other.block", socket.clone()).await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        let pid = BlockId::parse("com.acme.hello").unwrap();
        let err = ProcessSupervisor::connect(&pid, &socket).await.unwrap_err();
        assert!(matches!(err, SupervisorError::IdentityMismatch { .. }));
    }

    #[tokio::test]
    async fn missing_socket_times_out_on_connect() {
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("nope.sock");
        let pid = BlockId::parse("com.acme.hello").unwrap();
        let err = ProcessSupervisor::connect(&pid, &socket).await.unwrap_err();
        assert!(matches!(err, SupervisorError::Connect { .. }));
    }
}
