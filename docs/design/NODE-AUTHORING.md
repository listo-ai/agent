# Node Authoring

How to build a node kind — its settings, its slots, how it interacts with the msg envelope at runtime, and the settings-vs-msg-override pattern every non-trivial node eventually needs.

For the platform-wide node model (containment, facets, cascading delete, the unified graph) see [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md). This doc is the practical companion: "I'm writing a node. What do I declare, and how does it behave when messages flow through it?"

## Anatomy of a node kind

A kind is declared by a manifest plus some code. The manifest carries the contract; the code carries the behaviour.

| Thing | What it declares | Where it lives |
|---|---|---|
| **Kind ID** | Reverse-DNS identifier (`acme.io.http.client`) | Manifest |
| **Facets** | `isProtocol`, `isCompute`, `isIO`, etc. | Manifest |
| **Containment** | What parents this kind can live under, what children it can hold | Manifest |
| **Slots** | Typed ports — inputs, outputs, config, status | Manifest (JSON Schema per slot type) |
| **Settings schema** | JSON Schema for `config`-role slots; single or multi-variant | Manifest |
| **Trigger policy** | `on_any`, `on_all`, `on_specific` | Manifest |
| **Msg overrides** | Which `config` settings can be overridden by incoming msg fields, and which msg field maps to which setting | Manifest |
| **Runtime behaviour** | What the node does when it fires | Code (Rust / Wasm / extension process / QuickJS) |

Everything except the code is data — declarative, serializable, introspectable. Studio reads the manifest to render the palette entry, the property panel, and the validation rules. OpenAPI reads it to describe the API. The engine reads it to enforce containment, trigger policy, and override resolution.

## The msg envelope — quick reference

What travels on a wire when a slot's type is `Msg`. Full spec in [EVERYTHING-AS-NODE.md § "Wires, ports, and messages"](EVERYTHING-AS-NODE.md). This is the wire-level JSON; it's Node-RED-compatible by design.

| Field | Type | Semantics | Node-RED equivalent |
|---|---|---|---|
| `payload` | any | Primary data | `msg.payload` |
| `topic` | `string?` | Routing / grouping hint | `msg.topic` |
| `_msgid` | string (UUID) | Unique message id | `msg._msgid` |
| `_parentid` | string (UUID)? | Parent message id for provenance across fan-out | — (ours) |
| `_ts` | integer (ms) | Creation timestamp | — (ours) |
| `_source` | string? | Emitting node's graph path | — (ours) |
| `<anything else>` | any | User-added custom field; flattened at root | Any non-underscore field on `msg` |

Underscore-prefixed fields are platform-reserved. User fields sit at the root alongside `payload` / `topic` — same shape as Node-RED, so porting logic that does `msg.myField = ...` works unchanged.

Under the hood messages are **immutable** — a node "modifies" a message by producing a new one (typically via `Msg::child(new_payload)` which preserves provenance). The QuickJS Function node exposes `msg` as a mutable JS object for authoring familiarity; the runtime snapshots it on exit.

## Settings vs msg overrides — the core pattern

Most non-trivial nodes face the same question: **should this be a fixed setting or come from the message?**

The answer is usually "both, with the message winning when present." An HTTP client has a configured default URL, but an upstream node may want to hit a different URL for this one message. Node-RED's HTTP request node works exactly this way: `msg.url` overrides the configured URL, `msg.method` overrides the configured method, etc.

### Resolution order

When the node fires, each overridable setting is resolved in this order:

1. **`msg.<override_field>`** — if present and non-null, use it.
2. **The node's `config` slot value** — the static setting, either from the UI property panel or set via `PATCH /config`.
3. **Schema default** — the default declared in the settings schema.

This is deterministic and introspectable: given a msg and a node, you can always say where a value came from.

### Declaring overrides in the manifest

The manifest says which settings are overridable and where the override reads from on the msg. Keep override field names matching Node-RED conventions where one exists (`msg.url`, `msg.method`, `msg.headers`, `msg.payload`) so imported flows and ported logic keep working.

```yaml
# acme.io.http.client — manifest excerpt
settings_schema:
  type: object
  properties:
    url:     { type: string, format: uri, title: "URL" }
    method:  { type: string, enum: ["GET", "POST", "PUT", "DELETE", "PATCH"], default: "GET" }
    headers: { type: object, additionalProperties: { type: string }, default: {} }
    timeout_ms: { type: integer, minimum: 1, default: 30000 }
  required: [url]

msg_overrides:
  # setting name (in config) → msg field name that overrides it
  url:        url         # msg.url     overrides config.url
  method:     method      # msg.method  overrides config.method
  headers:    headers     # msg.headers overrides config.headers
  timeout_ms: timeout_ms  # msg.timeout_ms overrides config.timeout_ms
```

Settings **not** listed in `msg_overrides` are fixed config only — the flow author can't override them per message. Use this for anything where letting a msg change the value would be a security or correctness problem (credentials, authentication endpoints, destination tenants).

### The engine does the merging, not you

Node authors don't write merge code. The engine, knowing the kind's manifest, computes a resolved `Settings` struct per message by walking the resolution order above, and hands the node its **already-merged** settings alongside the msg. Node code is straightforward:

```rust
impl HttpClientNode {
    fn on_message(&mut self, ctx: &NodeCtx, msg: Msg) -> Result<Msg, NodeError> {
        let settings: ResolvedSettings<HttpSettings> = ctx.resolve_settings(&msg)?;
        // settings.url, settings.method, etc. are already the right values —
        // msg overrides applied, config filled in, defaults below that.
        let response = http_do(&settings, msg.payload).await?;
        Ok(msg.child(response.into_json()))
    }
}
```

