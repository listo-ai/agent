//! `agent capabilities` — show the host capability manifest.

use agent_client::AgentClient;
use anyhow::Result;

use crate::output::{self, OutputFormat};

pub async fn run(client: &AgentClient, fmt: OutputFormat) -> Result<()> {
    let manifest = client.capabilities().get_manifest().await?;

    match fmt {
        OutputFormat::Json => {
            output::ok(fmt, &manifest)?;
        }
        OutputFormat::Table => {
            println!(
                // NO_PRINTLN_LINT:allow
                "agent {}  ·  api v{}  ·  flow_schema={}  node_schema={}",
                manifest.platform.version,
                manifest.api.rest,
                manifest.platform.flow_schema,
                manifest.platform.node_schema,
            );
            println!(); // NO_PRINTLN_LINT:allow
            output::ok_table(
                fmt,
                &["CAPABILITY", "VERSION"],
                &manifest.capabilities,
                |c| vec![c.id.clone(), c.version.clone()],
            )?;
        }
    }
    Ok(())
}
