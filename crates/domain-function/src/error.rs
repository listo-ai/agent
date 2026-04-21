//! Errors crossing the Rhai boundary.
//!
//! These never escape the crate as-is — `function.rs` converts them
//! into `blocks_sdk::NodeError` (and, depending on `on_error` policy,
//! into an `err`-port emission or a status-slot update). Keeping a
//! dedicated type at this layer lets the eval path be explicit about
//! which stage failed (compile vs run vs convert) so the error
//! envelope downstream consumers see carries that detail.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FunctionError {
    /// `script` field empty or otherwise invalid before handing to Rhai.
    #[error("invalid script configuration: {0}")]
    InvalidConfig(String),

    /// Rhai parse / compile error. Line/position included if the
    /// engine supplied them.
    #[error("script compile error{at}: {msg}", at = fmt_at(line))]
    Compile {
        msg: String,
        line: Option<usize>,
    },

    /// Runtime error during eval — panic, exception, operation-counter
    /// tripwire, etc.
    #[error("script runtime error{at}: {msg}", at = fmt_at(line))]
    Runtime {
        msg: String,
        line: Option<usize>,
    },

    /// Could not turn a `Msg` into a Rhai value on the way in.
    #[error("msg → rhai conversion: {0}")]
    MsgSerialise(String),

    /// Could not turn a Rhai return into a `Msg` on the way out.
    #[error("rhai → msg conversion: {0}")]
    MsgDeserialise(String),
}

impl FunctionError {
    pub fn line(&self) -> Option<usize> {
        match self {
            FunctionError::Compile { line, .. } | FunctionError::Runtime { line, .. } => *line,
            _ => None,
        }
    }

    pub fn stage(&self) -> &'static str {
        match self {
            FunctionError::InvalidConfig(_) => "config",
            FunctionError::Compile { .. } => "compile",
            FunctionError::Runtime { .. } => "runtime",
            FunctionError::MsgSerialise(_) => "serialise",
            FunctionError::MsgDeserialise(_) => "deserialise",
        }
    }
}

/// Thin wrapper around Rhai's `EvalAltResult` that pulls the position
/// info out into our own shape. Kept in this module so `function.rs`
/// doesn't know about `rhai::EvalAltResult`.
pub(crate) fn from_eval_err(e: &rhai::EvalAltResult, compile_time: bool) -> FunctionError {
    let pos = e.position();
    let line = if pos.is_none() { None } else { pos.line() };
    let msg = e.to_string();
    if compile_time {
        FunctionError::Compile { msg, line }
    } else {
        FunctionError::Runtime { msg, line }
    }
}

pub(crate) fn from_parse_err(e: &rhai::ParseError) -> FunctionError {
    let pos = e.1;
    let line = if pos.is_none() { None } else { pos.line() };
    FunctionError::Compile {
        msg: e.0.to_string(),
        line,
    }
}

fn fmt_at(line: &Option<usize>) -> String {
    match line {
        Some(n) => format!(" (line {n})"),
        None => String::new(),
    }
}
