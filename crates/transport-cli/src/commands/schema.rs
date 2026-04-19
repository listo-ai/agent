//! `agent schema` — machine-readable command discovery.
//!
//! Dumps JSON Schema for a subcommand's inputs, outputs, and error
//! codes. No network round-trip — purely local metadata.
//!
//! ```bash
//! agent schema nodes create          # single command
//! agent schema --all -o json         # every command in one document
//! ```

use anyhow::Result;

use super::meta;
use crate::output::{self, OutputFormat};

pub fn run(fmt: OutputFormat, all: bool, command_parts: &[String]) -> Result<()> {
    if all {
        let all_schemas = meta::SchemaAllOutput {
            commands: meta::all_commands()
                .iter()
                .map(|c| c.to_schema_output())
                .collect(),
        };
        output::ok(fmt, &all_schemas)?;
    } else {
        let cmd_name = command_parts.join(" ");
        let info = meta::find_command(&cmd_name).ok_or_else(|| {
            anyhow::anyhow!("unknown command: {cmd_name}. Run `agent schema --all` to list all.")
        })?;
        output::ok(fmt, &info.to_schema_output())?;
    }
    Ok(())
}
