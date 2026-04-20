use crate::config::ServerConfig;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub fn sync_agent_config(
    agent_name: &str,
    format: &str,
    path: &str,
    servers: &HashMap<String, ServerConfig>,
) -> Result<()> {
    println!("🔄 Syncing {} config at {} (format: {})", agent_name, path, format);

    // Resolve ~ to home directory using shellexpand
    let resolved_path = shellexpand::tilde(path).into_owned();
    let path_buf = PathBuf::from(resolved_path);
    
    // Read existing JSON or create an empty object
    let mut root: Value = if path_buf.exists() {
        let content = fs::read_to_string(&path_buf)?;
        serde_json::from_str(&content).unwrap_or(json!({}))
    } else {
        if let Some(parent) = path_buf.parent() {
            fs::create_dir_all(parent)?;
        }
        json!({})
    };

    let servers_json = serde_json::to_value(servers)?;

    // Safely inject based on format
    match format {
        "vscode" => {
            // Support both popular VS Code MCP extensions
            root["rooveterinaryinc.roo-cline.mcpServers"] = servers_json.clone();
            root["saoudrizwan.claude-dev.mcpServers"] = servers_json;
        }
        "standard" | _ => {
            root["mcpServers"] = servers_json;
        }
    }

    let out = serde_json::to_string_pretty(&root)?;
    fs::write(&path_buf, out).context(format!("Failed to write updated config for {}", agent_name))?;
    
    println!("✨ Successfully updated {}", agent_name);
    Ok(())
}
