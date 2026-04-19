//! Safe-state policy for writable outputs.
//!
//! Per `docs/design/RUNTIME.md` § "Safe-state handling":
//!
//! > Every writable output declares its safe-state policy. The engine
//! > enforces it on stop, on crash, on disconnect.
//!
//! The policy enum is the stable contract. The actual driving logic
//! for a given output lives behind the [`OutputDriver`] trait \u{2014}
//! implemented by protocol extensions (BACnet priority array, MQTT
//! retained-null, HTTP PATCH with last-known-good, etc.).
//!
//! Stage 2 ships the types and a [`NoopOutputDriver`]; real driver
//! wiring lands with the BACnet extension in Stage 12 and the
//! commissioning-mode work in Stage 13.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::NodePath;

/// What the engine does to a writable output when the owning flow
/// stops, pauses, or disconnects.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum SafeStatePolicy {
    /// Leave the output at its last-commanded value.
    #[default]
    Hold,
    /// Drive the output to an explicit fail-safe value.
    FailSafe { value: JsonValue },
    /// Relinquish the output; downstream determines its own default
    /// (e.g. BACnet priority-array release to a lower priority).
    Release,
}

/// Driver responsible for applying a [`SafeStatePolicy`] to one output
/// when the engine transitions to `Stopping`. Async because most
/// real drivers talk to a protocol or an external API.
#[async_trait]
pub trait OutputDriver: Send + Sync + 'static {
    async fn apply(
        &self,
        path: &NodePath,
        slot: &str,
        policy: &SafeStatePolicy,
    ) -> Result<(), SafeStateError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SafeStateError {
    #[error("safe-state driver: {0}")]
    Driver(String),
}

/// Stand-in driver that logs and does nothing. Used until a real
/// protocol driver is wired to a given output. Keeps `Stopping` from
/// being a hard failure on a fresh install.
pub struct NoopOutputDriver;

#[async_trait]
impl OutputDriver for NoopOutputDriver {
    async fn apply(
        &self,
        path: &NodePath,
        slot: &str,
        policy: &SafeStatePolicy,
    ) -> Result<(), SafeStateError> {
        tracing::info!(
            %path, slot, ?policy,
            "safe-state: no driver bound; skipping",
        );
        Ok(())
    }
}

/// Pair of (target, policy) the engine iterates through on
/// `Stopping`. Registered by the owners of writable outputs; the
/// registry itself lives on the [`Engine`](crate::Engine).
#[derive(Clone)]
pub struct SafeStateBinding {
    pub path: NodePath,
    pub slot: String,
    pub policy: SafeStatePolicy,
    pub driver: Arc<dyn OutputDriver>,
}

impl std::fmt::Debug for SafeStateBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SafeStateBinding")
            .field("path", &self.path)
            .field("slot", &self.slot)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}
