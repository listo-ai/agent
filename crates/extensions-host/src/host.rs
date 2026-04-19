//! Process-plugin runtime manager.
//!
//! [`ProcessSupervisor`](crate::supervisor::ProcessSupervisor) is the
//! per-plugin primitive. [`PluginHost`] is the fleet-level conductor:
//! it iterates the [`PluginRegistry`], spawns a supervisor per plugin
//! that declares a `process_bin`, runs a per-plugin tokio task that
//! watches both `Health` and child-exit, and restarts with
//! exponential backoff. A crash-looping plugin is eventually marked
//! `Failed` and left alone (circuit-breaker).
//!
//! Lives in the agent binary's lifetime — spawn once after
//! [`PluginRegistry::scan`], `shutdown().await` on SIGTERM.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::manifest::{PluginId, ProcessBinContribution};
use crate::registry::{PluginLifecycle, PluginRegistry};
use crate::supervisor::{ProcessSupervisor, SupervisorError};

/// Per-plugin runtime state. Updated by the supervisor task; read by
/// REST / status handlers via [`PluginHost::state`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum PluginRuntimeState {
    /// Not running yet — task hasn't started or plugin is disabled.
    Idle,
    /// Spawning child / connecting gRPC / waiting for `Describe`.
    Starting,
    /// `Describe` succeeded and the plugin most recently reported
    /// `READY` on `Health`.
    Ready,
    /// Last `Health` returned `DEGRADED` or a non-fatal error.
    Degraded { detail: String },
    /// Crashed / health failed — waiting to restart.
    Restarting {
        attempt: u32,
        backoff_ms: u64,
        reason: String,
    },
    /// Circuit-broken. Too many failures in the window; no more
    /// auto-restarts. Operator must re-enable via REST.
    Failed { reason: String },
    /// Shut down cleanly.
    Stopped,
}

impl Default for PluginRuntimeState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Policy knobs. Sensible defaults; tune per deployment if needed.
#[derive(Debug, Clone)]
pub struct HostPolicy {
    /// How often to poll `Health` on a running plugin.
    pub health_interval: Duration,
    /// First restart delay.
    pub backoff_initial: Duration,
    /// Cap per-restart delay.
    pub backoff_max: Duration,
    /// Circuit-breaker: N fast failures within `failure_window`
    /// promotes the plugin to `Failed` and stops the auto-restart loop.
    pub failure_threshold: u32,
    pub failure_window: Duration,
}

impl Default for HostPolicy {
    fn default() -> Self {
        Self {
            health_interval: Duration::from_secs(10),
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            failure_threshold: 5,
            failure_window: Duration::from_secs(60),
        }
    }
}

struct Supervised {
    /// Current reported state. Cloneable out for REST.
    state: Arc<RwLock<PluginRuntimeState>>,
    /// Cancel the per-plugin task (disable / shutdown).
    cancel: CancellationToken,
    /// The driving task. `await` it on shutdown.
    handle: JoinHandle<()>,
}

/// Fleet-level conductor for process plugins.
///
/// Clone to share across axum handlers — internals are `Arc`-backed.
#[derive(Clone)]
pub struct PluginHost {
    inner: Arc<HostInner>,
}

struct HostInner {
    registry: PluginRegistry,
    socket_dir: PathBuf,
    policy: HostPolicy,
    supervised: Mutex<HashMap<PluginId, Supervised>>,
}

