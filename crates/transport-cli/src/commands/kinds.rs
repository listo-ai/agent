//! `agent kinds list` — kind operations.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum KindsCmd {
    /// List all registered kinds.
    List {
        /// Filter by facet (camelCase, e.g. `isContainer`, `isCompute`).
        #[arg(long)]
        facet: Option<String>,
        /// Filter to kinds placeable under this kind id.
        #[arg(long)]
        under: Option<String>,
    },
}

impl KindsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List { .. } => "kinds list",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &KindsCmd) -> Result<()> {
    match cmd {
        KindsCmd::List { facet, under } => {
            let kinds = client
                .kinds()
                .list(facet.as_deref(), under.as_deref())
                .await?;
            output::ok_table(
                fmt,
                &["ID", "DISPLAY_NAME", "CLASS", "FACETS"],
                &kinds,
                |k| {
                    let facets = k
                        .facets
                        .iter()
                        .map(|f| format!("{f:?}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    vec![
                        k.id.clone(),
                        k.display_name.clone().unwrap_or_default(),
                        k.placement_class.clone(),
                        facets,
                    ]
                },
            )?;
        }
    }
    Ok(())
}
