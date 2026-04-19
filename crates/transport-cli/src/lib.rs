//! CLI transport — clap subcommand tree + execution.
//!
//! This crate provides the command definitions and execution logic for
//! the CLI surface. It uses `agent-client` to talk to a running agent
//! over HTTP. The agent binary mounts these subcommands alongside its
//! own `run` command.
//!
//! # Architecture
//!
//! ```text
//! agent binary
//!   ├─ run           → starts the daemon (lives in apps/agent)
//!   ├─ health        → GET /healthz
//!   ├─ capabilities  → GET /api/v1/capabilities
//!   ├─ nodes …       → node CRUD
//!   ├─ slots …       → slot writes
//!   ├─ config …      → config writes
//!   ├─ links …       → link CRUD
//!   ├─ lifecycle …   → lifecycle transitions
//!   └─ seed …        → seed presets
//! ```
//!
//! ## Global options
//!
//! `--url`, `--token`, `--output` are threaded through to every subcommand.
//!
//! ## `--help-json`
//!
//! Every subcommand accepts `--help-json` (via `GlobalOpts`, `global = true`).
//! When set, `execute()` intercepts *before* making any HTTP call and prints the
//! command's machine-readable metadata from [`commands::meta`].
//!
//! ## `--examples` convention in `CommandMeta`
//!
//! Each subcommand's `CommandMeta` declaration in [`commands::meta`] carries
//! a `examples: &[&'static str]` slice.  Add entries there; `--help-json` picks
//! them up automatically.  No separate attribute or proc-macro required.
//!
//! ## Exit codes
//!
//! `execute()` never returns a Rust error to the caller.  Errors are classified
//! into a [`output::CliError`], printed as JSON (when `-o json`) or to stderr
//! (table mode), then `std::process::exit` is called with the right code:
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | Success |
//! | 1    | User error (bad args, not found, precondition) |
//! | 2    | Infrastructure error (agent unreachable, timeout) |
//! | 3    | Internal error (parse failure, panic caught) |

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

mod commands;
mod output;

pub use commands::meta;
pub use commands::{CliCommand, GlobalOpts};
pub use output::{
    CliError, OutputFormat, EXIT_INFRA_ERROR, EXIT_INTERNAL_ERROR, EXIT_SUCCESS, EXIT_USER_ERROR,
};

use agent_client::{AgentClient, AgentClientOptions};

/// Execute a CLI command against a running agent.
///
/// This function **never returns an `Err`** to the caller.  On failure it
/// prints the structured error (respecting `-o json`) and calls
/// [`std::process::exit`] with the appropriate exit code (1/2/3).
///
/// On `--help-json`, it prints metadata without making any HTTP calls and
/// exits 0.
pub async fn execute(global: &GlobalOpts, cmd: &CliCommand) {
    let fmt = global.output;

    // --help-json: print machine-readable metadata and exit 0. No HTTP call.
    if global.help_json {
        let name = cmd.command_name();
        match commands::meta::find_command(name) {
            Some(info) => {
                let help = info.to_help_json();
                if let Ok(json) = serde_json::to_string_pretty(&help) {
                    println!("{json}"); // NO_PRINTLN_LINT:allow
                }
                std::process::exit(0);
            }
            None => {
                eprintln!("--help-json: no metadata for command `{name}`"); // NO_PRINTLN_LINT:allow
                std::process::exit(3);
            }
        }
    }

    let client = AgentClient::with_options(AgentClientOptions {
        base_url: global.url.clone(),
        token: global.token.clone(),
    });

    if let Err(err) = commands::dispatch(&client, global, cmd).await {
        let cli_err = CliError::from_anyhow(&err);
        let exit_code = output::error(fmt, &cli_err);
        std::process::exit(exit_code);
    }
}
