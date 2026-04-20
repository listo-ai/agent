//! `agent blocks {list,get,enable,disable,reload}` — block operations.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum PluginsCmd {
    /// List all loaded blocks.
    List,
    /// Get details for a single block.
    Get {
        /// Block id (e.g. `acme-block`).
        id: String,
    },
    /// Enable a block.
    Enable {
        /// Block id.
        id: String,
    },
    /// Disable a block.
    Disable {
        /// Block id.
        id: String,
    },
    /// Trigger a full block reload scan.
    Reload,
    /// Get the process-runtime state for one block.
    Runtime {
        /// Block id.
        id: String,
    },
    /// Snapshot runtime state of every process block.
    RuntimeAll,
}

impl PluginsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List => "blocks list",
            Self::Get { .. } => "blocks get",
            Self::Enable { .. } => "blocks enable",
            Self::Disable { .. } => "blocks disable",
            Self::Reload => "blocks reload",
            Self::Runtime { .. } => "blocks runtime",
            Self::RuntimeAll => "blocks runtime-all",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &PluginsCmd) -> Result<()> {
    match cmd {
        PluginsCmd::List => {
            let blocks = client.blocks().list().await?;
            output::ok_table(
                fmt,
                &["ID", "VERSION", "LIFECYCLE", "DISPLAY_NAME", "KINDS"],
                &blocks,
                |p| {
                    vec![
                        p.id.clone(),
                        p.version.clone(),
                        p.lifecycle.to_string(),
                        p.display_name.clone().unwrap_or_default(),
                        p.kinds.join(","),
                    ]
                },
            )?;
        }
        PluginsCmd::Get { id } => {
            let block = client.blocks().get(id).await?;
            output::ok(fmt, &block)?;
        }
        PluginsCmd::Enable { id } => {
            client.blocks().enable(id).await?;
            output::ok_status(fmt, &format!("enabled {id}"))?;
        }
        PluginsCmd::Disable { id } => {
            client.blocks().disable(id).await?;
            output::ok_status(fmt, &format!("disabled {id}"))?;
        }
        PluginsCmd::Reload => {
            client.blocks().reload().await?;
            output::ok_status(fmt, "reload triggered")?;
        }
        PluginsCmd::Runtime { id } => {
            let state = client.blocks().runtime(id).await?;
            output::ok(fmt, &state)?;
        }
        PluginsCmd::RuntimeAll => {
            let entries = client.blocks().runtime_all().await?;
            output::ok_table(fmt, &["ID", "STATUS", "DETAIL"], &entries, |e| {
                vec![
                    e.id.clone(),
                    status_label(&e.state),
                    status_detail(&e.state),
                ]
            })?;
        }
    }
    Ok(())
}

fn status_label(s: &agent_client::types::PluginRuntimeState) -> String {
    use agent_client::types::PluginRuntimeState as S;
    match s {
        S::Idle => "idle",
        S::Starting => "starting",
        S::Ready => "ready",
        S::Degraded { .. } => "degraded",
        S::Restarting { .. } => "restarting",
        S::Failed { .. } => "failed",
        S::Stopped => "stopped",
    }
    .into()
}

fn status_detail(s: &agent_client::types::PluginRuntimeState) -> String {
    use agent_client::types::PluginRuntimeState as S;
    match s {
        S::Degraded { detail } => detail.clone(),
        S::Restarting {
            attempt,
            backoff_ms,
            reason,
        } => format!("attempt={attempt} backoff_ms={backoff_ms} reason={reason}"),
        S::Failed { reason } => reason.clone(),
        _ => String::new(),
    }
}
