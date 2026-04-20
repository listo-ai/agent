//! `agent users` — user management operations.

use agent_client::types::{GrantRoleReq, UserDto};
use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum UsersCmd {
    /// List `sys.auth.user` nodes with optional tag-aware filtering.
    List {
        /// RSQL filter expression, e.g. `tags.labels=contains=ops`.
        #[arg(long)]
        filter: Option<String>,
        /// Sort expression, e.g. `path` or `-path`.
        #[arg(long)]
        sort: Option<String>,
    },

    /// Grant a role to a user (submits wire shape; Zitadel fan-out is future).
    Grant {
        /// User node id (UUID).
        user_id: String,
        /// Role to grant (e.g. `org_admin`).
        #[arg(long)]
        role: String,
        /// Bulk-action correlation id (Studio-generated UUID for the session).
        #[arg(long)]
        bulk_action_id: String,
    },
}

impl UsersCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List { .. } => "users list",
            Self::Grant { .. } => "users grant",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &UsersCmd) -> Result<()> {
    match cmd {
        UsersCmd::List { filter, sort } => {
            let users = client
                .users()
                .list(filter.as_deref(), sort.as_deref(), None, None)
                .await?;
            output::ok_table(
                fmt,
                &["ID", "PATH", "DISPLAY_NAME", "EMAIL", "TAGS"],
                &users,
                |u: &UserDto| {
                    let tags = format_tags(u);
                    vec![
                        u.id.clone(),
                        u.path.clone(),
                        u.display_name.clone().unwrap_or_default(),
                        u.email.clone().unwrap_or_default(),
                        tags,
                    ]
                },
            )?;
        }
        UsersCmd::Grant { user_id, role, bulk_action_id } => {
            let resp = client
                .users()
                .grant_role(
                    user_id,
                    &GrantRoleReq {
                        role: role.clone(),
                        bulk_action_id: bulk_action_id.clone(),
                    },
                )
                .await?;
            output::ok_msg(fmt, &resp, &resp.status)?;
        }
    }
    Ok(())
}

fn format_tags(u: &UserDto) -> String {
    let mut parts = Vec::new();
    if !u.tags.labels.is_empty() {
        parts.push(format!("[{}]", u.tags.labels.join(",")));
    }
    if !u.tags.kv.is_empty() {
        let kv = u
            .tags
            .kv
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("{{{kv}}}"));
    }
    parts.join("")
}