impl PluginHost {
    /// Build a host and spawn a supervisor task for every plugin in
    /// the registry that declares a `process_bin` and is currently
    /// `Enabled`.
    ///
    /// `socket_dir` must be writable — supervisors drop their UDS
    /// paths inside it. Callers typically pass `/run/<app>/plugins/`
    /// or `<state-dir>/plugins/sockets/`.
    pub async fn start(
        registry: PluginRegistry,
        socket_dir: PathBuf,
        policy: HostPolicy,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&socket_dir)?;
        let host = Self {
            inner: Arc::new(HostInner {
                registry,
                socket_dir,
                policy,
                supervised: Mutex::new(HashMap::new()),
            }),
        };
        host.reconcile().await;
        Ok(host)
    }

    /// Read current runtime state for one plugin.
    pub async fn state(&self, id: &PluginId) -> Option<PluginRuntimeState> {
        let sup = self.inner.supervised.lock().await;
        match sup.get(id) {
            Some(s) => Some(s.state.read().await.clone()),
            None => None,
        }
    }

    /// Snapshot all runtime states.
    pub async fn states(&self) -> Vec<(PluginId, PluginRuntimeState)> {
        let sup = self.inner.supervised.lock().await;
        let mut out = Vec::with_capacity(sup.len());
        for (id, s) in sup.iter() {
            out.push((id.clone(), s.state.read().await.clone()));
        }
        out.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        out
    }

    /// Enable a plugin: update the registry and, if it has a
    /// `process_bin`, spawn its supervisor task.
    pub async fn enable(&self, id: &PluginId) -> Result<(), HostError> {
        self.inner
            .registry
            .set_enabled(id, true)
            .map_err(|e| HostError::Registry(e.to_string()))?;
        self.ensure_running(id).await;
        Ok(())
    }

    /// Disable a plugin: tear down its supervisor if running, then
    /// mark it Disabled in the registry.
    pub async fn disable(&self, id: &PluginId) -> Result<(), HostError> {
        self.stop_one(id).await;
        self.inner
            .registry
            .set_enabled(id, false)
            .map_err(|e| HostError::Registry(e.to_string()))?;
        Ok(())
    }

    /// Orderly shutdown: cancel every supervisor task, wait for each
    /// to drain. Call this on SIGTERM before the process exits.
    pub async fn shutdown(&self) {
        let mut sup = self.inner.supervised.lock().await;
        let entries: Vec<_> = sup.drain().collect();
        drop(sup);
        for (_id, s) in entries {
            s.cancel.cancel();
            let _ = s.handle.await;
        }
    }

    /// Bring running supervisors into line with the registry's current
    /// Enabled set. Called once at start and after any `enable`/
    /// `disable` that changes membership.
    async fn reconcile(&self) {
        let want_running: Vec<(PluginId, ProcessBinContribution)> = self
            .inner
            .registry
            .list()
            .into_iter()
            .filter(|s| matches!(s.lifecycle, PluginLifecycle::Enabled))
            .filter_map(|s| {
                // Summary doesn't carry the process_bin; look it up on
                // the full manifest via the registry's detail path.
                self.inner
                    .registry
                    .process_bin(&s.id)
                    .map(|bin| (s.id, bin))
            })
            .collect();

        for (id, _) in &want_running {
            self.ensure_running(id).await;
        }
    }

    async fn ensure_running(&self, id: &PluginId) {
        let mut sup = self.inner.supervised.lock().await;
        if sup.contains_key(id) {
            return;
        }
        let Some(bin) = self.inner.registry.process_bin(id) else {
            return;
        };
        // Resolve the binary path relative to the plugin root.
        let Some(plugin_root) = self.inner.registry.plugin_root(id) else {
            return;
        };
        let bin_path = plugin_root.join(&bin.path);

        let state = Arc::new(RwLock::new(PluginRuntimeState::Idle));
        let cancel = CancellationToken::new();

        let task_state = state.clone();
        let task_cancel = cancel.clone();
        let task_id = id.clone();
        let socket_dir = self.inner.socket_dir.clone();
        let policy = self.inner.policy.clone();
        let args = bin.args.clone();

        let handle = tokio::spawn(supervise_loop(
            task_id,
            bin_path,
            args,
            socket_dir,
            policy,
            task_state,
            task_cancel,
        ));
        sup.insert(
            id.clone(),
            Supervised {
                state,
                cancel,
                handle,
            },
        );
    }

    async fn stop_one(&self, id: &PluginId) {
        let mut sup = self.inner.supervised.lock().await;
        let Some(s) = sup.remove(id) else {
            return;
        };
        drop(sup);
        s.cancel.cancel();
        let _ = s.handle.await;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("registry: {0}")]
    Registry(String),
}

/// The per-plugin supervisor task. Runs until `cancel` fires.
///
/// One iteration = one lifetime of the child process:
///   1. Spawn + Describe + publish Ready.
///   2. Tick Health on `policy.health_interval`; also watch the child
///      for exit. On either failure, break to (3).
///   3. Shutdown, consult circuit-breaker, publish Restarting or
///      Failed, sleep backoff, next iteration.
async fn supervise_loop(
    id: PluginId,
    bin_path: PathBuf,
    args: Vec<String>,
    socket_dir: PathBuf,
    policy: HostPolicy,
    state: Arc<RwLock<PluginRuntimeState>>,
    cancel: CancellationToken,
) {
    let mut attempt: u32 = 0;
    let mut failure_times: Vec<Instant> = Vec::new();

    loop {
        if cancel.is_cancelled() {
            break;
        }

        set_state(&state, PluginRuntimeState::Starting).await;

        let mut sup = match spawn_with_cancel(&id, &bin_path, &args, &socket_dir, &cancel).await {
            SpawnOutcome::Ok(s) => s,
            SpawnOutcome::Cancelled => break,
            SpawnOutcome::Failed(e) => {
                record_failure(&mut failure_times, policy.failure_window);
                if failure_times.len() as u32 >= policy.failure_threshold {
                    tracing::error!(
                        plugin = %id, error = %e,
                        "plugin failed to start {} times in window — circuit open",
                        policy.failure_threshold
                    );
                    set_state(
                        &state,
                        PluginRuntimeState::Failed {
                            reason: e.to_string(),
                        },
                    )
                    .await;
                    break;
                }
                attempt += 1;
                let backoff = backoff_for(attempt, &policy);
                tracing::warn!(
                    plugin = %id, attempt, backoff_ms = backoff.as_millis() as u64,
                    error = %e, "plugin spawn failed; will retry"
                );
                set_state(
                    &state,
                    PluginRuntimeState::Restarting {
                        attempt,
                        backoff_ms: backoff.as_millis() as u64,
                        reason: e.to_string(),
                    },
                )
                .await;
                if sleep_or_cancel(backoff, &cancel).await {
                    break;
                }
                continue;
            }
        };

        set_state(&state, PluginRuntimeState::Ready).await;
        attempt = 0; // successful start resets attempt counter

        // Drive the plugin until it fails or we're cancelled.
        let reason = run_until_failure(&mut sup, &policy, &state, &cancel).await;
        sup.shutdown().await;

        match reason {
            RunReason::Cancelled => {
                set_state(&state, PluginRuntimeState::Stopped).await;
                break;
            }
            RunReason::Failure(msg) => {
                record_failure(&mut failure_times, policy.failure_window);
                if failure_times.len() as u32 >= policy.failure_threshold {
                    tracing::error!(
                        plugin = %id,
                        "plugin crashed {} times in window — circuit open",
                        policy.failure_threshold
                    );
                    set_state(&state, PluginRuntimeState::Failed { reason: msg }).await;
                    break;
                }
                attempt += 1;
                let backoff = backoff_for(attempt, &policy);
                tracing::warn!(
                    plugin = %id, attempt, backoff_ms = backoff.as_millis() as u64, reason = %msg,
                    "plugin down; restarting after backoff"
                );
                set_state(
                    &state,
                    PluginRuntimeState::Restarting {
                        attempt,
                        backoff_ms: backoff.as_millis() as u64,
                        reason: msg,
                    },
                )
                .await;
                if sleep_or_cancel(backoff, &cancel).await {
                    break;
                }
            }
        }
    }
}

