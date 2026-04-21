//! `agent kinds list` — kind palette operations.

use agent_client::{AgentClient, ListKindsOptions};
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum KindsCmd {
    /// List all registered kinds. Shortcuts (`--facet`, `--under`) and
    /// the full RSQL surface (`--filter`, `--sort`) compose on the
    /// server — pass all four if you need to narrow hard.
    List {
        /// Concrete-param shortcut: kinds carrying this facet
        /// (camelCase, e.g. `isContainer`, `isCompute`).
        #[arg(long)]
        facet: Option<String>,

        /// Concrete-param shortcut: kinds placeable under this parent
        /// node path.
        #[arg(long, value_name = "PARENT_PATH")]
        under: Option<String>,

        /// RSQL filter over `id` / `org` / `display_name` /
        /// `facets` / `placement_class`. Examples:
        ///
        /// --filter 'org==com.listo'
        /// --filter 'facets=contains=isCompute;placement_class==free'
        /// --filter 'id=prefix=sys.compute.'
        #[arg(long)]
        filter: Option<String>,

        /// Comma-separated sort fields; prefix a field with `-` for
        /// descending. Defaults to ascending `id`.
        ///
        /// --sort org,id       (by publisher then id)
        /// --sort -display_name
        #[arg(long)]
        sort: Option<String>,
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
        KindsCmd::List {
            facet,
            under,
            filter,
            sort,
        } => {
            let kinds = client
                .kinds()
                .list_with(ListKindsOptions {
                    facet: facet.as_deref(),
                    placeable_under: under.as_deref(),
                    filter: filter.as_deref(),
                    sort: sort.as_deref(),
                })
                .await?;
            output::ok_table(
                fmt,
                &["ID", "ORG", "DISPLAY_NAME", "CLASS", "FACETS"],
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
                        k.org.clone(),
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
