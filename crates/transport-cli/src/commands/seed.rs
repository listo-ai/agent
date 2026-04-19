//! `agent seed <preset>` — seed presets for testing.

use agent_client::AgentClient;
use anyhow::Result;

use crate::output::{self, OutputFormat};

pub async fn run(client: &AgentClient, fmt: OutputFormat, preset: &str) -> Result<()> {
    let result = client.seed().apply(preset).await?;
    match fmt {
        OutputFormat::Json => {
            output::ok(fmt, &result)?;
        }
        OutputFormat::Table => {
            println!("seeded folder: {}", result.folder); // NO_PRINTLN_LINT:allow
            for node in &result.nodes {
                println!("  {} ({})", node.path, node.kind); // NO_PRINTLN_LINT:allow
            }
            if !result.links.is_empty() {
                println!("links: {}", result.links.join(", ")); // NO_PRINTLN_LINT:allow
            }
        }
    }
    Ok(())
}
