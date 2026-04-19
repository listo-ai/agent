//! `agent auth whoami` — auth introspection commands.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum AuthCmd {
    /// Show the resolved auth context for this call — actor, tenant,
    /// scopes, provider.
    Whoami,
}

impl AuthCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Whoami => "auth whoami",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &AuthCmd) -> Result<()> {
    match cmd {
        AuthCmd::Whoami => {
            let who = client.auth().whoami().await?;
            output::ok(fmt, &who)?;
        }
    }
    Ok(())
}
