# AI subsystem

How the platform talks to large language models — from the Rust
runner crate at the bottom, through the REST surface and clients, up
to the Studio chat panel and the flow-level `sys.ai.run` node.

Related docs: [NEW-API.md](NEW-API.md) (the 5-touchpoint rule),
[EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) (why AI is also a node
kind), [MCP.md](MCP.md) (how external tools reach in to drive flows).

---

## Why this exists

One AI surface, two consumption paths:

1. **Direct** — the agent calls a provider itself (compose page layouts,
   CLI one-shots, Studio chat panel). Used when the agent is the actor.
2. **Decentralised** — a flow contains an `sys.ai.run` node that fires
   on a pulse. Used when AI is *part of* the user's automation.

Both paths go through a single `ai_runner::Registry`, so adding a new
provider lights up in every consumer. No per-consumer SDK copies, no
hand-rolled HTTP clients, no secrets sprinkled across crates.

Non-goals: chains, agents, memory, RAG. Those are user-space concerns
authored as flows.

---

## System diagram

```
┌───────────────────────────────────────────────────────────────────┐
│                           Studio                                  │
│  ┌────────────────────────────────────────────────────────────┐   │
│  │  Global AI Chat  (frontend/src/features/global-ai-chat/)   │   │
│  │    ↓ uses ↓                                                │   │
│  │  Chat UI lib  (frontend/src/lib/chat/)                     │   │
│  │    ↓ uses ↓                                                │   │
│  │  @sys/agent-client · AgentClient.ai.{providers,run,stream} │   │
│  └────────────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────┬───────────────┘
                                                    │ HTTP / SSE
┌───────────────────────────────────────────────────┴───────────────┐
│                           Agent                                   │
│  ┌────────────────────────────────────────────────────────────┐   │
│  │  REST  (crates/transport-rest/src/ai.rs)                   │   │
│  │    GET  /api/v1/ai/providers                               │   │
│  │    POST /api/v1/ai/run      · one-shot JSON                │   │
│  │    POST /api/v1/ai/stream   · SSE                          │   │
│  │                                                            │   │
│  │  Dashboard compose (crates/dashboard-transport/src/        │   │
│  │    compose.rs)                                             │   │
│  │    POST /api/v1/ui/compose  · tool-call → ComponentTree    │   │
│  │                                                            │   │
│  │  Flow node (crates/domain-ai/)                             │   │
│  │    sys.ai.run kind                                         │   │
│  │                        ↓ all three consume ↓               │   │
│  │  ┌──────────────────────────────────────────────────────┐  │   │
│  │  │  ai-runner · Arc<Registry>                           │  │   │
│  │  │    Provider → Arc<dyn Runner>                        │  │   │
│  │  │      · AnthropicRunner   (REST, anthropic-ai-sdk)    │  │   │
│  │  │      · OpenAiRunner      (REST, async-openai)        │  │   │
│  │  │      · ClaudeRunner      (CLI, claude-wrapper)       │  │   │
│  │  │      · CodexRunner       (CLI, tokio::process)       │  │   │
│  │  └──────────────────────────────────────────────────────┘  │   │
│  └────────────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────────┘
```

---

## Layer 1 — `ai-runner` crate

Location: [crates/ai-runner/](../../crates/ai-runner/)

### The trait

```rust
#[async_trait]
pub trait Runner: Send + Sync {
    fn provider(&self) -> Provider;
    fn available(&self) -> bool;
    async fn run(&self, cfg: RunConfig, session_id: String, on_event: OnEvent) -> RunResult;
}
```

One trait covers four backends. `available()` is a fast check (CLI
runners probe `PATH`; REST runners always return `true` and surface
key-missing errors at run time). `run()` streams events through the
`OnEvent` callback and returns an aggregated `RunResult` when done.

### The registry

```rust
let registry = Arc::new(ai_runner::Registry::with_defaults());
let runner = registry.get(&Provider::Claude).unwrap();
let result = runner.run(cfg, "session-1".into(), Arc::new(|ev| { /* ... */ })).await;
```

