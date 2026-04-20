//! `POST /api/v1/ui/compose` — AI-assisted page authoring.
//!
//! Takes a natural-language prompt (plus an optional current layout
//! to edit) and returns a validated ComponentTree. The agent owns
//! the Anthropic API key via env var; the client never sees it. This
//! is the canonical AI surface — the CLI (`agent ui compose`) and
//! the Studio builder's Compose panel both call through here.
//!
//! The tool call's input_schema is the live `ui_ir::Component` JSON
//! Schema, so the model is physically unable to emit a component
//! variant we don't render.
//!
//! Current scope — MVP:
//! - non-streaming (one-shot response body),
//! - Anthropic-only backend (Claude Sonnet by default, overridable
//!   via `COMPOSE_MODEL` env),
//! - no server-side graph grounding beyond what the caller passes
//!   in `context_hints`.
//!
//! Streaming, graph auto-grounding, and multi-turn chat are
//! follow-ups that live behind this same endpoint.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use ui_ir::Component;

use crate::state::DashboardState;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
const MAX_TOKENS: u32 = 8000;
const UPSTREAM_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Deserialize)]
pub struct ComposeRequest {
    /// Natural-language instruction from the user.
    pub prompt: String,
    /// Optional current layout to edit in place. When present the
    /// model is told to treat it as the starting point, not a rewrite.
    #[serde(default)]
    pub current_layout: Option<JsonValue>,
    /// Optional free-text hints about the surrounding graph
    /// (node paths, kinds, slots) the author wants referenced. Pure
    /// pass-through — the server does not auto-enrich in this pass.
    #[serde(default)]
    pub context_hints: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ComposeResponse {
    /// Generated ComponentTree JSON — always shape-valid against the
    /// emit_layout tool's input_schema.
    pub layout: JsonValue,
    /// Free-text the model emitted alongside the tool call, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---- error shape ----------------------------------------------------------

#[derive(Debug)]
enum ComposeError {
    Unavailable(String),
    Upstream { status: StatusCode, message: String },
    BadResponse(String),
    BadRequest(String),
}

impl IntoResponse for ComposeError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, "compose_unavailable", m),
            Self::Upstream { status, message } => (
                if status.is_server_error() {
                    StatusCode::BAD_GATEWAY
                } else {
                    StatusCode::BAD_GATEWAY
                },
                "upstream_error",
                message,
            ),
            Self::BadResponse(m) => (StatusCode::BAD_GATEWAY, "upstream_error", m),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
        };
        (
            status,
            Json(serde_json::json!({ "code": code, "error": message })),
        )
            .into_response()
    }
}

// ---- handler --------------------------------------------------------------

pub async fn handler(
    State(state): State<DashboardState>,
    Json(req): Json<ComposeRequest>,
) -> Result<Json<ComposeResponse>, Response> {
    compose_inner(state, req)
        .await
        .map(Json)
        .map_err(|e| e.into_response())
}

