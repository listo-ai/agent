// Compose — direct Anthropic calls from the browser.
//
// MVP constraints: API key in `PUBLIC_ANTHROPIC_API_KEY`, single-shot
// non-streaming completion, tool-use forces the model to emit a
// schema-valid ComponentTree. When we add multi-user / streaming /
// prompt-caching, this moves behind a server endpoint — the shape of
// `composeLayout(req)` stays the same.
//
// The tool's input_schema is our live /ui/vocabulary schema, so the
// model is physically unable to emit a component type we don't know
// how to render. Dry-run still validates bindings after the fact.

import { agentPromise } from "@/lib/agent";

const ANTHROPIC_URL = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION = "2023-06-01";
const MODEL = "claude-sonnet-4-6";
const MAX_TOKENS = 8000;

const env = (import.meta as unknown as { env: Record<string, string | undefined> }).env;
const API_KEY = env["PUBLIC_ANTHROPIC_API_KEY"] ?? "";

export function hasApiKey(): boolean {
  return API_KEY.length > 0;
}

export interface ComposeRequest {
  /** Natural-language instruction from the user. */
  prompt: string;
  /** Serialised current layout JSON (if editing an existing page). */
  currentLayout?: string;
  /** Optional short description of the surrounding graph state. */
  contextHints?: string;
}

export interface ComposeResult {
  layout: unknown;
  /** Free-text explanation the model chose to emit alongside the tool call. */
  note?: string;
}

/**
 * One-shot generation. Throws on transport / API / schema failure —
 * callers surface the message to the user.
 */
export async function composeLayout(req: ComposeRequest): Promise<ComposeResult> {
  if (!API_KEY) {
    throw new Error(
      "PUBLIC_ANTHROPIC_API_KEY not set — configure it in the frontend env to use Compose.",
    );
  }

  const vocabulary = await fetchVocabularySchema();

  const systemPrompt = buildSystemPrompt();
  const userMessage = buildUserMessage(req);

  const body = {
    model: MODEL,
    max_tokens: MAX_TOKENS,
    system: systemPrompt,
    tools: [
      {
        name: "emit_layout",
        description:
          "Emit the complete ui.page layout — an object with `ir_version` and a `root` ComponentTree. Always use this tool to respond; never write raw JSON in prose.",
        input_schema: {
          type: "object",
          required: ["ir_version", "root"],
          properties: {
            ir_version: { type: "integer" },
            root: vocabulary,
          },
        },
      },
    ],
    tool_choice: { type: "tool", name: "emit_layout" },
    messages: [{ role: "user", content: userMessage }],
  };

  const resp = await fetch(ANTHROPIC_URL, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-api-key": API_KEY,
      "anthropic-version": ANTHROPIC_VERSION,
      // Opt-in header required when hitting the Anthropic API directly
      // from a browser. Without it CORS preflight rejects.
      "anthropic-dangerous-direct-browser-access": "true",
    },
    body: JSON.stringify(body),
  });

  if (!resp.ok) {
    const text = await resp.text().catch(() => resp.statusText);
    throw new Error(`Anthropic HTTP ${resp.status}: ${text}`);
  }

  const payload = (await resp.json()) as AnthropicResponse;
  const toolUse = payload.content.find(
    (c): c is AnthropicToolUseBlock =>
      c.type === "tool_use" &&
      (c as AnthropicToolUseBlock).name === "emit_layout",
  );
  if (!toolUse) {
    throw new Error("Model did not call emit_layout — nothing to apply.");
  }
  const note = payload.content
    .filter((c): c is AnthropicTextBlock => c.type === "text")
    .map((c) => c.text.trim())
    .filter(Boolean)
    .join("\n\n");

  const result: ComposeResult = { layout: toolUse.input };
  if (note) result.note = note;
  return result;
}

async function fetchVocabularySchema(): Promise<unknown> {
  const client = await agentPromise;
  const vocab = await client.ui.vocabulary();
  // The schemars-emitted schema at `.schema` is the Component union.
  return vocab.schema;
}

function buildSystemPrompt(): string {
  return [
    "You author live dashboards stored as `ui.page.layout` nodes.",
    "Every response must invoke the `emit_layout` tool with a complete",
    "ComponentTree. Never write raw JSON in prose. Never invent component",
    "types — only use variants present in the tool's input_schema.",
    "",
    "Guidelines:",
    "- Bindings: use {{$page.<key>}} for page-local state, {{$stack.<alias>}} for nav frames, {{$user.<claim>}} for auth, {{$self.<slot>}} for the page's own slots.",
    "- $target.* only exists on kind-views (`/ui/render`), never on authored pages.",
    "- Tables: `source.query` is RSQL over node fields (path, kind, parent_path). Set `subscribe: true` for live rows. `columns[].field` is a dot path; flow-engine envelopes store real values at `.payload` — prefer flattened status slots (e.g. `slots.current_count`) over envelope slots (`slots.count.payload`).",
    "- Charts/KPIs: `source.node_id` must be a real UUID the user has mentioned or one the caller is referencing; when the user has not named a node, ask with a text component rather than fabricating an id.",
    "- Keep component ids stable, short, and unique (e.g. `hb-table`, `range`, `kind-filter`).",
    "- Prefer small composed layouts (kpi row + filters + table) over monolithic pages.",
  ].join("\n");
}

function buildUserMessage(req: ComposeRequest): string {
  const parts: string[] = [req.prompt.trim()];
  if (req.currentLayout && req.currentLayout.trim()) {
    parts.push(
      "",
      "Current layout (edit this — don't replace wholesale unless the user asked):",
      "```json",
      req.currentLayout.trim(),
      "```",
    );
  }
  if (req.contextHints && req.contextHints.trim()) {
    parts.push("", "Context:", req.contextHints.trim());
  }
  return parts.join("\n");
}

// ---- Anthropic response shapes (subset we care about) ---------------------

interface AnthropicResponse {
  content: AnthropicContentBlock[];
}
type AnthropicContentBlock = AnthropicTextBlock | AnthropicToolUseBlock | { type: string };
interface AnthropicTextBlock {
  type: "text";
  text: string;
}
interface AnthropicToolUseBlock {
  type: "tool_use";
  name: string;
  input: unknown;
}
