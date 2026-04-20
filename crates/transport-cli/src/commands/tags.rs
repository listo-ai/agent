//! `agent tags` — manage `config.tags` on a node.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;
use domain_tags::parse_shorthand;
use serde_json::json;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum TagsCmd {
    /// Read the current tags on a node.
    Get {
        /// Node path (e.g. `/station/counter`).
        path: String,
    },

    /// Set tags using shorthand notation.
    ///
    /// Notation: `[label1,label2]{key:value}` — labels, kv pairs, or both.
    Set {
        /// Node path (e.g. `/station/counter`).
        path: String,
        /// Shorthand tags expression (e.g. `[code,ops]{site:abc}`).
        tags: String,
    },

    /// Clear all tags from a node (writes null to config.tags).
    Clear {
        /// Node path (e.g. `/station/counter`).
        path: String,
    },
}

impl TagsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Get { .. } => "tags get",
            Self::Set { .. } => "tags set",
            Self::Clear { .. } => "tags clear",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &TagsCmd) -> Result<()> {
    match cmd {
        TagsCmd::Get { path } => {
            let node = client.nodes().get(path).await?;
            let tags_val = node
                .slots
                .iter()
                .find(|s| s.name == "config.tags")
                .map(|s| s.value.clone())
                .unwrap_or(serde_json::Value::Null);

            let labels: Vec<String> = tags_val
                .get("labels")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let kv_entries: Vec<(String, String)> = tags_val
                .get("kv")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                        .collect()
                })
                .unwrap_or_default();

            match fmt {
                OutputFormat::Json => output::ok(fmt, &tags_val)?,
                OutputFormat::Table => {
                    let labels_str = if labels.is_empty() {
                        "(none)".to_string()
                    } else {
                        labels.join(", ")
                    };
                    let kv_str = if kv_entries.is_empty() {
                        "(none)".to_string()
                    } else {
                        kv_entries
                            .iter()
                            .map(|(k, v)| format!("{k}={v}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    output::ok_table(
                        fmt,
                        &["PATH", "LABELS", "KV"],
                        &[path],
                        |p| vec![p.to_string(), labels_str.clone(), kv_str.clone()],
                    )?;
                }
            }
        }
        TagsCmd::Set { path, tags } => {
            let parsed = parse_shorthand(tags)
                .map_err(|e| anyhow::anyhow!("invalid tags: {e}"))?;

            let value = json!({
                "labels": parsed.labels,
                "kv": parsed.kv,
            });

            let gen = client.slots().write(path, "config.tags", &value).await?;
            output::ok_msg(fmt, &serde_json::json!({ "generation": gen }),
                &format!("Tags updated on {path} (generation {gen})."))?;
        }
        TagsCmd::Clear { path } => {
            let gen = client
                .slots()
                .write(path, "config.tags", &serde_json::Value::Null)
                .await?;
            output::ok_msg(fmt, &serde_json::json!({ "generation": gen }),
                &format!("Tags cleared on {path} (generation {gen})."))?;
        }
    }
    Ok(())
}
