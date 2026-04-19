//! The `Engine` struct \u{2014} the long-lived service.
//!
//! Owns the live-wire worker task, the state machine, the safe-state
//! registry. One per agent. Holds an [`Arc<GraphStore>`] so every
//! graph mutation (including those produced by propagation) goes
//! through the same store used by CRUD.
//!
//! `docs/design/RUNTIME.md` is the canonical spec for shutdown order:
//! stop accepting work \u{2192} drain in-flight \u{2192} drive outputs to safe
//! state \u{2192} flush outbox (Stage 7) \u{2192} exit. Only the first three land
//! in Stage 2.

use std::sync::{Arc, Mutex};

use graph::{GraphEvent, GraphStore};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::EngineError;
use crate::live_wire::LiveWireExecutor;
use crate::safe_state::SafeStateBinding;
use crate::state::EngineState;

pub struct Engine {
    graph: Arc<GraphStore>,
    state: Arc<Mutex<EngineState>>,
    inner: Mutex<EngineInner>,
    safe_state: Mutex<Vec<SafeStateBinding>>,
}

#[derive(Default)]
struct EngineInner {
    events: Option<mpsc::UnboundedReceiver<GraphEvent>>,
    worker: Option<JoinHandle<()>>,
    control: Option<mpsc::UnboundedSender<Control>>,
}

enum Control {
    Shutdown,
}

impl Engine {
    /// Build an engine around an existing graph store and the event
    /// receiver paired with the sink you passed to `GraphStore::new`.
    pub fn new(graph: Arc<GraphStore>, events: mpsc::UnboundedReceiver<GraphEvent>) -> Arc<Self> {
        Arc::new(Self {
            graph,
            state: Arc::new(Mutex::new(EngineState::Stopped)),
            inner: Mutex::new(EngineInner {
                events: Some(events),
                ..EngineInner::default()
            }),
            safe_state: Mutex::new(Vec::new()),
        })
    }

    pub fn graph(&self) -> &Arc<GraphStore> {
        &self.graph
    }

    pub fn state(&self) -> EngineState {
        *self.state.lock().expect("engine state lock poisoned")
    }

    pub fn register_safe_state(&self, binding: SafeStateBinding) {
        self.safe_state
            .lock()
            .expect("safe-state lock poisoned")
            .push(binding);
    }

    pub async fn start(&self) -> Result<(), EngineError> {
        self.transition(EngineState::Starting)?;
        let events = {
            let mut inner = self.inner.lock().expect("engine inner lock poisoned");
            inner.events.take().ok_or(EngineError::AlreadyStarted)?
        };
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let exec = LiveWireExecutor::new(self.graph.clone());
        let state = self.state.clone();
        let worker = tokio::spawn(worker_loop(events, control_rx, state, exec));
        {
            let mut inner = self.inner.lock().expect("engine inner lock poisoned");
            inner.worker = Some(worker);
            inner.control = Some(control_tx);
        }
        self.transition(EngineState::Running)?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<(), EngineError> {
        self.transition(EngineState::Pausing)?;
        self.transition(EngineState::Paused)
    }

    pub async fn resume(&self) -> Result<(), EngineError> {
        self.transition(EngineState::Resuming)?;
        self.transition(EngineState::Running)
    }

    /// Graceful shutdown: transition to `Stopping`, apply every
    /// registered safe-state binding, close the control channel, join
    /// the worker, transition to `Stopped`. Idempotent \u{2014} calling on a
    /// stopped engine is a no-op.
    pub async fn shutdown(&self) -> Result<(), EngineError> {
        if self.state() == EngineState::Stopped {
            return Ok(());
        }
        self.transition(EngineState::Stopping)?;
        self.apply_safe_state().await;
        let (control, worker) = {
            let mut inner = self.inner.lock().expect("engine inner lock poisoned");
            (inner.control.take(), inner.worker.take())
        };
        if let Some(tx) = control {
            let _ = tx.send(Control::Shutdown);
            drop(tx);
        }
        if let Some(handle) = worker {
            handle.await.map_err(|_| EngineError::WorkerPanicked)?;
        }
        self.transition(EngineState::Stopped)
    }

    fn transition(&self, to: EngineState) -> Result<(), EngineError> {
        let mut s = self.state.lock().expect("engine state lock poisoned");
        let from = *s;
        if !from.can_transition_to(to) {
            return Err(EngineError::IllegalTransition { from, to });
        }
        tracing::info!(from = ?from, to = ?to, "engine state transition");
        *s = to;
        Ok(())
    }

    async fn apply_safe_state(&self) {
        let bindings = {
            let g = self.safe_state.lock().expect("safe-state lock poisoned");
            g.clone()
        };
        for b in bindings {
            if let Err(err) = b.driver.apply(&b.path, &b.slot, &b.policy).await {
                tracing::warn!(
                    path = %b.path, slot = %b.slot, error = %err,
                    "safe-state apply failed \u{2014} continuing shutdown",
                );
            }
        }
    }
}

async fn worker_loop(
    mut events: mpsc::UnboundedReceiver<GraphEvent>,
    mut control: mpsc::UnboundedReceiver<Control>,
    state: Arc<Mutex<EngineState>>,
    exec: LiveWireExecutor,
) {
    loop {
        tokio::select! {
            biased;
            ctrl = control.recv() => {
                match ctrl {
                    Some(Control::Shutdown) | None => {
                        tracing::debug!("worker received shutdown");
                        break;
                    }
                }
            }
            event = events.recv() => {
                let Some(event) = event else { break };
                let propagating = state
                    .lock()
                    .map(|g| g.propagates())
                    .unwrap_or(false);
                if propagating {
                    exec.handle(&event);
                } else {
                    tracing::trace!("event arrived in non-running state \u{2014} skipping");
                }
            }
        }
    }
    events.close();
    while events.recv().await.is_some() {
        // drain remaining; we're done propagating
    }
    tracing::debug!("worker exited");
}