`Registry::with_defaults()` pre-loads all four runners. Downstream
code holds `Arc<Registry>` and selects at run time. Adding a provider
= one `registry.register(Arc::new(MyRunner))` call.

### `RunConfig` at a glance

| Field                 | Applies to     | Notes |
|-----------------------|----------------|-------|
| `prompt`              | all            | Required |
| `system_prompt`       | all            | Stitched into provider-specific slot |
| `model`               | all            | Runner default if `None` |
| `history`             | REST           | Pre-loaded `HistoryMessage` chain |
| `api_key`             | REST           | Falls back to env var |
| `max_tokens`          | REST           | Runner-specific default |
| `tools` / `tool_choice` | Anthropic REST | Schema-enforced function calling |
| `thinking_budget`     | Anthropic REST, Claude CLI | `low` / `medium` / `high` / `off` / raw integer |
| `resume_id`, `mcp_url`, `mcp_token`, `allowed_tools`, `work_dir` | CLI | Claude-specific |

Fields that don't apply to the selected runner are silently ignored.

### Events

`EventKind` is the normalised streaming shape:

| Variant       | When |
|---------------|------|
| `Connected`   | Provider acknowledges the request. Carries the model name. |
| `Text`        | Incremental text delta. |
| `ToolCall`    | Model started a tool invocation (name only — lightweight notification). |
| `ToolUse`     | Structured tool invocation with parsed `input` JSON (Anthropic REST only today). |
| `Done`        | Terminal success. Carries tokens + cost + duration. |
| `Error`       | Fatal error — streaming halts after this frame. |

The terminal `Done` / `Error` arrives through the callback *and* the
returned `RunResult`. Consumers pick whichever shape fits — the REST
SSE endpoint pumps the callback stream; the compose endpoint inspects
only `RunResult.tool_uses`.

### Provider specifics

| Provider    | Backend                         | Auth                        | Thinking |
|-------------|---------------------------------|-----------------------------|----------|
| `Claude`    | `claude` CLI via claude-wrapper | `claude auth login` (no key in-process) | Prompt trigger (`think` / `think hard` / `ultrathink`) |
| `Codex`     | `codex` CLI via tokio::process  | `OPENAI_API_KEY` in env     | Not supported |
| `Anthropic` | anthropic-ai-sdk 0.2.x          | `ANTHROPIC_API_KEY` or `RunConfig::api_key` | `with_thinking { budget_tokens }` |
| `OpenAi`    | async-openai 0.35.x             | `OPENAI_API_KEY` or `RunConfig::api_key` | Not supported |

---

## Layer 2 — REST surface

Location: [crates/transport-rest/src/ai.rs](../../crates/transport-rest/src/ai.rs)

Three endpoints, one `ApiError` shape, keys held only in `AppState`
(not in request bodies). All three return `503 ai_unavailable` when
the registry was not wired at startup (tests, minimal profiles).

### `GET /api/v1/ai/providers`

Lists every registered runner with its `available()` flag. Lets the
Studio picker render a live dropdown instead of hard-coding
`["anthropic", "openai", ...]`.

Response:

```json
[
  { "provider": "anthropic", "available": true },
  { "provider": "claude",    "available": true },
  { "provider": "codex",     "available": false },
  { "provider": "openai",    "available": true }
]
```

### `POST /api/v1/ai/run`

Non-streaming one-shot. Body:

```json
{
  "prompt": "…",
  "system_prompt": "…",
  "provider": "claude",
  "model":    "claude-opus-4-5",
  "max_tokens": 8000,
  "thinking_budget": "high"
}
```

Response: `{ text, provider, model, input_tokens, output_tokens, duration_ms }`.

### `POST /api/v1/ai/stream`

Same body shape. Returns `text/event-stream`. Each frame is a
discriminated-union JSON payload, with the `event:` line carrying the
tag for filtering consumers that want to read event names without
parsing:

