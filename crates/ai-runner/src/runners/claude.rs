/// Claude Code CLI runner — backed by the `claude-wrapper` crate.
///
/// The `claude` binary manages its own authentication (`claude auth login`).
/// No API key is needed by this crate.
///
/// MCP HTTP servers with auth headers are configured via a hand-written
/// temp JSON file because `McpConfigBuilder` does not yet support HTTP headers.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use claude_wrapper::{Claude, OutputFormat, QueryCommand, streaming::stream_query};
use tracing::warn;

use crate::runner::{OnEvent, Runner};
use crate::types::{Event, EventKind, Provider, RunConfig, RunResult, ToolCallEntry};

pub struct ClaudeRunner;

fn claude_effort_prefix(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    match raw.to_ascii_lowercase().as_str() {
        "" | "off" | "none" | "disabled" => None,
        "low" => Some("Think about this before answering.".into()),
        "medium" => Some("Think hard about this before answering.".into()),
        "high" => Some("Ultrathink about this before answering.".into()),
        _ => Some("Think hard about this before answering.".into()),
    }
}

#[async_trait]
impl Runner for ClaudeRunner {
    fn provider(&self) -> Provider {
        Provider::Claude
    }

    fn available(&self) -> bool {
        // Uses the same discovery logic `run()` does, so the probe and
        // the actual call agree. Returning true here still doesn't
        // guarantee the run succeeds — auth/network can fail — but
        // false means we genuinely can't find the binary anywhere.
        discover_claude_binary().is_some()
    }

    async fn run(&self, cfg: RunConfig, session_id: String, on_event: OnEvent) -> RunResult {
        let mut result = RunResult {
            provider: self.provider().to_string(),
            ..Default::default()
        };

        // Resolve the `claude` binary via our own discovery (env override
        // → PATH → well-known install locations → editor-shipped copies)
        // and hand the absolute path to claude-wrapper. The underlying
        // `which::which("claude")` it would call on its own only finds
        // binaries on PATH, which misses modern installs that ship inside
        // editor extensions (VS Code, Cursor).
        let binary = match discover_claude_binary() {
            Some(p) => p,
            None => {
                let msg = format!(
                    "claude binary not found. Searched: CLAUDE_BINARY env, PATH, and well-known install locations (~/.local/bin, ~/.bun/bin, ~/.npm-global/bin, /opt/homebrew/bin, /usr/local/bin, ~/.vscode/extensions, ~/.cursor/extensions). Install Claude Code or set CLAUDE_BINARY=/abs/path/to/claude."
                );
                on_event(Event {
                    session_id: session_id.clone(),
                    provider: self.provider().to_string(),
                    kind: EventKind::Error { message: msg.clone() },
                });
                result.error = Some(msg);
                return result;
            }
        };

        let builder = Claude::builder().binary(binary);
        let built = match cfg.work_dir.as_deref() {
            Some(dir) => builder.build().map(|c| c.with_working_dir(dir)),
            None => builder.build(),
        };
        let claude = match built {
            Ok(c) => Arc::new(c),
            Err(e) => {
                let msg = format!(
                    "claude CLI not available: {e}. Install Claude Code and ensure `claude` is on the agent's PATH, or pick a different provider."
                );
                on_event(Event {
                    session_id: session_id.clone(),
                    provider: self.provider().to_string(),
                    kind: EventKind::Error { message: msg.clone() },
                });
                result.error = Some(msg);
                return result;
            }
        };

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

        // Claude Code CLI has no first-class thinking budget flag —
        // the documented pattern is prompt triggers ("think", "think
        // hard", "ultrathink"). Map our provider-agnostic aliases
        // onto those triggers so `thinking_budget="high"` feels the
        // same across runners.
        let prompt_with_effort = claude_effort_prefix(cfg.thinking_budget.as_deref())
            .map(|prefix| format!("{prefix}\n\n{}", cfg.prompt))
            .unwrap_or_else(|| cfg.prompt.clone());

        // Build the QueryCommand. `stream_query` consumes
        // `QueryCommand::args()` verbatim — it does NOT force
        // `--output-format stream-json` under the hood. Without it
        // the CLI emits plain text, our stream-event parser gets
        // nothing, and every run returns empty. Force it here; the
        // CLI also requires `--verbose` when combining stream-json
        // with `--print`, which claude-wrapper adds automatically.
        let mut cmd = QueryCommand::new(&prompt_with_effort).output_format(OutputFormat::StreamJson);
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

// ---------------------------------------------------------------------------
// Binary discovery
// ---------------------------------------------------------------------------

/// Resolve the `claude` binary across the installation patterns we know
/// Claude Code users hit. Order:
///
/// 1. **`CLAUDE_BINARY` env var** — explicit escape hatch.
/// 2. **`PATH` lookup** — honours the user's shell config.
/// 3. **Well-known bin dirs** — `~/.local/bin`, `~/.bun/bin`,
///    `~/.npm-global/bin`, `/opt/homebrew/bin`, `/usr/local/bin`. Also
///    scans all `~/.nvm/versions/node/*/bin/` entries.
/// 4. **Editor-shipped copies** — Anthropic publishes `claude` inside
///    the VS Code / Cursor extension (`anthropic.claude-code-<ver>-<arch>`).
///    The newest extension version wins (lexicographic sort on dir name).
fn discover_claude_binary() -> Option<PathBuf> {
    // 1. explicit override
    if let Ok(v) = std::env::var("CLAUDE_BINARY") {
        let v = v.trim();
        if !v.is_empty() {
            let p = PathBuf::from(v);
            if p.is_file() {
                return Some(p);
            }
            // Honour the user's intent even if the file is missing —
            // downstream `Claude::builder().build()` will surface the
            // concrete error, which is more useful than silently
            // falling through to PATH.
            return Some(p);
        }
    }

    // 2. PATH
    if let Some(p) = find_on_path("claude") {
        return Some(p);
    }

    // 3. Well-known bin dirs (for agents launched without a user shell).
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let static_candidates: [PathBuf; 4] = [
            home.join(".local/bin/claude"),
            home.join(".bun/bin/claude"),
            home.join(".npm-global/bin/claude"),
            home.join(".config/npm/global/bin/claude"),
        ];
        for c in &static_candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
        // nvm — any installed node version.
        if let Some(p) = scan_nvm_node_bins(&home) {
            return Some(p);
        }
        // Editor-shipped (VS Code / Cursor / vscode-server).
        for root in [
            home.join(".vscode/extensions"),
            home.join(".vscode-server/extensions"),
            home.join(".cursor/extensions"),
            home.join(".windsurf/extensions"),
        ] {
            if let Some(p) = scan_vscode_extensions(&root) {
                return Some(p);
            }
        }
    }
    for sys in ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"] {
        let p = PathBuf::from(sys);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Minimal `which(1)` replacement — no crate dep required. Iterates
/// `$PATH`, returns the first `claude` with an executable bit set.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(name);
        if is_executable_file(&full) {
            return Some(full);
        }
    }
    None
}

