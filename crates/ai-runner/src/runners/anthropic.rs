/// Anthropic cloud REST runner — backed by the `anthropic-ai-sdk` crate.
///
/// Auth: `ANTHROPIC_API_KEY` env var, or supply via [`RunConfig::api_key`].
use std::collections::HashMap;
use std::time::Instant;

use anthropic_ai_sdk::client::AnthropicClient;
use anthropic_ai_sdk::types::message::{
    ContentBlock, ContentBlockDelta, CreateMessageParams, Message, MessageClient, MessageError,
    RequiredMessageParams, Role, StreamEvent, Tool as SdkTool, ToolChoice as SdkToolChoice,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use tracing::warn;

use crate::runner::{OnEvent, Runner};
use crate::types::{
    Event, EventKind, Provider, RunConfig, RunResult, ToolChoice, ToolDef, ToolUse,
};

const DEFAULT_MODEL: &str = "claude-opus-4-5";
const DEFAULT_MAX_TOKENS: u32 = 8096;
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicRunner;

#[async_trait]
impl Runner for AnthropicRunner {
    fn provider(&self) -> Provider {
        Provider::Anthropic
    }

    fn available(&self) -> bool {
        true // reachable if we have a key; failure surfaces at run time
    }

    async fn run(&self, cfg: RunConfig, session_id: String, on_event: OnEvent) -> RunResult {
        let mut result = RunResult {
            provider: self.provider().to_string(),
            ..Default::default()
        };

        let api_key = match cfg.api_key.clone().or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()) {
            Some(k) => k,
            None => {
                let msg = "no API key: set ANTHROPIC_API_KEY or RunConfig::api_key".to_string();
                emit_error(&on_event, &session_id, &self.provider().to_string(), msg.clone());
                result.error = Some(msg);
                return result;
            }
        };

        let client = match AnthropicClient::new::<MessageError>(api_key, ANTHROPIC_VERSION) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("client init: {e}");
                emit_error(&on_event, &session_id, &self.provider().to_string(), msg.clone());
                result.error = Some(msg);
                return result;
            }
        };

        let model = cfg.model.as_deref().unwrap_or(DEFAULT_MODEL).to_string();
        let max_tokens = cfg.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

        let mut messages: Vec<Message> = cfg
            .history
            .iter()
            .map(|m| {
                let role = if m.role == "assistant" { Role::Assistant } else { Role::User };
                Message::new_text(role, &m.content)
            })
            .collect();
        messages.push(Message::new_text(Role::User, &cfg.prompt));

        let mut params = CreateMessageParams::new(RequiredMessageParams {
            model: model.clone(),
            messages,
            max_tokens,
        })
        .with_stream(true);

        if let Some(sys) = &cfg.system_prompt {
            params = params.with_system(sys);
        }
        if !cfg.tools.is_empty() {
            params = params.with_tools(cfg.tools.iter().map(to_sdk_tool).collect());
        }
        if let Some(choice) = &cfg.tool_choice {
            params = params.with_tool_choice(to_sdk_choice(choice));
        }

        let mut stream = match client.create_message_streaming(&params).await {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("request: {e}");
                emit_error(&on_event, &session_id, &self.provider().to_string(), msg.clone());
                result.error = Some(msg);
                return result;
            }
        };

        let provider_str = self.provider().to_string();
        let start = Instant::now();
        let mut input_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;
        let mut text_buf = String::new();
        let mut error: Option<String> = None;

        // Per-content-block-index state for in-flight tool_use assembly.
        struct PendingTool {
            id: String,
            name: String,
            input_json: String,
        }
        let mut pending: HashMap<usize, PendingTool> = HashMap::new();
        let mut tool_uses: Vec<ToolUse> = Vec::new();

        while let Some(ev_result) = stream.next().await {
            match ev_result {
                Ok(ev) => match ev {
                    StreamEvent::MessageStart { message } => {
                        on_event(Event {
                            session_id: session_id.clone(),
                            provider: provider_str.clone(),
                            kind: EventKind::Connected { model: Some(model.clone()) },
                        });
                        input_tokens = message.usage.input_tokens;
                    }
                    StreamEvent::ContentBlockStart { index, content_block } => {
                        if let ContentBlock::ToolUse { id, name, input } = content_block {
                            // `input` here is usually `{}` — the real payload streams as
                            // input_json_delta. We seed with the empty-object case so a tool
                            // that takes no input still produces a valid entry on stop.
                            let seed = if input.is_null() || input == serde_json::json!({}) {
                                String::new()
                            } else {
                                input.to_string()
                            };
                            pending.insert(index, PendingTool { id, name, input_json: seed });
                        }
                    }
                    StreamEvent::ContentBlockDelta { index, delta } => match delta {
                        ContentBlockDelta::TextDelta { text } => {
                            text_buf.push_str(&text);
                            on_event(Event {
                                session_id: session_id.clone(),
                                provider: provider_str.clone(),
                                kind: EventKind::Text { content: text },
                            });
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some(p) = pending.get_mut(&index) {
                                p.input_json.push_str(&partial_json);
                            }
                        }
                        _ => {}
                    },
                    StreamEvent::ContentBlockStop { index } => {
                        if let Some(p) = pending.remove(&index) {
                            let input = if p.input_json.trim().is_empty() {
                                serde_json::json!({})
                            } else {
                                match serde_json::from_str::<serde_json::Value>(&p.input_json) {
                                    Ok(v) => v,
                                    Err(e) => {
                                        warn!(provider = "anthropic", tool = %p.name, "tool input parse: {e}");
                                        serde_json::json!({ "__parse_error": e.to_string(), "__raw": p.input_json })
                                    }
                                }
                            };
                            on_event(Event {
                                session_id: session_id.clone(),
                                provider: provider_str.clone(),
                                kind: EventKind::ToolCall { name: p.name.clone() },
                            });
                            on_event(Event {
                                session_id: session_id.clone(),
                                provider: provider_str.clone(),
                                kind: EventKind::ToolUse {
                                    id: p.id.clone(),
                                    name: p.name.clone(),
                                    input: input.clone(),
                                },
                            });
                            tool_uses.push(ToolUse { id: p.id, name: p.name, input });
                        }
                    }
                    StreamEvent::MessageDelta { usage, .. } => {
                        if let Some(u) = usage {
                            output_tokens = u.output_tokens;
                        }
                    }
                    StreamEvent::MessageStop => {
                        on_event(Event {
                            session_id: session_id.clone(),
                            provider: provider_str.clone(),
                            kind: EventKind::Done {
                                duration_ms: start.elapsed().as_millis() as u64,
                                cost_usd: 0.0,
                                input_tokens,
                                output_tokens,
                            },
                        });
                        break;
                    }
                    StreamEvent::Error { error: e } => {
                        let msg = format!("stream error: {:?}", e);
                        warn!(provider = "anthropic", "{msg}");
                        on_event(Event {
                            session_id: session_id.clone(),
                            provider: provider_str.clone(),
                            kind: EventKind::Error { message: msg.clone() },
                        });
                        error = Some(msg);
                        break;
                    }
                    _ => {}
                },
                Err(e) => {
                    let msg = format!("stream recv: {e}");
                    warn!(provider = "anthropic", "{msg}");
                    on_event(Event {
                        session_id: session_id.clone(),
                        provider: provider_str.clone(),
                        kind: EventKind::Error { message: msg.clone() },
                    });
                    error = Some(msg);
                    break;
                }
            }
        }

        result.text = text_buf;
        result.model = Some(model);
        result.duration_ms = start.elapsed().as_millis() as u64;
        result.input_tokens = input_tokens;
        result.output_tokens = output_tokens;
        result.tool_calls = tool_uses.len() as u32;
        result.tool_uses = tool_uses;
        result.error = error;
        result
    }
}

fn to_sdk_tool(t: &ToolDef) -> SdkTool {
    SdkTool {
        name: t.name.clone(),
        description: t.description.clone(),
        input_schema: t.input_schema.clone(),
    }
}

fn to_sdk_choice(c: &ToolChoice) -> SdkToolChoice {
    match c {
        ToolChoice::Auto => SdkToolChoice::Auto,
        ToolChoice::Any => SdkToolChoice::Any,
        ToolChoice::Tool { name } => SdkToolChoice::Tool { name: name.clone() },
        ToolChoice::None => SdkToolChoice::None,
    }
}

fn emit_error(on_event: &OnEvent, session_id: &str, provider: &str, message: String) {
    on_event(Event {
        session_id: session_id.to_string(),
        provider: provider.to_string(),
        kind: EventKind::Error { message },
    });
}