```
event: connected
data: {"type":"connected","model":"claude-opus-4-5"}

event: text
data: {"type":"text","content":"Hello"}

event: text
data: {"type":"text","content":", world"}

event: done
data: {"type":"done","duration_ms":812,"cost_usd":0.004,"input_tokens":42,"output_tokens":18}

event: result
data: {"type":"result","text":"Hello, world","provider":"anthropic",…}
```

Guarantees:

1. Exactly one `result` frame ends every stream — **even on errors**
   (which emit `error` first, then `result`). Clients only need one
   terminator.
2. Unknown event types are ignored by the reference clients
   (forward-compat for new variants).
3. Keep-alive pings fire every 15 s.

### `POST /api/v1/ui/compose` (dashboard-transport)

The schema-enforced sibling. Uses `RunConfig.tools` + `tool_choice` to
force the model to call `emit_layout`, then returns the tool's
structured `input` as the page layout. Today Anthropic-only because
it's the runner with tool-call plumbing; the trait is ready for
OpenAI's function calling when that support lands.

### `AppState` wiring

```rust
app_state = app_state.with_ai(Arc::new(Registry::with_defaults()), AiDefaults {
    provider: Some(Provider::Anthropic),
    anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
    openai_api_key:    std::env::var("OPENAI_API_KEY").ok(),
    model:             std::env::var("COMPOSE_MODEL").ok(),
});
```

Single registry instance is shared by `/ai/*`, `/ui/compose`, and
`domain_ai::runtime::init(...)` so the flow runtime uses the same
credentials. See [crates/apps/agent/src/main.rs](../../crates/apps/agent/src/main.rs).

---

## Layer 3 — Clients (5-touchpoint per [NEW-API.md](NEW-API.md))

### Rust — `agent-client`

```rust
let providers = client.ai().providers().await?;
let resp      = client.ai().run(&req).await?;
let mut stream = client.ai().stream(&req).await?;
while let Some(ev) = stream.next().await { /* ... */ }
```

File: [clients/rs/src/ai.rs](../../clients/rs/src/ai.rs). SSE parsing
is hand-rolled (no extra dep) in `sse::parse` — multi-line `data:`,
LF/CRLF tolerant, forward-compatible with unknown frames.

DTOs (mirror REST 1:1): `AiProviderStatus`, `AiRunRequest`,
`AiRunResponse`, `AiStreamEvent` in
[clients/rs/src/types.rs](../../clients/rs/src/types.rs).

### TypeScript — `@sys/agent-client`

```ts
const providers = await client.ai.providers()
const resp      = await client.ai.run(req)

for await (const ev of client.ai.stream(req, { signal })) {
  if (ev.type === 'text')   bubble.append(ev.content)
  if (ev.type === 'result') finalise(ev)
}
```

Streaming uses `fetch` + `ReadableStream` + `TextDecoder`; the
reusable helper is [clients/ts/src/transport/sse-post.ts](../../clients/ts/src/transport/sse-post.ts)
(POST-based SSE — the built-in `EventSource` is GET-only). Zod
schemas enforce shape at every boundary, including the streaming
payloads.

**Fleet scope:** `client.ai.run()` / `providers()` work over fleet
transport. `client.ai.stream()` currently throws when the client was
constructed with `FleetScope.remote(…)` — fleet SSE is a follow-up.

### CLI

Three subcommands under `agent ai`:

```
agent ai providers                              # list + availability
agent ai run     "..." [--provider X --model Y --thinking high]
agent ai stream  "..." [same flags]             # text streams to stdout
```

File: [crates/transport-cli/src/commands/ai.rs](../../crates/transport-cli/src/commands/ai.rs).
Meta entries in `commands/meta.rs`.

---

## Flow-level: `sys.ai.run` node

Location: [crates/domain-ai/](../../crates/domain-ai/)

Fires on a pulse at its `trigger` input. Reads prompt / provider /
model from settings (with per-message `msg_overrides`), dispatches
through the shared `Arc<Registry>` (installed via
`domain_ai::runtime::init` at agent startup), and emits on completion:

