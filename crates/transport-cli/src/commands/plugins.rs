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
}

impl PluginsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List => "plugins list",
            Self::Get { .. } => "plugins get",
            Self::Enable { .. } => "plugins enable",
            Self::Disable { .. } => "plugins disable",
            Self::Reload => "plugins reload",
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
    }
    Ok(())
}