`resolve_settings` is SDK-provided. It validates the merged result against the settings schema so a malformed override fails cleanly rather than causing a downstream crash.

## Worked example — HTTP client node

Putting it together with a concrete node.

### Manifest

```yaml
kind: acme.io.http.client
display_name: "HTTP Request"
description: "Send an HTTP request. Settings are static; any can be overridden by the incoming message."

facets: [isIO, isCompute]
must_live_under: []        # free — drop anywhere
may_contain:     []        # leaf

slots:
  inputs:
    in:       { type: Msg, role: input,  title: "Request", description: "Incoming message; may override settings." }
  outputs:
    out:      { type: Msg, role: output, title: "Response", description: "Response body in payload, status/headers in metadata." }
    error:    { type: Msg, role: output, title: "Error",    description: "Emitted on request failure." }
  status:
    last_status: { type: integer, role: status, title: "Last status code" }
    last_error:  { type: string,  role: status, title: "Last error" }

trigger_policy: on_any       # fire whenever a msg arrives on `in`

settings_schema:
  type: object
  properties:
    url:        { type: string, format: uri, title: "URL" }
    method:     { type: string, enum: ["GET", "POST", "PUT", "DELETE", "PATCH"], default: "GET" }
    headers:    { type: object, additionalProperties: { type: string }, default: {} }
    timeout_ms: { type: integer, minimum: 1, default: 30000 }
    follow_redirects: { type: boolean, default: true }
  required: [url]

msg_overrides:
  url:        url
  method:     method
  headers:    headers
  timeout_ms: timeout_ms
# `follow_redirects` deliberately NOT overridable — we don't let upstream msg
# silently flip redirect behaviour on security-sensitive endpoints.
```

### Runtime, three ways the node can be used

**(a) Static — URL fixed in the property panel.**

```
[Timer every 60s] ──▶ [HTTP Client (url=https://api.example.com/ping)] ──▶ [Log]
```

Every minute, a msg arrives at `in`. No `msg.url` / `msg.method` set, so the engine uses the configured settings: `GET https://api.example.com/ping`.

**(b) Dynamic — msg overrides individual fields.**

```
[Webhook] ──▶ [Function: set msg.url = "https://api/" + msg.payload.id] ──▶ [HTTP Client (method=POST)] ──▶ [Log]
```

The Function node sets `msg.url`. When the HTTP Client fires, the engine sees `msg.url` present and uses it; `msg.method` is absent so configured `method=POST` wins; everything else comes from config / defaults.

**(c) Fully dynamic — msg carries the whole request.**

```
[External queue] ──▶ [HTTP Client (url=<unused>, method=<unused>)] ──▶ [Publish]
```

Every msg carries its own `url`, `method`, `headers`, `payload`. The configured settings are effectively placeholders; the msg wins on everything overridable. `follow_redirects` still comes from config because it's not in `msg_overrides`.

### What the node sees

Given this incoming msg:

```json
{
  "payload": { "name": "Ada" },
  "topic":   "create-user",
  "_msgid":  "b7f2e1c0-...",
  "url":     "https://api.example.com/users",
  "method":  "POST",
  "headers": { "Authorization": "Bearer ..." }
}
```

And this node config:

```json
{
  "url":        "https://api.example.com/ping",
  "method":     "GET",
  "headers":    {},
  "timeout_ms": 5000,
  "follow_redirects": true
}
```

The node's `resolve_settings` call yields:

| Field | Value | Source |
|---|---|---|
| `url` | `https://api.example.com/users` | msg override |
| `method` | `POST` | msg override |
| `headers` | `{ "Authorization": "Bearer ..." }` | msg override |
| `timeout_ms` | `5000` | config |
| `follow_redirects` | `true` | config (not overridable) |

The payload of the response goes to `out`'s `payload`; status code and response headers land in `out.metadata.status` / `out.metadata.response_headers`; the `_parentid` on the emitted msg is the incoming `_msgid` so you can trace request → response in the audit log.

## Best practices

**Sensible defaults.** A user who drops the node and types a URL should get a working GET request. Defaults cover everything except what only the user knows.

**Allow overrides by default — unless override would be a footgun.** Overridable fields make flows more reusable. Don't allow override on: credentials, tenant/org identifiers, destination routing that cross-tenants matters, anything whose value is a security boundary. Put those in config only.

**Name overrides to match Node-RED** where an equivalent exists. `msg.url`, `msg.method`, `msg.headers`, `msg.payload`, `msg.topic` are the canonical names; imported Node-RED flows and ported JS will use these. Novel fields get new names — don't reuse Node-RED names for a different meaning.

**Declare slot roles accurately.** `config` for user-authored settings, `input`/`output` for data flow, `status` for computed state. The platform uses roles to route audit, telemetry, and RBAC correctly; getting them wrong breaks all three quietly.

**Emit child messages, not root messages.** When a node transforms a message, use `msg.child(new_payload)` so `_parentid` is populated and provenance tracing works across the whole flow. Downstream debugging and the audit log both depend on this.

**Surface errors on a dedicated output.** Two outputs (`out`, `error`) beats one output with a status code buried in metadata. Flow authors want to wire the error path explicitly; they rarely want to write `if (msg.metadata.error) { ... }` by hand.

**Declare `trigger_policy` deliberately.** `on_any` is the right default for most nodes. Pick `on_all` when the node genuinely can't do its job without all inputs (two-operand math, join). Pick `on_specific` when one input is the trigger and others are latched values (sample-and-hold, gated outputs).

## See also

- [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) — the graph model, containment, facets, the msg envelope in full
- [UI.md](UI.md) — how the Studio renders property panels from the manifest, including multi-variant settings
- [RUNTIME.md](RUNTIME.md) — lifecycle, safe-state on writable outputs, commissioning and simulation modes
