//! `agent links {list,create,remove}` — link operations.

use agent_client::types::LinkEndpointRef;
use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum LinksCmd {
    /// List all links.
    List,
    /// Create a link between two slot endpoints.
    Create {
        /// Source node path.
        #[arg(long)]
        source_path: String,
        /// Source slot name.
        #[arg(long)]
        source_slot: String,
        /// Target node path.
        #[arg(long)]
        target_path: String,
        /// Target slot name.
        #[arg(long)]
        target_slot: String,
    },
    /// Remove a link by ID.
    Remove {
        /// Link UUID.
        id: String,
    },
}

impl LinksCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List => "links list",
            Self::Create { .. } => "links create",
            Self::Remove { .. } => "links remove",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &LinksCmd) -> Result<()> {
    match cmd {
        LinksCmd::List => {
            let links = client.links().list().await?;
            output::ok_table(
                fmt,
                &["ID", "SOURCE", "TARGET"],
                &links,
                |l| {
                    let src = format!(
                        "{}:{}",
                        l.source.path.as_deref().unwrap_or(&l.source.node_id),
                        l.source.slot,
                    );
                    let tgt = format!(
                        "{}:{}",
                        l.target.path.as_deref().unwrap_or(&l.target.node_id),
                        l.target.slot,
                    );
                    vec![l.id.clone(), src, tgt]
                },
            )?;
        }
        LinksCmd::Create {
            source_path,
            source_slot,
            target_path,
            target_slot,
        } => {
            let source = LinkEndpointRef::by_path(source_path.clone(), source_slot.clone());
            let target = LinkEndpointRef::by_path(target_path.clone(), target_slot.clone());
            let id = client.links().create(&source, &target).await?;
            output::ok_msg(fmt, &serde_json::json!({ "id": id }), &format!("created {id}"))?;
        }
        LinksCmd::Remove { id } => {
            client.links().remove(id).await?;
            output::ok_status(fmt, "removed")?;
        }
    }
    Ok(())
}
