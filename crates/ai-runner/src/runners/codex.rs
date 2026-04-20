/// OpenAI Codex CLI runner.
///
/// Spawns `codex --quiet --full-auto [--model <m>] <prompt>` and streams
/// stdout as plain-text lines. The Codex CLI reads `OPENAI_API_KEY` from
/// the environment — this crate does not set it.
///
/// Install: `npm install -g @openai/codex`
use std::time::Instant;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::runner::{OnEvent, Runner};
use crate::types::{Event, EventKind, Provider, RunConfig, RunResult};

pub struct CodexRunner;

#[async_trait]
impl Runner for CodexRunner {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    fn available(&self) -> bool {
        std::process::Command::new("codex")
            .arg("--version")
            .output()
            .is_ok()
    }

    async fn run(&self, cfg: RunConfig, session_id: String, on_event: OnEvent) -> RunResult {
        let mut result = RunResult {
            provider: self.provider().to_string(),
            ..Default::default()
        };

        let model = cfg.model.clone().unwrap_or_else(|| "codex".to_string());

        let mut args = vec!["--quiet".to_string(), "--full-auto".to_string()];
        if let Some(m) = &cfg.model {
            args.extend(["--model".to_string(), m.clone()]);
        }
        args.push(cfg.prompt.clone());

        let mut cmd = tokio::process::Command::new("codex");
        cmd.args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        if let Some(dir) = &cfg.work_dir {
            cmd.current_dir(dir);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("spawn codex: {e}");
                on_event(Event {
                    session_id: session_id.clone(),
                    provider: self.provider().to_string(),
                    kind: EventKind::Error { message: msg.clone() },
                });
                result.error = Some(msg);
                return result;
            }
        };

        let stdout = child.stdout.take().expect("stdout was piped");
        let mut lines = BufReader::new(stdout).lines();
        let start = Instant::now();

        // Emit "connected" before the first line.
        on_event(Event {
            session_id: session_id.clone(),
            provider: self.provider().to_string(),
            kind: EventKind::Connected { model: Some(model.clone()) },
        });

        let mut text_buf = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            let content = format!("{line}\n");
            text_buf.push_str(&content);
            on_event(Event {
                session_id: session_id.clone(),
                provider: self.provider().to_string(),
                kind: EventKind::Text { content },
            });
        }

        let wait_result = child.wait().await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let error = match wait_result {
            Ok(status) if !status.success() => {
                let msg = format!("codex exited with code {}", status.code().unwrap_or(-1));
                on_event(Event {
                    session_id: session_id.clone(),
                    provider: self.provider().to_string(),
                    kind: EventKind::Error { message: msg.clone() },
                });
                Some(msg)
            }
            Err(e) => {
                let msg = format!("codex wait: {e}");
                on_event(Event {
                    session_id: session_id.clone(),
                    provider: self.provider().to_string(),
                    kind: EventKind::Error { message: msg.clone() },
                });
                Some(msg)
            }
            _ => {
                on_event(Event {
                    session_id: session_id.clone(),
                    provider: self.provider().to_string(),
                    kind: EventKind::Done {
                        duration_ms,
                        cost_usd: 0.0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                });
                None
            }
        };

        result.text = text_buf;
        result.model = Some(model);
        result.duration_ms = duration_ms;
        result.error = error;
        result
    }
}
