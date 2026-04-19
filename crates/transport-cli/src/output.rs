//! Output formatting — deterministic JSON contract surface.
//!
//! See `docs/design/CLI.md` § "LLM-friendly surface" for the full
//! contract. Every command funnels through [`ok`], [`ok_table`],
//! [`ok_status`], [`ok_msg`], or the error path [`error`]. Commands
//! never construct JSON directly.
//!
//! ## Exit codes (stable across versions)
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | Success |
//! | 1    | User error (bad args, not-found, precondition) |
//! | 2    | Infrastructure error (agent unreachable, timeout) |
//! | 3    | Internal error (parse failure, panic caught) |
//!
//! ## Error shape
//!
//! ```json
//! {"code": "bad_path", "message": "…", "details": {…}}
//! ```

use std::fmt::Write as _;

use agent_client::ClientError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---- exit codes (stable across versions) ----------------------------------

pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_USER_ERROR: i32 = 1;
pub const EXIT_INFRA_ERROR: i32 = 2;
pub const EXIT_INTERNAL_ERROR: i32 = 3;

// ---- output format --------------------------------------------------------

/// Output format selected by `--output`.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Pretty-printed JSON.
    Json,
    /// Human-readable aligned table (default).
    #[default]
    Table,
}

// ---- error shape ----------------------------------------------------------

/// Deterministic error shape — identical for every command.
///
/// `code` is a stable snake_case enum. `message` is human-readable and
/// may change between versions. `details` is per-code structured data.
///
/// ```json
/// {"code": "bad_path", "message": "…", "details": {…}}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliError {
    pub code: String,
    pub message: String,
    pub details: Value,
    #[serde(skip)]
    pub exit_code: i32,
}

impl CliError {
    /// Classify a [`ClientError`] into a stable CLI error.
    pub fn from_client(err: &ClientError) -> Self {
        match err {
            ClientError::Http { status, message } => {
                let (code, exit_code) = classify_http(*status, message);
                Self {
                    code,
                    message: message.clone(),
                    details: serde_json::json!({ "http_status": status }),
                    exit_code,
                }
            }
            ClientError::Transport(e) => Self {
                code: "agent_unreachable".into(),
                message: e.to_string(),
                details: Value::Object(Default::default()),
                exit_code: EXIT_INFRA_ERROR,
            },
            ClientError::Parse(msg) => Self {
                code: "parse_error".into(),
                message: msg.clone(),
                details: Value::Object(Default::default()),
                exit_code: EXIT_INTERNAL_ERROR,
            },
            ClientError::CapabilityMismatch(msg) => Self {
                code: "capability_mismatch".into(),
                message: msg.clone(),
                details: Value::Object(Default::default()),
                exit_code: EXIT_USER_ERROR,
            },
        }
    }

    /// Classify an [`anyhow::Error`] — tries to downcast to
    /// `ClientError` first, falls back to internal error.
    pub fn from_anyhow(err: &anyhow::Error) -> Self {
        if let Some(ce) = err.downcast_ref::<ClientError>() {
            Self::from_client(ce)
        } else {
            Self {
                code: "internal_error".into(),
                message: err.to_string(),
                details: Value::Object(Default::default()),
                exit_code: EXIT_INTERNAL_ERROR,
            }
        }
    }
}

/// Classify an HTTP error status + message into a stable error code.
fn classify_http(status: u16, message: &str) -> (String, i32) {
    let lower = message.to_lowercase();
    match status {
        404 => ("not_found".into(), EXIT_USER_ERROR),
        422 => ("invalid_value".into(), EXIT_USER_ERROR),
        400 => {
            let code = if lower.contains("bad path `") {
                // parse_path failure: "bad path `/...`: ..."
                "bad_path"
            } else if lower.contains("not found") || lower.contains("no node") {
                "not_found"
            } else if lower.contains("already exists") || lower.contains("duplicate") {
                "duplicate_name"
            } else if lower.contains("illegal") && lower.contains("transition") {
                "illegal_transition"
            } else {
                "bad_request"
            };
            (code.into(), EXIT_USER_ERROR)
        }
        s if s >= 500 => ("server_error".into(), EXIT_INFRA_ERROR),
        _ => ("unknown_error".into(), EXIT_USER_ERROR),
    }
}

// ---- ok helpers (success path) --------------------------------------------

/// Print a single serialisable value.
pub fn ok<T: Serialize>(fmt: OutputFormat, value: &T) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(value)?); // NO_PRINTLN_LINT:allow
        }
        OutputFormat::Table => {
            // Fall back to JSON for single values — table only helps
            // for lists and structured records.
            println!("{}", serde_json::to_string_pretty(value)?); // NO_PRINTLN_LINT:allow
        }
    }
    Ok(())
}

/// Print a list as aligned table (table mode) or JSON array (json mode).
pub fn ok_table<T: Serialize>(
    fmt: OutputFormat,
    headers: &[&str],
    rows: &[T],
    row_to_cells: impl Fn(&T) -> Vec<String>,
) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(rows)?); // NO_PRINTLN_LINT:allow
        }
        OutputFormat::Table => {
            render_table(headers, rows, row_to_cells);
        }
    }
    Ok(())
}

/// Print a status message (table = plain text; json = `{"status":"…"}`).
pub fn ok_status(fmt: OutputFormat, message: &str) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!( // NO_PRINTLN_LINT:allow
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({ "status": message }))?,
            );
        }
        OutputFormat::Table => {
            println!("{message}"); // NO_PRINTLN_LINT:allow
        }
    }
    Ok(())
}

/// Print a typed value (json) or a human-friendly message (table).
///
/// Use when json output should be structured data but table output is
/// a simple one-liner (e.g. `slots write` → `{"generation": 42}` vs
/// `"generation 42"`).
pub fn ok_msg<T: Serialize>(fmt: OutputFormat, value: &T, table_msg: &str) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(value)?); // NO_PRINTLN_LINT:allow
        }
        OutputFormat::Table => {
            println!("{table_msg}"); // NO_PRINTLN_LINT:allow
        }
    }
    Ok(())
}

/// Print a [`CliError`] and return its exit code.
pub fn error(fmt: OutputFormat, err: &CliError) -> i32 {
    match fmt {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(err) {
                println!("{json}"); // NO_PRINTLN_LINT:allow
            }
        }
        OutputFormat::Table => {
            eprintln!("error: {} ({})", err.message, err.code); // NO_PRINTLN_LINT:allow
        }
    }
    err.exit_code
}

// ---- table renderer -------------------------------------------------------

fn render_table<T>(headers: &[&str], rows: &[T], row_to_cells: impl Fn(&T) -> Vec<String>) {
    let cell_rows: Vec<Vec<String>> = rows.iter().map(&row_to_cells).collect();

    // Compute column widths.
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in &cell_rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    // Header.
    let mut line = String::new();
    for (i, h) in headers.iter().enumerate() {
        if i > 0 {
            line.push_str("  ");
        }
        let _ = write!(line, "{:<width$}", h, width = widths[i]);
    }
    println!("{line}"); // NO_PRINTLN_LINT:allow

    // Separator.
    let sep: String = widths
        .iter()
        .map(|w| "-".repeat(*w))
        .collect::<Vec<_>>()
        .join("  ");
    println!("{sep}"); // NO_PRINTLN_LINT:allow

    // Rows.
    for row in &cell_rows {
        let mut line = String::new();
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            let w = widths.get(i).copied().unwrap_or(0);
            let _ = write!(line, "{:<width$}", cell, width = w);
        }
        println!("{line}"); // NO_PRINTLN_LINT:allow
    }
}
