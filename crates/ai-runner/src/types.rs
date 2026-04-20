use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Which AI backend to use.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// Claude Code CLI — auth managed by the `claude` binary itself.
    Claude,
    /// OpenAI Codex CLI — reads `OPENAI_API_KEY` from environment.
    Codex,
    /// Anthropic cloud REST API — key via `RunConfig::api_key` or `ANTHROPIC_API_KEY`.
    Anthropic,
    /// OpenAI cloud REST API — key via `RunConfig::api_key` or `OPENAI_API_KEY`.
    OpenAi,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
        };
        f.write_str(s)
    }
}

/// A single message in a multi-turn conversation (REST providers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMessage {
    /// `"system"`, `"user"`, or `"assistant"`.
    pub role: String,
    pub content: String,
}

/// Provider-agnostic configuration for one run.
/// Fields that don't apply to a provider are silently ignored.
#[derive(Debug, Clone, Default)]
pub struct RunConfig {
    // ── Shared ───────────────────────────────────────────────────────────
    /// The user prompt.
    pub prompt: String,
    /// Optional system prompt / context.
    pub system_prompt: Option<String>,
    /// Model override, e.g. `"claude-opus-4-5"`, `"gpt-4o"`.
    pub model: Option<String>,

    // ── CLI-only ─────────────────────────────────────────────────────────
    /// Resume a previous CLI session by its session ID.
    pub resume_id: Option<String>,
    /// MCP server URL, e.g. `http://localhost:8090/mcp`.
    pub mcp_url: Option<String>,
    /// Bearer token for MCP server auth.
    pub mcp_token: Option<String>,
    /// Tool filter pattern, e.g. `"mcp__acme__*"`.
    pub allowed_tools: Option<String>,
    /// Thinking budget: `"low"`, `"medium"`, `"high"`, or a token count.
    pub thinking_budget: Option<String>,
    /// Working directory for spawned subprocesses.
    pub work_dir: Option<String>,

    // ── REST-only ─────────────────────────────────────────────────────────
    /// API key. Falls back to the standard env var when absent.
    pub api_key: Option<String>,
    /// Base URL override (proxies, local servers).
    pub base_url: Option<String>,
    /// Pre-loaded conversation history for stateless REST providers.
    pub history: Vec<HistoryMessage>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Extra HTTP headers forwarded verbatim.
    pub extra_headers: HashMap<String, String>,

    // ── Tool calling ────────────────────────────────────────────────────
    /// Tools exposed to the model for structured output / function calling.
    /// Runners that don't support tools ignore this silently.
    pub tools: Vec<ToolDef>,
    /// How the model is allowed / required to pick a tool.
    pub tool_choice: Option<ToolChoice>,
}

/// A tool the model may invoke. Mirrors the Anthropic / OpenAI schema shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the tool's input object.
    pub input_schema: JsonValue,
}

/// Constraint on tool selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolChoice {
    /// Model decides whether to call a tool.
    Auto,
    /// Model must call some tool.
    Any,
    /// Model must call the named tool.
    Tool { name: String },
    /// Model must not call any tool.
    None,
}

/// A structured tool invocation captured from the model's output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: JsonValue,
}

/// A normalised streaming event emitted by any provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Caller-supplied identifier grouping all events for one run.
    pub session_id: String,
    /// Provider that produced this event.
    pub provider: String,
    pub kind: EventKind,
}

/// The typed payload of an [`Event`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// Backend process / HTTP stream established.
    Connected { model: Option<String> },
    /// A chunk of generated text.
    Text { content: String },
    /// The model invoked a tool (lightweight notification — name only).
    ToolCall { name: String },
    /// The model invoked a tool with structured input (REST providers).
    ToolUse {
        id: String,
        name: String,
        input: JsonValue,
    },
    /// Run finished successfully.
    Done {
        duration_ms: u64,
        cost_usd: f64,
        input_tokens: u32,
        output_tokens: u32,
    },
    /// Something went wrong.
    Error { message: String },
}

/// Records one tool invocation within a run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallEntry {
    pub name: String,
    pub duration_ms: u64,
    /// `"ok"` or `"error"`.
    pub status: String,
    pub error: Option<String>,
}

/// Aggregated result returned after a run completes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunResult {
    pub text: String,
    pub provider: String,
    pub model: Option<String>,
    /// CLI session ID for resume support (Claude runner only).
    pub session_id: Option<String>,
    pub duration_ms: u64,
    pub cost_usd: f64,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub tool_calls: u32,
    pub tool_call_log: Vec<ToolCallEntry>,
    /// Structured tool invocations captured during the run (REST providers).
    pub tool_uses: Vec<ToolUse>,
    /// Set when the run ended with a fatal error.
    pub error: Option<String>,
}