fn is_executable_file(p: &std::path::Path) -> bool {
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return std::fs::metadata(p)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn scan_nvm_node_bins(home: &std::path::Path) -> Option<PathBuf> {
    let root = home.join(".nvm/versions/node");
    let rd = std::fs::read_dir(&root).ok()?;
    for entry in rd.flatten() {
        let candidate = entry.path().join("bin/claude");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Scan an editor's extensions dir for the Claude Code extension's
/// shipped native binary and return the newest (lexicographic sort
/// over directory names works: they share the
/// `anthropic.claude-code-<semver>-<arch>` prefix).
fn scan_vscode_extensions(root: &std::path::Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(root).ok()?;
    let mut best: Option<(String, PathBuf)> = None;
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("anthropic.claude-code-") {
            continue;
        }
        let bin = entry.path().join("resources/native-binary/claude");
        if !is_executable_file(&bin) {
            continue;
        }
        if best.as_ref().map(|(n, _)| name > *n).unwrap_or(true) {
            best = Some((name, bin));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod discover_tests {
    use super::*;

    #[test]
    fn finds_claude_on_this_system() {
        // Integration-style sanity test — runs in the dev environment
        // and asserts the crate's own discovery logic resolves a real
        // binary. Skipped in environments that genuinely lack Claude
        // (CI without the extension installed).
        std::env::remove_var("CLAUDE_BINARY");
        match discover_claude_binary() {
            Some(p) => {
                eprintln!("discovered claude at: {}", p.display());
                assert!(p.is_file(), "discovered path {p:?} is not a file");
            }
            None => {
                eprintln!("no claude found — skipping (set CLAUDE_BINARY to force)");
            }
        }
    }
}
