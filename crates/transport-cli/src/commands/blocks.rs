//! `agent plugins {list,get,enable,disable,reload}` — plugin operations.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum PluginsCmd {
    /// List all loaded plugins.
    List,
    /// Get details for a single plugin.
    Get {
        /// Plugin id (e.g. `acme-plugin`).
        id: String,
    },
    /// Enable a plugin.
    Enable {
        /// Plugin id.
        id: String,
    },
    /// Disable a plugin.
    Disable {
        /// Plugin id.
        id: String,
    },
    /// Trigger a full plugin reload scan.
    Reload,
    /// Get the process-runtime state for one plugin.
    Runtime {
        /// Plugin id.
        id: String,
    },
    /// Snapshot runtime state of every process plugin.
    RuntimeAll,
}

impl PluginsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List => "plugins list",
            Self::Get { .. } => "plugins get",
            Self::Enable { .. } => "plugins enable",
            Self::Disable { .. } => "plugins disable",
            Self::Reload => "plugins reload",
            Self::Runtime { .. } => "plugins runtime",
            Self::RuntimeAll => "plugins runtime-all",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &PluginsCmd) -> Result<()> {
    match cmd {
        PluginsCmd::List => {
            let plugins = client.plugins().list().await?;
            output::ok_table(
                fmt,
                &["ID", "VERSION", "LIFECYCLE", "DISPLAY_NAME", "KINDS"],
                &plugins,
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
            let plugin = client.plugins().get(id).await?;
            output::ok(fmt, &plugin)?;
        }
        PluginsCmd::Enable { id } => {
            client.plugins().enable(id).await?;
            output::ok_status(fmt, &format!("enabled {id}"))?;
        }
        PluginsCmd::Disable { id } => {
            client.plugins().disable(id).await?;
            output::ok_status(fmt, &format!("disabled {id}"))?;
        }
        PluginsCmd::Reload => {
            client.plugins().reload().await?;
            output::ok_status(fmt, "reload triggered")?;
        }
        PluginsCmd::Runtime { id } => {
            let state = client.plugins().runtime(id).await?;
            output::ok(fmt, &state)?;
        }
        PluginsCmd::RuntimeAll => {
            let entries = client.plugins().runtime_all().await?;
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
