//! `agent nodes {list,get,create}` — node operations.

use agent_client::{AgentClient, NodeListParams};
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum NodesCmd {
    /// List all nodes.
    List {
        /// Optional filter expression, e.g. `kind==sys.core.folder`.
        #[arg(long)]
        filter: Option<String>,
        /// Optional sort expression, e.g. `path,-kind`.
        #[arg(long)]
        sort: Option<String>,
        /// Optional 1-based page number.
        #[arg(long)]
        page: Option<u64>,
        /// Optional page size.
        #[arg(long)]
        size: Option<u64>,
    },
    /// Get a single node by path.
    Get {
        /// Node path (e.g. `/station/floor1/ahu-5`).
        path: String,
    },
    /// Create a child node.
    Create {
        /// Parent path.
        parent: String,
        /// Kind ID (e.g. `sys.compute.count`).
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
            Self::List { .. } => "nodes list",
            Self::Get { .. } => "nodes get",
            Self::Create { .. } => "nodes create",
            Self::Delete { .. } => "nodes delete",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &NodesCmd) -> Result<()> {
    match cmd {
        NodesCmd::List {
            filter,
            sort,
            page,
            size,
        } => {
            let page_out = client
                .nodes()
                .list_page(&NodeListParams {
                    filter: filter.clone(),
                    sort: sort.clone(),
                    page: *page,
                    size: *size,
                })
                .await?;
            match fmt {
                OutputFormat::Json => output::ok(fmt, &page_out)?,
                OutputFormat::Table => {
                    output::ok_table(
                        fmt,
                        &["PATH", "KIND", "LIFECYCLE", "ID"],
                        &page_out.data,
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
            }
        }
        NodesCmd::Get { path } => {
            let node = client.nodes().get(path).await?;
            output::ok(fmt, &node)?;
        }
        NodesCmd::Create { parent, kind, name } => {
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
