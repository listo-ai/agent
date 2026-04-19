//! `agent slots write` — slot operations.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum SlotsCmd {
    /// Write a slot value.
    Write {
        /// Node path (e.g. `/station/counter`).
        path: String,
        /// Slot name (e.g. `in`).
        slot: String,
        /// Value as JSON (e.g. `42`, `"hello"`, `{"x":1}`).
        value: String,
    },
}

impl SlotsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Write { .. } => "slots write",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &SlotsCmd) -> Result<()> {
    match cmd {
        SlotsCmd::Write { path, slot, value } => {
            let parsed: serde_json::Value = serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::Value::String(value.clone()));
            let gen = client.slots().write(path, slot, &parsed).await?;
            output::ok_msg(fmt, &serde_json::json!({ "generation": gen }), &format!("generation {gen}"))?;
        }
    }
    Ok(())
}
