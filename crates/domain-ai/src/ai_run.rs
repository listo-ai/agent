//! `sys.ai.run` — single-shot AI call as a flow node.
//!
//! Fires on any message arriving at the `trigger` input. Reads prompt /
//! provider / model from settings (with per-message overrides via
//! `msg_overrides`), dispatches to the shared [`ai_runner::Registry`]
//! (installed via [`crate::init`]), and emits on completion:
//!
//! - status `running`    — `true` while a request is in flight
//! - status `last_error` — null on success, error string on failure
//! - status `input_tokens` / `output_tokens` / `duration_ms`
//! - output `text`       — full response body (pulse)
//! - output `done`       — pulse payload `{ ok, text, tokens, duration_ms }`
//!
//! Because `Runner::run` is async and `NodeBehavior::on_message` is
//! synchronous, we clone the graph + emit-sink `Arc`s off the `NodeCtx`
//! and `tokio::spawn` the call. The finishing write goes through those
//! handles directly.

use std::sync::Arc;

use ai_runner::{Provider, RunConfig};
use blocks_sdk::prelude::*;
use blocks_sdk::{EmitSink, GraphAccess};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use spi::NodePath;
use tracing::warn;

use crate::runtime;

#[derive(NodeKind)]
#[node(
    kind = "sys.ai.run",
    manifest = "manifests/ai_run.yaml",
    behavior = "custom"
)]
pub struct AiRun;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AiRunConfig {
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

impl NodeBehavior for AiRun {
    type Config = AiRunConfig;

    fn on_init(&self, ctx: &NodeCtx, _cfg: &AiRunConfig) -> Result<(), NodeError> {
        ctx.update_status("running", json!(false))?;
        ctx.update_status("last_error", JsonValue::Null)?;
        ctx.update_status("input_tokens", json!(0))?;
        ctx.update_status("output_tokens", json!(0))?;
        ctx.update_status("duration_ms", json!(0))?;
        Ok(())
    }

    fn on_message(&self, ctx: &NodeCtx, _port: InputPort, msg: Msg) -> Result<(), NodeError> {
        let cfg = ctx.resolve_settings::<AiRunConfig>(&msg)?;
        if cfg.prompt.trim().is_empty() {
            return Err(NodeError::runtime(
                "sys.ai.run: `prompt` is empty (set in settings or pass via `msg.prompt`)",
            ));
        }

        let (registry, defaults) = match runtime::get() {
            Some(rt) => rt,
            None => {
                ctx.update_status(
                    "last_error",
                    json!("ai runtime not initialised (domain_ai::init was not called)"),
                )?;
                return Ok(());
            }
        };

        let provider = cfg
            .provider
            .clone()
            .or_else(|| defaults.provider.clone())
            .unwrap_or(Provider::Anthropic);

        let runner = match registry.get(&provider) {
            Some(r) => r,
            None => {
                ctx.update_status(
                    "last_error",
                    json!(format!("provider `{provider}` not registered")),
                )?;
                return Ok(());
            }
        };

        ctx.update_status("running", json!(true))?;
        ctx.update_status("last_error", JsonValue::Null)?;

        let run_cfg = RunConfig {
            prompt: cfg.prompt.clone(),
            system_prompt: cfg.system_prompt.clone(),
            model: cfg.model.clone().or_else(|| defaults.model.clone()),
            api_key: defaults.api_key_for(&provider),
            max_tokens: cfg.max_tokens,
            ..Default::default()
        };

        let graph = ctx.graph();
        let emit_sink = ctx.emit_sink();
        let node_id = ctx.node_id();
        let node_path = ctx.node_path().clone();
        let session_id = format!("ai-run-{}", node_id.0);
        let msg_for_task = msg;

        tokio::spawn(async move {
            let result = runner.run(run_cfg, session_id, Arc::new(|_ev| {})).await;
            finalize(
                graph.as_ref(),
                emit_sink.as_ref(),
                &node_path,
                node_id,
                &msg_for_task,
                result,
            );
        });

        Ok(())
    }
}

fn finalize(
    graph: &dyn GraphAccess,
    emit_sink: &dyn EmitSink,
    node_path: &NodePath,
    node_id: spi::NodeId,
    msg: &Msg,
    result: ai_runner::RunResult,
) {
    let status_writes: Vec<(&str, JsonValue)> = vec![
        ("running", json!(false)),
        (
            "last_error",
            result
                .error
                .as_ref()
                .map(|e| JsonValue::String(e.clone()))
                .unwrap_or(JsonValue::Null),
        ),
        ("input_tokens", json!(result.input_tokens)),
        ("output_tokens", json!(result.output_tokens)),
        ("duration_ms", json!(result.duration_ms)),
    ];
    for (slot, value) in status_writes {
        if let Err(e) = graph.write_slot(node_path, slot, value) {
            warn!(node = %node_id.0, slot, "sys.ai.run write_slot failed: {e}");
        }
    }

    if result.error.is_none() {
        let text_msg = msg.child(JsonValue::String(result.text.clone()));
        if let Err(e) = emit_sink.emit(node_id, "text", text_msg) {
            warn!(node = %node_id.0, "sys.ai.run emit text failed: {e}");
        }
    }

    let done_payload = json!({
        "ok": result.error.is_none(),
        "text": result.text,
        "error": result.error,
        "input_tokens": result.input_tokens,
        "output_tokens": result.output_tokens,
        "duration_ms": result.duration_ms,
        "provider": result.provider,
        "model": result.model,
    });
    if let Err(e) = emit_sink.emit(node_id, "done", msg.child(done_payload)) {
        warn!(node = %node_id.0, "sys.ai.run emit done failed: {e}");
    }
}