async fn compose_inner(
    state: DashboardState,
    req: ComposeRequest,
) -> Result<ComposeResponse, ComposeError> {
    if req.prompt.trim().is_empty() {
        return Err(ComposeError::BadRequest("prompt is required".into()));
    }

    let api_key = state.ai_api_key.clone().ok_or_else(|| {
        ComposeError::Unavailable(
            "ANTHROPIC_API_KEY not set on the agent — AI compose is disabled".into(),
        )
    })?;
    let model = state
        .ai_model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let component_schema = serde_json::to_value(schemars::schema_for!(Component))
        .expect("schemars component schema is infallible");

    let body = serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system_prompt(),
        "tools": [{
            "name": "emit_layout",
            "description": "Emit the complete ui.page layout — always respond via this tool, never in prose.",
            "input_schema": {
                "type": "object",
                "required": ["ir_version", "root"],
                "properties": {
                    "ir_version": { "type": "integer" },
                    "root": component_schema,
                    "vars": {
                        "type": "object",
                        "description": "Author-declared constants referenced via {{$vars.<key>}}. Use for repeated node ids, filter literals, etc."
                    }
                }
            }
        }],
        "tool_choice": { "type": "tool", "name": "emit_layout" },
        "messages": [{ "role": "user", "content": user_message(&req) }]
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(UPSTREAM_TIMEOUT_SECS))
        .build()
        .map_err(|e| ComposeError::BadResponse(format!("http client build: {e}")))?;

    let http_resp = client
        .post(ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ComposeError::BadResponse(format!("anthropic: {e}")))?;

    let status = http_resp.status();
    if !status.is_success() {
        let text = http_resp.text().await.unwrap_or_default();
        return Err(ComposeError::Upstream {
            status,
            message: format!("anthropic HTTP {status}: {text}"),
        });
    }

    let payload: AnthropicResponse = http_resp
        .json()
        .await
        .map_err(|e| ComposeError::BadResponse(format!("decoding anthropic response: {e}")))?;

    let mut tool_input: Option<JsonValue> = None;
    let mut text_notes: Vec<String> = Vec::new();
    for block in payload.content {
        match block {
            AnthropicBlock::ToolUse { name, input } if name == "emit_layout" => {
                tool_input = Some(input);
            }
            AnthropicBlock::Text { text } => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    text_notes.push(trimmed.to_string());
                }
            }
            _ => {}
        }
    }

    let layout = tool_input.ok_or_else(|| {
        ComposeError::BadResponse("model did not call emit_layout — no layout to return".into())
    })?;

    let note = if text_notes.is_empty() {
        None
    } else {
        Some(text_notes.join("\n\n"))
    };

    Ok(ComposeResponse { layout, note })
}

// ---- prompt construction --------------------------------------------------

fn system_prompt() -> String {
    [
        "You author live dashboards stored as `ui.page.layout` nodes.",
        "Every response must invoke the `emit_layout` tool with a complete",
        "ComponentTree. Never write raw JSON in prose. Never invent",
        "component types — only use variants present in the tool's",
        "input_schema.",
        "",
        "Binding grammar:",
        "- {{$vars.<key>}} — author-declared constants on the tree.",
        "  Use this to DRY repeated ids / strings. Declare them in the",
        "  top-level `vars` field.",
        "- {{$page.<key>}} — page-local state (filter values, selected",
        "  row, chart range). Interactive components (select, toggle,",
        "  date_range) write these.",
        "- {{$stack.<alias>}} — nav-frame lookup (rare in authored pages).",
        "- {{$user.<claim>}} — verified auth claim.",
        "- {{$self.<slot>}} — slot on the page node itself.",
        "- $target.* is reserved for kind-views (`/ui/render`), never in",
        "  authored pages.",
        "",
        "Data components:",
        "- table: source.query is RSQL over node fields (path, kind,",
        "  parent_path, lifecycle). Set subscribe: true for live rows.",
        "  columns[].field is a dot path into the slot value. Source",
        "  nodes write a Msg to their output slot, so drill into the",
        "  payload: e.g. `slots.out.payload.count`, not `slots.out`.",
        "- chart, kpi, sparkline: source.{node_id, slot}. node_id must be",
        "  a real UUID the user referenced. When unsure, declare it as",
        "  a $vars key the user will fill in rather than fabricating.",
        "",
        "Quality rules:",
        "- Every component gets a stable, short, unique id.",
        "- Prefer composed pages (kpi row + filters + table) over",
        "  monolithic ones.",
        "- Do not emit deprecated or non-existent component types.",
    ]
    .join("\n")
}

fn user_message(req: &ComposeRequest) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(req.prompt.trim().to_string());
    if let Some(current) = &req.current_layout {
        parts.push(String::new());
        parts.push(
            "Current layout — edit this in place, don't rewrite unless the user asked:".into(),
        );
        parts.push("```json".into());
        parts.push(
            serde_json::to_string_pretty(current).unwrap_or_else(|_| "<unserialisable>".into()),
        );
        parts.push("```".into());
    }
    if let Some(hints) = &req.context_hints {
        let trimmed = hints.trim();
        if !trimmed.is_empty() {
            parts.push(String::new());
            parts.push("Context:".into());
            parts.push(trimmed.to_string());
        }
    }
    parts.join("\n")
}

// ---- Anthropic response shapes (subset we care about) --------------------

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBlock {
    Text {
        text: String,
    },
    ToolUse {
        name: String,
        input: JsonValue,
    },
    #[serde(other)]
    Unknown,
}
