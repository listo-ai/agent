/// Claude Code CLI runner — backed by the `claude-wrapper` crate.
///
/// The `claude` binary manages its own authentication (`claude auth login`).
/// No API key is needed by this crate.
///
/// MCP HTTP servers with auth headers are configured via a hand-written
/// temp JSON file because `McpConfigBuilder` does not yet support HTTP headers.
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use claude_wrapper::{Claude, QueryCommand, streaming::stream_query};
use tracing::warn;

use crate::runner::{OnEvent, Runner};
use crate::types::{Event, EventKind, Provider, RunConfig, RunResult, ToolCallEntry};

pub struct ClaudeRunner;

#[async_trait]
impl Runner for ClaudeRunner {
    fn provider(&self) -> Provider {
        Provider::Claude
    }

    fn available(&self) -> bool {
        std::process::Command::new("claude")
            .arg("--version")
            .output()
            .is_ok()
    }

    async fn run(&self, cfg: RunConfig, session_id: String, on_event: OnEvent) -> RunResult {
        let mut result = RunResult {
            provider: self.provider().to_string(),
            ..Default::default()
        };

        // Build the Claude client, optionally scoped to a working directory.
        let claude = Arc::new(match cfg.work_dir.as_deref() {
            Some(dir) => Claude::builder().build().map(|c| c.with_working_dir(dir)),
            None => Claude::builder().build().map(|c| c),
        }.unwrap_or_else(|_| {
            // Fallback: try default build ignoring working-dir errors.
            Claude::builder().build().expect("claude binary not found")
        }));

        // Write MCP config to a temp file if a URL is provided.
        // We write raw JSON because McpConfigBuilder doesn't support HTTP headers yet.
        let mcp_tmp_path: Option<std::path::PathBuf> = cfg.mcp_url.as_deref().and_then(|url| {
            let token = cfg.mcp_token.as_deref().unwrap_or("");
            let json = serde_json::json!({
                "mcpServers": {
                    "acme": {
                        "type": "http",
                        "url": url,
                        "headers": { "Authorization": format!("Bearer {token}") }
                    }
                }
            });
            let path = std::env::temp_dir().join(format!("ai-runner-mcp-{}.json", std::process::id()));
            std::fs::write(&path, serde_json::to_vec_pretty(&json).ok()?).ok()?;
            Some(path)
        });

        // Build the QueryCommand.
        let mut cmd = QueryCommand::new(&cfg.prompt);
        if let Some(m) = &cfg.model {
            cmd = cmd.model(m);
        }
        if let Some(sys) = &cfg.system_prompt {
            cmd = cmd.system_prompt(sys);
        }
        if let Some(resume) = &cfg.resume_id {
            cmd = cmd.resume(resume);
        }
        if let Some(tools) = &cfg.allowed_tools {
            // allowed_tools takes an iterator; split a comma-separated string.
            let tool_list: Vec<&str> = tools.split(',').map(str::trim).collect();
            cmd = cmd.allowed_tools(tool_list);
        }
        if let Some(path) = &mcp_tmp_path {
            cmd = cmd.mcp_config(path.to_string_lossy().as_ref());
        }

        let start = Instant::now();
        let on_event_clone = Arc::clone(&on_event);
        let sid = session_id.clone();
        let provider_str = self.provider().to_string();

        // Mutable state accumulated within the sync stream callback.
        let mut text_buf = String::new();
        let mut tool_calls: Vec<ToolCallEntry> = Vec::new();
        let mut tool_start: Option<(String, Instant)> = None;
        let mut cost_usd = 0.0f64;
        let mut claude_session_id: Option<String> = None;
        let mut connected = false;

        let stream_result = stream_query(&claude, &cmd, |ev| {
            let etype = ev.event_type().unwrap_or("");
            match etype {
                "system" => {
                    if let Some(s) = ev.session_id() {
                        claude_session_id = Some(s.to_string());
                    }
                    if !connected {
                        connected = true;
                        let model = ev.data["model"].as_str().map(String::from);
                        on_event_clone(Event {
                            session_id: sid.clone(),
                            provider: provider_str.clone(),
                            kind: EventKind::Connected { model },
                        });
                    }
                }
                "assistant" => {
                    if let Some(blocks) = ev.data["message"]["content"].as_array() {
                        for block in blocks {
                            match block["type"].as_str() {
                                Some("text") => {
                                    let t = block["text"].as_str().unwrap_or("").to_string();
                                    text_buf.push_str(&t);
                                    on_event_clone(Event {
                                        session_id: sid.clone(),
                                        provider: provider_str.clone(),
                                        kind: EventKind::Text { content: t },
                                    });
                                }
                                Some("tool_use") => {
                                    if let Some((name, ts)) = tool_start.take() {
                                        tool_calls.push(ToolCallEntry {
                                            name,
                                            duration_ms: ts.elapsed().as_millis() as u64,
                                            status: "ok".into(),
                                            error: None,
                                        });
                                    }
                                    let name = block["name"].as_str().unwrap_or("").to_string();
                                    on_event_clone(Event {
                                        session_id: sid.clone(),
                                        provider: provider_str.clone(),
                                        kind: EventKind::ToolCall { name: name.clone() },
                                    });
                                    tool_start = Some((name, Instant::now()));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                "result" => {
                    cost_usd = ev.cost_usd().unwrap_or(0.0);
                    on_event_clone(Event {
                        session_id: sid.clone(),
                        provider: provider_str.clone(),
                        kind: EventKind::Done {
                            duration_ms: start.elapsed().as_millis() as u64,
                            cost_usd,
                            input_tokens: 0,
                            output_tokens: 0,
                        },
                    });
                }
                _ => {}
            }
        })
        .await;

        // Close any still-open tool call.
        if let Some((name, ts)) = tool_start {
            tool_calls.push(ToolCallEntry {
                name,
                duration_ms: ts.elapsed().as_millis() as u64,
                status: "ok".into(),
                error: None,
            });
        }

        let error = match stream_result {
            Err(e) => {
                let msg = e.to_string();
                warn!(provider = "claude", "stream error: {msg}");
                on_event(Event {
                    session_id: session_id.clone(),
                    provider: self.provider().to_string(),
                    kind: EventKind::Error { message: msg.clone() },
                });
                Some(msg)
            }
            Ok(_) => None,
        };

        result.text = text_buf;
        result.session_id = claude_session_id;
        result.duration_ms = start.elapsed().as_millis() as u64;
        result.cost_usd = cost_usd;
        result.tool_calls = tool_calls.len() as u32;
        result.tool_call_log = tool_calls;
        // Clean up MCP temp file.
        if let Some(path) = mcp_tmp_path {
            let _ = std::fs::remove_file(path);
        }

        result.error = error;
        result
    }
}
