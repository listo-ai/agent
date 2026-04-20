//! `agent ai providers` / `agent ai run` — direct access to the shared
//! AI runner registry.

use agent_client::types::{AiRunRequest, AiStreamEvent};
use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;
use futures_util::StreamExt;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum AiCmd {
    /// List registered AI providers and their availability.
    Providers,

    /// Run a one-shot prompt through the shared registry.
    Run {
        /// The user prompt.
        prompt: String,

        /// Optional system / instruction prompt.
        #[arg(long)]
        system: Option<String>,

        /// Override the default AI provider.
        /// One of `anthropic`, `openai`, `claude`, `codex`.
        #[arg(long)]
        provider: Option<String>,

        /// Override the model (e.g. `claude-opus-4-5`, `gpt-4o`).
        #[arg(long)]
        model: Option<String>,

        /// Generation cap. Falls back to the runner's default.
        #[arg(long)]
        max_tokens: Option<u32>,

        /// Extended thinking / reasoning effort.
        /// One of `low`, `medium`, `high`, `off`, or a raw integer.
        #[arg(long)]
        thinking: Option<String>,
    },

    /// Stream a prompt (`POST /api/v1/ai/stream`). Prints text deltas
    /// live; emits the final aggregated JSON on completion when
    /// `--output json` is set.
    Stream {
        /// The user prompt.
        prompt: String,

        /// Optional system / instruction prompt.
        #[arg(long)]
        system: Option<String>,

        /// Override the default AI provider.
        #[arg(long)]
        provider: Option<String>,

        /// Override the model.
        #[arg(long)]
        model: Option<String>,

        /// Generation cap.
        #[arg(long)]
        max_tokens: Option<u32>,

        /// Extended thinking / reasoning effort (`low|medium|high|off`).
        #[arg(long)]
        thinking: Option<String>,
    },
}

impl AiCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Providers => "ai providers",
            Self::Run { .. } => "ai run",
            Self::Stream { .. } => "ai stream",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &AiCmd) -> Result<()> {
    match cmd {
        AiCmd::Providers => {
            let providers = client.ai().providers().await?;
            output::ok_table(fmt, &["PROVIDER", "AVAILABLE"], &providers, |p| {
                vec![p.provider.clone(), p.available.to_string()]
            })?;
        }
        AiCmd::Run {
            prompt,
            system,
            provider,
            model,
            max_tokens,
            thinking,
        } => {
            let req = AiRunRequest {
                prompt: prompt.clone(),
                system_prompt: system.clone(),
                provider: provider.clone(),
                model: model.clone(),
                max_tokens: *max_tokens,
                thinking_budget: thinking.clone(),
            };
            let resp = client.ai().run(&req).await?;
            output::ok(fmt, &resp)?;
        }
        AiCmd::Stream {
            prompt,
            system,
            provider,
            model,
            max_tokens,
            thinking,
        } => {
            let req = AiRunRequest {
                prompt: prompt.clone(),
                system_prompt: system.clone(),
                provider: provider.clone(),
                model: model.clone(),
                max_tokens: *max_tokens,
                thinking_budget: thinking.clone(),
            };
            let mut stream = client.ai().stream(&req).await?;
            let mut final_result: Option<AiStreamEvent> = None;

            while let Some(ev) = stream.next().await {
                let ev = ev?;
                match &ev {
                    AiStreamEvent::Text { content } => {
                        // Live text goes to stdout regardless of format so
                        // `agent ai stream "..." | tee` works.
                        print!("{content}"); // NO_PRINTLN_LINT:allow
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                    AiStreamEvent::Error { message } => {
                        eprintln!("stream error: {message}"); // NO_PRINTLN_LINT:allow
                    }
                    AiStreamEvent::Result { .. } => {
                        final_result = Some(ev.clone());
                    }
                    _ => {}
                }
            }
            // Newline after live text, then emit the summary in JSON mode.
            println!(); // NO_PRINTLN_LINT:allow
            if matches!(fmt, OutputFormat::Json) {
                if let Some(result) = final_result {
                    output::ok(fmt, &result)?;
                }
            }
        }
    }
    Ok(())
}
