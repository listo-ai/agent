//! `agent health` — check agent liveness.

use agent_client::AgentClient;
use anyhow::Result;

use crate::output::{self, OutputFormat};

pub async fn run(client: &AgentClient, fmt: OutputFormat) -> Result<()> {
    let ok = client.health().check().await?;
    if ok {
        output::ok_status(fmt, "ok")?;
    } else {
        output::ok_status(fmt, "unhealthy")?;
        std::process::exit(1);
    }
    Ok(())
}
