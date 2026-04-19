//! `agent nodes {list,get,create}` — node operations.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum NodesCmd {
    /// List all nodes.
    List,
    /// Get a single node by path.
    Get {
        /// Node path (e.g. `/station/floor1/ahu-5`).
        path: String,
    },
    /// Create a child node.
    Create {
        /// Parent path.
        parent: String,
        /// Kind ID (e.g. `acme.compute.count`).
        kind: String,
        /// Node name.
        name: String,
    },
    /// Delete a node (and its children, depending on cascade policy).
    Delete {
        /// Node path.
        path: String,
    },
}

impl NodesCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List => "nodes list",
            Self::Get { .. } => "nodes get",
            Self::Create { .. } => "nodes create",
            Self::Delete { .. } => "nodes delete",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &NodesCmd) -> Result<()> {
    match cmd {
        NodesCmd::List => {
            let nodes = client.nodes().list().await?;
            output::ok_table(
                fmt,
                &["PATH", "KIND", "LIFECYCLE", "ID"],
                &nodes,
                |n| {
                    vec![
                        n.path.clone(),
                        n.kind.clone(),
                        n.lifecycle.clone(),
                        n.id.clone(),
                    ]
                },
            )?;
        }
        NodesCmd::Get { path } => {
            let node = client.nodes().get(path).await?;
            output::ok(fmt, &node)?;
        }
        NodesCmd::Create {
            parent,
            kind,
            name,
        } => {
            let created = client.nodes().create(parent, kind, name).await?;
            output::ok_msg(fmt, &created, &format!("created {}", created.path))?;
        }
        NodesCmd::Delete { path } => {
            client.nodes().delete(path).await?;
            output::ok_status(fmt, &format!("deleted {path}"))?;
        }
    }
    Ok(())
}
