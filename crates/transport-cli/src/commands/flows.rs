//! `agent flows <subcommand>` — flow document operations.
//!
//! All mutating commands accept an optional `--expected-head` flag that
//! provides optimistic-concurrency control (OCC): the server rejects the
//! request with 409 if the live head revision id doesn't match.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;
use serde_json::Value as JsonValue;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum FlowsCmd {
    /// List all flows.
    List {
        /// Maximum number of flows to return (default: 50).
        #[arg(long)]
        limit: Option<u32>,
        /// Skip this many flows (default: 0).
        #[arg(long)]
        offset: Option<u32>,
    },

    /// Get a single flow by id.
    Get {
        /// Flow id.
        id: String,
    },

    /// Create a new flow.
    Create {
        /// Human-readable name.
        name: String,
        /// Initial document as a JSON string (default: `{}`).
        #[arg(long, default_value = "{}")]
        document: String,
        /// Author tag.
        #[arg(long, default_value = "cli")]
        author: String,
    },

    /// Delete a flow and its entire revision history.
    Delete {
        /// Flow id.
        id: String,
        /// Expected current head revision id (omit to bypass OCC check).
        #[arg(long)]
        expected_head: Option<String>,
    },

    /// Append a forward edit revision to a flow.
    Edit {
        /// Flow id.
        id: String,
        /// New document as a JSON string.
        document: String,
        /// Short description of this change.
        #[arg(long, default_value = "edited via CLI")]
        summary: String,
        /// Expected current head revision id.
        #[arg(long)]
        expected_head: Option<String>,
        /// Author tag.
        #[arg(long, default_value = "cli")]
        author: String,
    },

    /// Undo the last logical edit.
    Undo {
        /// Flow id.
        id: String,
        /// Expected current head revision id.
        #[arg(long)]
        expected_head: Option<String>,
        /// Author tag.
        #[arg(long, default_value = "cli")]
        author: String,
    },

    /// Redo the next undone edit.
    Redo {
        /// Flow id.
        id: String,
        /// Expected current head revision id.
        #[arg(long)]
        expected_head: Option<String>,
        /// Expected redo-target revision id (guards stale-cursor redo).
        #[arg(long)]
        expected_target: Option<String>,
        /// Author tag.
        #[arg(long, default_value = "cli")]
        author: String,
    },

    /// Revert a flow to the state at a specific revision.
    Revert {
        /// Flow id.
        id: String,
        /// Target revision id to revert to.
        #[arg(long)]
        to: String,
        /// Expected current head revision id.
        #[arg(long)]
        expected_head: Option<String>,
        /// Author tag.
        #[arg(long, default_value = "cli")]
        author: String,
    },

    /// List revisions for a flow.
    Revisions {
        /// Flow id.
        id: String,
        /// Maximum number of revisions to return (default: 50).
        #[arg(long)]
        limit: Option<u32>,
        /// Skip this many revisions (default: 0).
        #[arg(long)]
        offset: Option<u32>,
    },

    /// Return the materialised flow document at a specific revision.
    DocumentAt {
        /// Flow id.
        id: String,
        /// Revision id.
        #[arg(long)]
        rev_id: String,
    },
}

impl FlowsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List { .. } => "flows list",
            Self::Get { .. } => "flows get",
            Self::Create { .. } => "flows create",
            Self::Delete { .. } => "flows delete",
            Self::Edit { .. } => "flows edit",
            Self::Undo { .. } => "flows undo",
            Self::Redo { .. } => "flows redo",
            Self::Revert { .. } => "flows revert",
            Self::Revisions { .. } => "flows revisions",
            Self::DocumentAt { .. } => "flows document-at",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &FlowsCmd) -> Result<()> {
    match cmd {
        FlowsCmd::List { limit, offset } => {
            let flows = client.flows().list(*limit, *offset).await?;
            output::ok_table(
                fmt,
                &["ID", "NAME", "HEAD_SEQ", "HEAD_REVISION_ID"],
                &flows,
                |f| {
                    vec![
                        output::compact_id(&f.id),
                        f.name.clone(),
                        f.head_seq.to_string(),
                        f.head_revision_id.as_deref().map(output::compact_id).unwrap_or_default(),
                    ]
                },
            )?;
        }

        FlowsCmd::Get { id } => {
            let flow = client.flows().get(id).await?;
            output::ok_msg(fmt, &flow, &format!("{} ({})", flow.name, output::compact_id(&flow.id)))?;
        }

        FlowsCmd::Create {
            name,
            document,
            author,
        } => {
            let doc: JsonValue = serde_json::from_str(document)
                .map_err(|e| anyhow::anyhow!("invalid JSON for --document: {e}"))?;
            let flow = client.flows().create(name, doc, author).await?;
            output::ok_msg(
                fmt,
                &flow,
                &format!("created flow {} ({})", flow.name, output::compact_id(&flow.id)),
            )?;
        }

        FlowsCmd::Delete { id, expected_head } => {
            client.flows().delete(id, expected_head.as_deref()).await?;
            output::ok_status(fmt, &format!("deleted flow {id}"))?;
        }

        FlowsCmd::Edit {
            id,
            document,
            summary,
            expected_head,
            author,
        } => {
            let doc: JsonValue = serde_json::from_str(document)
                .map_err(|e| anyhow::anyhow!("invalid JSON for document: {e}"))?;
            let result = client
                .flows()
                .edit(id, expected_head.as_deref(), doc, author, summary)
                .await?;
            output::ok_msg(
                fmt,
                &result,
                &format!("head is now {}", output::compact_id(&result.head_revision_id)),
            )?;
        }

        FlowsCmd::Undo {
            id,
            expected_head,
            author,
        } => {
            let result = client
                .flows()
                .undo(id, expected_head.as_deref(), author)
                .await?;
            output::ok_msg(
                fmt,
                &result,
                &format!("head is now {}", output::compact_id(&result.head_revision_id)),
            )?;
        }

        FlowsCmd::Redo {
            id,
            expected_head,
            expected_target,
            author,
        } => {
            let result = client
                .flows()
                .redo(
                    id,
                    expected_head.as_deref(),
                    expected_target.as_deref(),
                    author,
                )
                .await?;
            output::ok_msg(
                fmt,
                &result,
                &format!("head is now {}", output::compact_id(&result.head_revision_id)),
            )?;
        }

        FlowsCmd::Revert {
            id,
            to,
            expected_head,
            author,
        } => {
            let result = client
                .flows()
                .revert(id, expected_head.as_deref(), to, author)
                .await?;
            output::ok_msg(
                fmt,
                &result,
                &format!("head is now {}", output::compact_id(&result.head_revision_id)),
            )?;
        }

        FlowsCmd::Revisions { id, limit, offset } => {
            let revs = client.flows().list_revisions(id, *limit, *offset).await?;
            output::ok_table(
                fmt,
                &["SEQ", "ID", "OP", "AUTHOR", "SUMMARY", "CREATED_AT"],
                &revs,
                |r| {
                    vec![
                        r.seq.to_string(),
                        output::compact_id(&r.id),
                        r.op.clone(),
                        r.author.clone(),
                        r.summary.clone(),
                        r.created_at.clone(),
                    ]
                },
            )?;
        }

        FlowsCmd::DocumentAt { id, rev_id } => {
            let doc = client.flows().document_at(id, rev_id).await?;
            output::ok_msg(fmt, &doc, "flow document")?;
        }
    }

    Ok(())
}