| Slot | Role   | Carries |
|------|--------|---------|
| `trigger`      | input  | Any payload — the pulse that kicks the run. |
| `text`         | output | Final response body (pulse). |
| `done`         | output | `{ ok, text, error, input_tokens, output_tokens, duration_ms, provider, model }` (pulse). |
| `running`      | status | `true` while the run is in flight. |
| `last_error`   | status | `null` on success, error string on failure. |
| `input_tokens` / `output_tokens` / `duration_ms` | status | Post-run metrics. |

Because `Runner::run` is async and `NodeBehavior::on_message` is
synchronous, the behavior **clones** the graph + emit-sink `Arc`s off
`NodeCtx` and `tokio::spawn`s the run. The graph-writing seam was
added for exactly this case — see
[crates/blocks-sdk/src/ctx.rs](../../crates/blocks-sdk/src/ctx.rs)
`NodeCtx::graph()` / `emit_sink()`.

Streaming deltas are currently dropped (the `on_event` callback is a
no-op). Exposing a `chunk` output or a status slot that accumulates
deltas is the obvious next step.

### Why a global (and not DI)

The engine's `NodeCtx` has no extension slot for arbitrary services
(by design — everything is a node, and sharing is through slots).
`domain_ai::runtime` is a process-wide `OnceLock<(Arc<Registry>,
AiDefaults)>` seeded once at startup. It's pragmatic: the alternative
is threading the registry through every flow-context API for one
domain crate, which doesn't earn its weight until a second consumer
needs DI.

---

## Studio — the user-facing chat

### The reusable chat library

Location: [frontend/src/lib/chat/](../../frontend/src/lib/chat/)

Stateless, provider-agnostic UI primitives — tightened for this
project's `exactOptionalPropertyTypes`:

| Component / hook     | Purpose |
|----------------------|---------|
| `ChatBubble`         | User / assistant / system rendering, tool-call badges, streaming cursor |
| `ChatInput`          | Auto-resizing textarea, attachments (drag/paste/file-picker), header+footer slots |
| `ChatSuggestions`    | Empty-state pill grid for onboarding prompts |
| `Markdown`           | react-markdown + remark-gfm + Prism syntax highlighting |
| `CommandPicker`      | `/` slash-command menu (opt-in) |
| `ContextPicker`      | `@` mention chip list (opt-in) |
| `AttachmentPreview`  | Thumbnails + inline renderers |
| `CopyButton` / `useCopy` | Clipboard utility |
| `useAutoScroll` / `useAutoResize` | Small DOM hooks |
| `useFileAttach`      | File attachment state machine (drag/drop/paste) |
| `useAiChat`          | Opinionated driver wired to `AgentClient.ai.stream` — see below |

All types (`ChatMessage`, `ChatRole`, `MessageStatus`,
`MessageAttachment`) are defined **inside the lib** — the lib does
not import from consuming features. The lib is reusable in any
Studio surface; the global chat is one consumer.

### `useAiChat` — the opinionated driver

```ts
const { messages, isStreaming, send, clear, cancel } = useAiChat({
  client,
  systemPrompt,
  provider,     // optional; falls back to agent default
  model,        // optional; runner default
  thinkingBudget,
  streaming: true,  // default; set false to use ai.run() instead
})
```

Manages `ChatMessage[]`, pumps SSE frames into the last assistant
bubble (`thinking → streaming → done`), captures `tool_call` names
into `message.toolCalls`, surfaces errors as system bubbles, and
exposes an `AbortController` via `cancel()` so component unmount
cancels the request.

### Global AI chat

Location: [frontend/src/features/global-ai-chat/](../../frontend/src/features/global-ai-chat/)

One floating panel, available on every route, context-aware.

