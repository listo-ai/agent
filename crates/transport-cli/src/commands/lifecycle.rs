//! `agent lifecycle <path> <to>` — lifecycle transitions.

use agent_client::AgentClient;
use anyhow::Result;

use crate::output::{self, OutputFormat};

pub async fn run(
    client: &AgentClient,
    fmt: OutputFormat,
    path: &str,
    to: &str,
) -> Result<()> {
    let new_state = client.lifecycle().transition(path, to).await?;
    output::ok_status(fmt, &format!("{path} → {new_state}"))?;
    Ok(())
}
