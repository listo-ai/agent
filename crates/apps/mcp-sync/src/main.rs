mod config;
mod formatters;
mod health;
mod sync;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;

#[derive(Parser)]
#[command(name = "mcp-sync", about = "Syncs MCP servers to multiple agent configs")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync mcp-compose.yaml to all agents
    Sync {
        /// Path to the mcp-compose.yaml file
        #[arg(short, long, default_value = "mcp-compose.yaml")]
        file: String,
    },
    /// Test health of all servers in mcp-compose.yaml
    Test {
        #[arg(short, long, default_value = "mcp-compose.yaml")]
        file: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Sync { file } => {
            let content = fs::read_to_string(file).context("Could not read config file")?;
            let compose: config::McpCompose = serde_yml::from_str(&content).context("Invalid YAML format")?;
            sync::sync_all(&compose)?;
        }
        Commands::Test { file } => {
            let content = fs::read_to_string(file).context("Could not read config file")?;
            let compose: config::McpCompose = serde_yml::from_str(&content).context("Invalid YAML format")?;
            for (name, server) in &compose.servers {
                health::check_server_health(name, server).await?;
            }
        }
    }

    Ok(())
}
