//! `sys.logic.function` — the kind and its `NodeBehavior`.
//!
//! Wires convert/cache/helpers/error together into the on_message
//! path. Keep this file focused on *orchestration*: everything
//! computationally interesting happens in a sibling module.

use blocks_sdk::prelude::*;
use rhai::{Engine, Scope};
use serde::Deserialize;
use serde_json::json;

use crate::cache;
use crate::convert::{msg_to_rhai, rhai_to_msg_batch};
use crate::error::{self, FunctionError};
use crate::helpers;

const STATUS_LAST_ERROR: &str = "last_error";
const STATUS_EXEC_COUNT: &str = "exec_count";
const STATUS_ERROR_COUNT: &str = "error_count";
const PORT_OUT: &str = "out";
const PORT_ERR: &str = "err";

#[derive(NodeKind)]
#[node(
    kind = "sys.logic.function",
    manifest = "manifests/function.yaml",
    behavior = "custom"
)]
pub struct Function;

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionConfig {
    #[serde(default = "default_script")]
    pub script: String,
    #[serde(default = "default_max_ops")]
    pub max_operations: u64,
    #[serde(default)]
    pub on_error: OnError,
}

fn default_script() -> String {
    "msg".to_string()
}
fn default_max_ops() -> u64 {
    100_000
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    /// Script error → log + error_count++, nothing emitted.
    Drop,
    /// Script error → log + emit `{ error, source_msg, stage, line? }`
    /// on the `err` output port. Node-RED catch-node analogue.
    #[default]
    EmitErr,
    /// Script error → return `NodeError::runtime` so the engine's fault
    /// surfaces (visible to dispatch metrics and fault-handling flows).
    Throw,
}

impl NodeBehavior for Function {
    type Config = FunctionConfig;

    fn on_init(&self, _ctx: &NodeCtx, _cfg: &Self::Config) -> Result<(), NodeError> {
        // Compilation is lazy on first on_message so a broken script
        // doesn't prevent the node from coming up — operators can
        // load a flow, see the `err` port wired to a logging node,
        // and fix the script interactively.
        Ok(())
    }

    fn on_config_change(&self, ctx: &NodeCtx, _cfg: &Self::Config) -> Result<(), NodeError> {
        // Hash mismatch on next dispatch will force a recompile; we
        // just drop the stale entry eagerly so memory is correct.
        cache::evict(ctx.node_id());
        Ok(())
    }

    fn on_shutdown(&self, ctx: &NodeCtx) -> Result<(), NodeError> {
        cache::evict(ctx.node_id());
        Ok(())
    }

    fn on_message(&self, ctx: &NodeCtx, port: InputPort, msg: Msg) -> Result<(), NodeError> {
        if port != "in" {
            return Err(NodeError::runtime(format!("unexpected port `{port}`")));
        }
        let cfg: FunctionConfig = ctx.resolve_settings::<FunctionConfig>(&msg)?.into_inner();
        bump(ctx, STATUS_EXEC_COUNT);

        match eval(ctx, &cfg, &msg) {
            Ok(out_msgs) => {
                for m in out_msgs {
                    ctx.emit(PORT_OUT, m)?;
                }
                Ok(())
            }
            Err(err) => handle_error(ctx, &cfg, &msg, err),
        }
    }
}

/// Compile-if-needed + eval. Returns zero or more messages to emit.
fn eval(
    ctx: &NodeCtx,
    cfg: &FunctionConfig,
    msg: &Msg,
) -> Result<Vec<Msg>, FunctionError> {
    if cfg.script.trim().is_empty() {
        return Err(FunctionError::InvalidConfig("script is empty".into()));
    }

    let script_hash = cache::hash_script(&cfg.script);
    let ast = match cache::get(ctx.node_id(), script_hash) {
        Some(ast) => ast,
        None => {
            let compiled = compile(&cfg.script)?;
            cache::put(ctx.node_id(), script_hash, compiled.clone());
            compiled
        }
    };

    // Engine is built per-call so helpers can capture this node's
    // path and live graph handle. Rhai engine construction is cheap
    // (µs) — the expensive part is compile, which we just cached.
    let mut engine = Engine::new();
    if cfg.max_operations > 0 {
        engine.set_max_operations(cfg.max_operations);
    }
    // Defensive bounds — if the user goes crazy with string/array
    // growth, trip before memory pressure. Generous enough that
    // normal transforms don't hit them.
    engine.set_max_string_size(1_000_000);
    engine.set_max_array_size(100_000);

    helpers::install(&mut engine, ctx.graph(), ctx.node_path().clone());

    let mut scope = Scope::new();
    scope.push_dynamic("msg", msg_to_rhai(msg)?);

    let result: rhai::Dynamic = engine
        .eval_ast_with_scope(&mut scope, &ast)
        .map_err(|e| error::from_eval_err(&e, false))?;

    rhai_to_msg_batch(result)
}

fn compile(script: &str) -> Result<rhai::AST, FunctionError> {
    let engine = Engine::new();
    engine.compile(script).map_err(|e| error::from_parse_err(&e))
}

fn handle_error(
    ctx: &NodeCtx,
    cfg: &FunctionConfig,
    source_msg: &Msg,
    err: FunctionError,
) -> Result<(), NodeError> {
    tracing::warn!(
        node = %ctx.node_path().as_str(),
        stage = err.stage(),
        line = ?err.line(),
        error = %err,
        "function script error",
    );
    bump(ctx, STATUS_ERROR_COUNT);
    let _ = ctx.update_status(STATUS_LAST_ERROR, json!(err.to_string()));

    match cfg.on_error {
        OnError::Drop => Ok(()),
        OnError::Throw => Err(NodeError::runtime(err.to_string())),
        OnError::EmitErr => {
            let envelope = json!({
                "error": err.to_string(),
                "stage": err.stage(),
                "line": err.line(),
                "source_msg": serde_json::to_value(source_msg).unwrap_or(serde_json::Value::Null),
            });
            ctx.emit(PORT_ERR, Msg::new(envelope))
        }
    }
}

/// `ctx.update_status` requires the slot be declared `role: status`.
/// All three of our counters satisfy that, so a failure here means a
/// manifest drift bug — log and move on rather than crashing the
/// dispatch.
fn bump(ctx: &NodeCtx, slot: &str) {
    let next = ctx
        .read_status(slot)
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        .saturating_add(1);
    if let Err(e) = ctx.update_status(slot, json!(next)) {
        tracing::warn!(node = %ctx.node_path().as_str(), slot, error = %e, "function: status bump failed");
    }
}
