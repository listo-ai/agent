//! `acme.logic.heartbeat` — self-driven boolean toggle with a counter.
//!
//! Source node: no inputs. On `on_init` it emits `start_state` and arms
//! a timer. Each timer fire flips `state` and increments `count`. A
//! stale-fire guard (same pattern as [`crate::trigger`]) ignores fires
//! whose handle no longer matches the active one — defends against the
//! cancel-vs-fire race when the user edits `interval_ms` while a timer
//! is pending.

use extensions_sdk::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

#[derive(NodeKind)]
#[node(
    kind = "acme.logic.heartbeat",
    manifest = "manifests/heartbeat.yaml",
    behavior = "custom"
)]
pub struct Heartbeat;

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatConfig {
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
    #[serde(default)]
    pub start_state: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_interval_ms() -> u64 {
    1000
}
fn default_enabled() -> bool {
    true
}

const STATE_SLOT: &str = "state";
const COUNT_SLOT: &str = "count";
const PENDING_TIMER_STATUS: &str = "pending_timer";

impl NodeBehavior for Heartbeat {
    type Config = HeartbeatConfig;

    fn on_init(&self, ctx: &NodeCtx, cfg: &HeartbeatConfig) -> Result<(), NodeError> {
        cancel_pending(ctx);
        ctx.emit(STATE_SLOT, Msg::new(json!(cfg.start_state)))?;
        ctx.emit(COUNT_SLOT, Msg::new(json!(0)))?;
        arm_if_enabled(ctx, cfg)
    }

    fn on_timer(&self, ctx: &NodeCtx, handle: TimerHandle) -> Result<(), NodeError> {
        // Stale-fire guard — an old timer may still race after an
        // interval change or a disable.
        let pending = ctx.read_status(PENDING_TIMER_STATUS).ok().and_then(|v| v.as_u64());
        if pending != Some(handle.0) {
            return Ok(());
        }

        let cfg = ctx.resolve_settings::<HeartbeatConfig>(&Msg::new(JsonValue::Null))?;
        if !cfg.enabled {
            ctx.update_status(PENDING_TIMER_STATUS, JsonValue::Null)?;
            return Ok(());
        }

        let current = ctx.read_status(STATE_SLOT).ok().and_then(|v| v.as_bool()).unwrap_or(cfg.start_state);
        let next = !current;
        ctx.emit(STATE_SLOT, Msg::new(json!(next)))?;

        let prior_count = ctx.read_status(COUNT_SLOT).ok().and_then(|v| v.as_u64()).unwrap_or(0);
        ctx.emit(COUNT_SLOT, Msg::new(json!(prior_count + 1)))?;

        arm_if_enabled(ctx, &cfg)
    }
}

fn arm_if_enabled(ctx: &NodeCtx, cfg: &HeartbeatConfig) -> Result<(), NodeError> {
    if !cfg.enabled {
        ctx.update_status(PENDING_TIMER_STATUS, JsonValue::Null)?;
        return Ok(());
    }
    let handle = ctx.schedule(cfg.interval_ms)?;
    ctx.update_status(PENDING_TIMER_STATUS, json!(handle.0))
}

fn cancel_pending(ctx: &NodeCtx) {
    if let Some(id) = ctx.read_status(PENDING_TIMER_STATUS).ok().and_then(|v| v.as_u64()) {
        ctx.cancel(TimerHandle(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg: HeartbeatConfig = serde_json::from_value(json!({})).unwrap();
        assert_eq!(cfg.interval_ms, 1000);
        assert!(!cfg.start_state);
        assert!(cfg.enabled);
    }

    #[test]
    fn config_accepts_partial_override() {
        let cfg: HeartbeatConfig =
            serde_json::from_value(json!({"interval_ms": 500})).unwrap();
        assert_eq!(cfg.interval_ms, 500);
        assert!(cfg.enabled);
    }
}