```
  ┌────────────────────────────────────────────────┐
  │ ✨ AI ASSISTANT                                │
  ├────────────────────────────────────────────────┤
  │ 📍 CONTEXT THE AI WILL SEE                     │
  │   route       /pages/4abdc…/edit               │
  │   page id     4abdc…                           │
  │   path        /pages/hb-simple                 │
  │   components  7                                │
  │   title       HB simple                        │
  │   add hints                                    │
  ├────────────────────────────────────────────────┤
  │ 🖥 Runner   claude · think high  ▼             │
  ├────────────────────────────────────────────────┤
  │  …chat messages…                               │
  │                                                │
  ├────────────────────────────────────────────────┤
  │ [ Ask about this view…              ] [▲]     │
  └────────────────────────────────────────────────┘
```

#### How the pieces plug together

```
useLocation  ──▶ useChatContextSync  ──▶ store.setContext
                                                │
                                                ▼
[SiteHeader ✨] ──▶ store.toggle()
                                                │
                                                ▼
     Sheet opens → ContextStrip + RunnerSettings + ChatPane
                                                │
             useContextData (fetch page/flow snapshot)
                                                │
                    buildSystemPrompt(ctx, hints + appendix)
                                                │
                                                ▼
                         useAiChat({ client, systemPrompt,
                                     provider, model,
                                     thinkingBudget })
                                                │
                                                ▼
                                 AgentClient.ai.stream (SSE)
```

#### Files in play

| File | Role |
|------|------|
| [context.ts](../../frontend/src/features/global-ai-chat/context.ts) | `ChatContext` discriminated union + `parseRoute()` — maps a route path to a typed context |
| [use-context-sync.ts](../../frontend/src/features/global-ai-chat/use-context-sync.ts) | Subscribes to `useLocation`, keeps the store in sync |
| [use-context-data.ts](../../frontend/src/features/global-ai-chat/use-context-data.ts) | Per-context enrichment (fetches the page node / flow node on change) |
| [prompt.ts](../../frontend/src/features/global-ai-chat/prompt.ts) | Builds the system prompt from `(ctx, hints + promptAppendix)` |
| [store.ts](../../frontend/src/features/global-ai-chat/store.ts) | Zustand store: ephemeral `open`/`context`; persisted `provider`/`model`/`thinkingBudget`/`extraHints` |
| [RunnerSettings.tsx](../../frontend/src/features/global-ai-chat/RunnerSettings.tsx) | Collapsible provider/model/effort picker; reads providers from `/ai/providers` |
| [GlobalAiChat.tsx](../../frontend/src/features/global-ai-chat/GlobalAiChat.tsx) | The `Sheet` composition: ContextStrip + RunnerSettings + ChatPane |

#### Route → context mapping

| Route                                  | `ChatContext`                                |
|----------------------------------------|----------------------------------------------|
| `/` or `/flows`                        | `flows_list`                                 |
| `/flows/edit/flow-1`                   | `flow_edit { flowPath: "/flow-1" }`          |
| `/flows/edit/flow-1/add`               | `flow_edit { flowPath: "/flow-1", nodePath: "add" }` |
| `/pages`                               | `pages_list`                                 |
| `/pages/:id/edit`                      | `page_edit { pageId }`                       |
| `/ui/:pageRef`                         | `page_view { pageRef }`                      |
| `/render/:targetId`                    | `render_view { targetId }`                   |
| `/blocks` / `/settings`                | `blocks` / `settings`                        |
| anything else                          | `unknown { path }`                           |

#### Context enrichment

`useContextData(ctx)` does a `useQuery` per context kind and returns:

```ts
{
  chips:         [{ key: 'path', value: '/pages/hb-simple' }, ...],
  promptAppendix: string | undefined,
  loading:        boolean,
  error:          string | undefined,
}
```

- `page_edit` fetches the `ui.page` node, counts components, pulls the
  title, includes the full layout JSON (truncated to 4 KB) in the
  prompt appendix.
- `flow_edit` fetches the flow node snapshot.
- Other variants render only the route chips.

The **context strip in the UI is a promise** — anything shown as a
chip *is* in the prompt. Nothing hidden.

#### Runner settings

Persisted choices (`provider`, `model`, `thinkingBudget`) flow into
`useAiChat`, which only includes them in the request body when
non-default — so users who never open the picker still get the agent's
default (`AiDefaults.provider` + env keys).