enum SpawnOutcome {
    Ok(ProcessSupervisor),
    Cancelled,
    Failed(SupervisorError),
}

async fn spawn_with_cancel(
    id: &PluginId,
    bin: &Path,
    args: &[String],
    socket_dir: &Path,
    cancel: &CancellationToken,
) -> SpawnOutcome {
    tokio::select! {
        _ = cancel.cancelled() => SpawnOutcome::Cancelled,
        r = ProcessSupervisor::spawn(id, bin, args, socket_dir) => match r {
            Ok(s) => SpawnOutcome::Ok(s),
            Err(e) => SpawnOutcome::Failed(e),
        },
    }
}

enum RunReason {
    Cancelled,
    Failure(String),
}

async fn run_until_failure(
    sup: &mut ProcessSupervisor,
    policy: &HostPolicy,
    state: &Arc<RwLock<PluginRuntimeState>>,
    cancel: &CancellationToken,
) -> RunReason {
    let mut ticker = tokio::time::interval(policy.health_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Burn the immediate first tick — we just started.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return RunReason::Cancelled,
            _ = ticker.tick() => {
                if let Some(status) = sup.try_exit_status() {
                    return RunReason::Failure(format!("child exited early: {status}"));
                }
                match sup.health().await {
                    Ok(h) => {
                        use transport_grpc::proto::health_response::Status as HS;
                        if h.status == HS::Degraded as i32 {
                            set_state(state, PluginRuntimeState::Degraded { detail: h.detail }).await;
                        } else if h.status == HS::Stopping as i32 {
                            return RunReason::Failure("plugin reported STOPPING".into());
                        } else {
                            set_state(state, PluginRuntimeState::Ready).await;
                        }
                    }
                    Err(e) => return RunReason::Failure(e.to_string()),
                }
            }
        }
    }
}

async fn set_state(slot: &Arc<RwLock<PluginRuntimeState>>, new: PluginRuntimeState) {
    *slot.write().await = new;
}

fn backoff_for(attempt: u32, policy: &HostPolicy) -> Duration {
    // Simple exponential — saturating shift, cap to max.
    let shift = attempt.saturating_sub(1).min(16);
    let base = policy.backoff_initial.saturating_mul(1u32 << shift);
    base.min(policy.backoff_max)
}

fn record_failure(stamps: &mut Vec<Instant>, window: Duration) {
    let now = Instant::now();
    stamps.push(now);
    stamps.retain(|t| now.duration_since(*t) <= window);
}

async fn sleep_or_cancel(d: Duration, cancel: &CancellationToken) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(d) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_saturates_to_max() {
        let p = HostPolicy::default();
        assert_eq!(backoff_for(1, &p), Duration::from_secs(1));
        assert_eq!(backoff_for(2, &p), Duration::from_secs(2));
        assert_eq!(backoff_for(3, &p), Duration::from_secs(4));
        // Eventually caps at 60.
        assert_eq!(backoff_for(20, &p), Duration::from_secs(60));
    }

    #[test]
    fn failure_window_evicts_old_stamps() {
        let mut v: Vec<Instant> = Vec::new();
        record_failure(&mut v, Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(5));
        record_failure(&mut v, Duration::from_millis(1));
        assert_eq!(v.len(), 1, "old stamp should have been evicted");
    }
}
