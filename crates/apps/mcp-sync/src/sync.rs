use crate::config::McpCompose;
use crate::formatters::sync_agent_config;
use anyhow::Result;

pub fn sync_all(compose: &McpCompose) -> Result<()> {
    for agent in &compose.agents {
        sync_agent_config(&agent.name, &agent.format, &agent.path, &compose.servers)?;
    }
    println!("✅ All agents synced successfully.");
    Ok(())
}
