use anyhow::Result;
use std::process::Command;
use crate::config::ServerConfig;
use std::time::Duration;

/// Basic health check: spawn the process and see if it immediately crashes.
/// In the future, this can be expanded to send a full JSON-RPC `initialize` message.
pub async fn check_server_health(name: &str, config: &ServerConfig) -> Result<()> {
    println!("🩺 Testing health for MCP server: {}...", name);

    // This is a naive health check for MVP: just try to spawn it.
    // If it's a valid command, it should spawn.
    let mut child = Command::new(&config.command)
        .args(&config.args)
        .envs(&config.env)
        .spawn()?;

    // Wait a brief moment to see if it immediately exits with an error
    tokio::time::sleep(Duration::from_millis(500)).await;

    if let Ok(Some(status)) = child.try_wait() {
        if !status.success() {
            anyhow::bail!("Server {} crashed immediately with status: {}", name, status);
        }
    }

    println!("✅ Server {} is healthy (process running).", name);
    
    // Cleanup the test process
    let _ = child.kill();
    let _ = child.wait();

    Ok(())
}
