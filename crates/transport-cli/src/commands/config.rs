//! `agent config set` — config operations.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Set a node's config blob (JSON) and re-fire `on_init`.
    Set {
        /// Node path.
        path: String,
        /// Config as JSON string (e.g. `{"step":5}`).
        config: String,
    },
}

impl ConfigCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Set { .. } => "config set",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &ConfigCmd) -> Result<()> {
    match cmd {
        ConfigCmd::Set { path, config } => {
            let parsed: serde_json::Value = serde_json::from_str(config)?;
            client.config().set(path, &parsed).await?;
            output::ok_status(fmt, "ok")?;
        }
    }
    Ok(())
}