The effort chip row maps `low` / `medium` / `high` to runner-specific
behaviour:

| Provider   | `low`         | `medium`        | `high`           |
|------------|---------------|-----------------|------------------|
| Anthropic  | 1024 budget   | 4096 budget     | 16384 budget     |
| Claude CLI | `"Think…"`    | `"Think hard…"` | `"Ultrathink…"`  |
| OpenAI     | — (ignored)   | —               | —                |
| Codex      | — (ignored)   | —               | —                |

#### Trigger placement

The ✨ button lives in [SiteHeader.tsx](../../frontend/src/components/layout/SiteHeader.tsx)
— there is no floating FAB, and per-feature Compose buttons were
deleted (the page-builder's old `ComposePanel` is gone). Single
entry point, everywhere.

---

## Extending

### Add a new provider

1. Implement `Runner` in [crates/ai-runner/src/runners/](../../crates/ai-runner/src/runners/).
2. `pub mod my_runner;` in `mod.rs`.
3. Register in `Registry::with_defaults()`.
4. Add a `Provider` variant + `impl Display` entry.
5. That's it — the REST endpoints, Rust client, TS client, CLI, and
   Studio picker pick it up automatically.

### Add a new REST endpoint

Follow [NEW-API.md](NEW-API.md) — the five touchpoints (handler →
routes → Rust client → TS client → CLI → fixtures) gate merge.

### Add a new context variant in the global chat

1. New variant in `context.ts`'s `ChatContext` union.
2. Match arm in `parseRoute()`.
3. Match arm in `prompt.ts`'s `contextSection()`.
4. Optional: match arm in `use-context-data.ts` for enrichment.
5. Optional: suggestions in `GlobalAiChat.defaultSuggestions`.

Type system guides you through every site — missing arms surface as
compile errors.

### Add a new chat driver

`useAiChat` is one implementation of *drive the chat lib*. If you need
e.g. multi-turn over MCP, or offline canned responses, write a sibling
hook that returns the same `{ messages, isStreaming, send, clear }`
shape. The `ChatBubble` + `ChatInput` components don't care which
driver feeds them.

---

## Known limitations / follow-ups

- **Tools only on Anthropic REST.** OpenAI's function-calling is wired
  through the SDK but not yet exposed via `RunConfig.tools` on
  `OpenAiRunner`. Low-risk addition when a flow demands it.
- **Fleet SSE** (`client.ai.stream` over `FleetScope.remote`) throws —
  needs a fleet streaming frame + subject convention.
- **`sys.ai.run` drops text deltas.** The node's `OnEvent` is a no-op.
  Either emit per-chunk on a new `chunk` output, or accumulate into a
  status slot for live-bind UIs.
- **Page renderer enrichment.** `page_view` (`/ui/:ref`) shows only the
  route — resolved tree is not fetched yet. Would let the AI explain
  what a user actually sees.
- **"Ask AI about this node" inline triggers.** Any panel can call
  `useGlobalAiChat.getState().setContext(...)` + `setOpen(true)` to
  open the chat pre-loaded on a narrower context. Not wired yet.
- **MCP.** Exposing the agent's graph/flow surface over MCP so external
  Claude Desktop / Cursor sessions can drive flows is its own track;
  see [MCP.md](MCP.md).

---

## Related decisions

- Why not just call Anthropic/OpenAI direct from every consumer? → One
  place to add providers, one place to hold keys, one place to swap
  SDKs when they break. The registry pays for itself the second time
  anyone reaches for it.
- Why is the chat *in the header* and not a FAB? → One global entry,
  per-feature Compose buttons get stale and fragment the surface. The
  header is always visible and the hotkey story lives there.
- Why is `thinking_budget` a *string*? → Uniform across CLI aliases
  (`low`/`medium`/`high`) and raw integer token counts. Per-runner
  parsing keeps the wire shape flat.
- Why global AI chat and not per-route panels? → Same reason as the
  header: one surface, context-aware. The route-parser + enrichment
  pair covers the "per-route panel" case without duplicating shells.
