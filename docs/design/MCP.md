# MCP Server Overview

## What it is

An optional adapter that exposes your platform's capabilities to LLMs via the **Model Context Protocol**. Off by default. Same binary as the agent — `yourapp mcp serve --stdio` or `--http :3000`. Built on `rmcp` (Anthropic's official Rust SDK).

## The core idea

The MCP server is a **thin translation layer** over your existing public API. It doesn't re-implement business logic. Every MCP tool call → validated → hits the same code path a CLI command or REST request would. Same RBAC, same audit, same Zitadel JWT check.

If the REST API can't do it, the MCP server can't do it. That's the rule.

## What it exposes

MCP has three primitive types. Here's what each maps to in our stack:

| MCP primitive | What it represents | Our implementation |
|---|---|---|
| **Resources** | Read-only data the LLM can reference | Flows, devices, extensions, recent telemetry windows, logs, audit events |
| **Tools** | Actions the LLM can invoke | Deploy flow, dry-run flow, query device, fetch logs, query telemetry, list extensions |
| **Prompts** | Pre-built prompt templates | "Debug why this flow isn't running," "Explain this node kind," "Write a function node for X" |

## Deployment modes

| Mode | When | Transport | Auth |
|---|---|---|---|
| **stdio** | Local dev, Claude Desktop-style clients, CLI tooling | Parent process pipes | See below — stdio has no HTTP headers |
| **HTTP** | Remote clients, browser-based LLM tools | HTTP with SSE streaming | `Authorization: Bearer <jwt>`, same as REST API |
| **Embedded in CLI** | Ad-hoc agent tasks from a dev's shell | stdio spawned by `yourapp mcp serve` | Inherits CLI's cached OIDC tokens from OS keystore |

On the edge agent: stdio only, local use. On the Control Plane: HTTP, auth required, rate limited.

### stdio auth — spell it out

stdio has no headers. Three concrete mechanisms, in priority order:

1. **Parent-process identity.** If the parent is a CLI invocation by the logged-in OS user (`yourapp mcp serve --stdio`), the MCP server identifies as that user via cached CLI tokens in the OS keystore. The MCP server's process boundary IS the auth boundary.
2. **Handshake token.** The MCP `initialize` call accepts an optional `auth.bearer` param; clients that have a token (Claude Desktop with an org login) pass it here, and the server verifies it like a Bearer header.
3. **Token env var.** `YOURAPP_MCP_TOKEN` read by the server on startup; useful for automation. Discouraged for humans because env vars leak.

Whichever mechanism is used, the resulting `AuthContext` is identical to HTTP-path requests — every tool call goes through the same RBAC check.

## The off-switch (three layers, defense in depth)

Non-negotiable requirement:

| Layer | Mechanism | Purpose |
|---|---|---|
| **Build-time** | Cargo feature flag `--features mcp` | Customers with compliance requirements get binaries with no MCP code at all |
| **Config** | `mcp: { enabled: false }` in YAML | **Default off.** Must be explicitly enabled. |
| **Runtime** | `yourapp mcp disable` / admin toggle | Live kill switch, no restart, for incident response |

All three must work independently. Disabling at runtime must not rely on rebuilding or reconfiguring.

## Security defaults

| Concern | Default |
|---|---|
| Bind address | `127.0.0.1` only — remote exposure requires explicit config |
| Auth | Required — same Zitadel JWT as the public API |
| Permissions | Per-tool RBAC matching the caller's role — MCP can never exceed what the user could do themselves |
| Rate limits | Per-session and per-token, configurable |
| Audit | Every tool call logged with MCP session ID + user ID + arguments |
| Destructive tools | Opt-in — `deploy`, `delete`, `restart` require an additional config flag |
| Prompt injection via tool *descriptions* | Descriptions are static, reviewed, never include live data |
| Prompt injection via tool *results* | **The real risk.** Device names, flow descriptions, log lines, audit messages contain user-authored text that may carry injected instructions. We do not sanitize content flowing to the LLM — that's the client's responsibility. What we do: (a) return **structured** results where possible (JSON fields, not prose), (b) clearly demarcate user-authored content in results with stable markers, (c) never echo tool results back into a follow-up tool description. Document for integrators that tool *results* are attacker-influenced input. |
| Idempotency | Opt-in per tool via explicit `Idempotency-Key` input field backed by server-side dedupe. Not implicit — "same args = same outcome" is only true when the key is provided. |

## Tool design principles

Tools are where most MCP implementations go wrong. Rules for ours:

1. **One tool, one verb.** `list_flows`, `deploy_flow`, `get_flow_logs` — not a single `flow_tool` with a `subcommand` parameter.
2. **Version in the name.** `deploy_flow_v1`. When we change the schema, it's a new tool; old clients keep working.
3. **Idempotency where possible.** `deploy_flow` with the same args twice = same outcome.
4. **Structured errors.** LLM can recover from a typed error like `{ "code": "flow_not_found", "id": "..." }`. It cannot recover from a stringified Rust panic.
5. **Small, focused schemas.** Every tool input and output documented via JSON Schema. No "arbitrary object" parameters.
6. **Safe by default.** Read tools are always available. Write tools are opt-in via config.
7. **Deterministic naming.** Any tool that lists things is `list_*`. Any tool that fetches one thing is `get_*`. Any tool that changes state is a verb (`deploy`, `restart`, `delete`).

## Tool inventory (v1)

**Read — always available when MCP is enabled:**

| Tool | Purpose |
|---|---|
| `list_flows` | List flows with RSQL filter |
| `get_flow` | Get one flow by ID |
| `list_devices` | List devices with RSQL filter |
| `get_device` | Get one device |
| `list_extensions` | List installed extensions |
| `get_extension` | Extension metadata, manifest, status |
| `get_flow_logs` | Recent logs for a flow |
| `query_telemetry` | Time-range query against point history |
| `list_recent_events` | Recent audit/system events |
| `dry_run_flow` | Execute a flow against synthetic input, **no side effects** — read-tier by design (see RUNTIME.md simulation mode) |

**Write — gated behind `mcp: { allow_writes: true }`:**

| Tool | Purpose |
|---|---|
| `deploy_flow` | Deploy a flow to a target |
| `pause_flow` | Pause a running flow |
| `resume_flow` | Resume a paused flow |
| `install_extension` | Install an extension from the registry |
| `update_device_config` | Change device configuration |

**Destructive — gated behind `mcp: { allow_destructive: true }` AND require confirmation tokens:**

| Tool | Purpose |
|---|---|
| `delete_flow` | Remove a flow |
| `uninstall_extension` | Remove an extension |
| `restart_agent` | Restart an edge agent |

## Resources

Resources are read-only URIs the LLM can subscribe to or reference. They're useful for giving the LLM context without burning tool calls.

| URI pattern | Contents |
|---|---|
| `flow://<id>` | Full flow document |
| `flow://<id>/logs` | Recent log stream |
| `device://<id>` | Device metadata and current state |
| `extension://<id>` | Extension manifest + docs |
| `schema://flow` | Current flow JSON Schema |
| `schema://extension` | Extension manifest schema |
| `docs://api` | OpenAPI spec |

## Prompts

Pre-built templates for common LLM workflows:

| Prompt | Use |
|---|---|
| `debug_flow` | "Here's a flow that isn't working. Walk through what it does, look at recent logs, identify the problem." |
| `explain_node` | "Explain what this node type does and give an example flow that uses it." |
| `write_function_node` | "Write a JavaScript function node that [...]" — outputs code the user can paste into a Function node |
| `suggest_extension` | "I need to integrate with [service]. Is there an extension, or should one be written?" |
| `audit_investigate` | "Here's an audit event. Explain what happened and whether it's suspicious." |

## Lifecycle

| Phase | What happens |
|---|---|
| Startup | If feature enabled and config allows, MCP server binds to configured transport |
| Session open | Client connects, MCP handshake, RBAC check against auth token |
| Tool discovery | Server returns filtered tool list based on caller's role + write/destructive config |
| Tool call | Request validated against tool's input schema → handler calls the public API internally → result serialized via output schema → audit logged |
| Resource read | URI resolved → underlying API call → cached + returned |
| Session close | Session metrics flushed; in-flight calls cancelled gracefully |
| Shutdown | Server drains in-flight calls, rejects new ones, exits |

## Wire path

```
LLM client → MCP (stdio or HTTP)
           → rmcp dispatcher
           → tool handler (validates args)
           → same internal API the REST layer uses
           → SeaORM / extension supervisor / NATS
           → response
           → audit log entry
           → back to LLM
```

No shortcut. No "direct database access for speed." Every path goes through the same layers as a human user would.

## Observability

Because every tool call hits the audited API path, you get:

- Prometheus metrics per tool (count, latency, error rate)
- Audit log entries with session ID → trace back every LLM action to the session it came from
- Per-tool rate limits
- Per-session quotas
- Anomaly detection (the same LLM deploying 100 flows in 60 seconds is worth flagging)

## Testing

| Test | How |
|---|---|
| Unit | Each tool handler tested like a REST handler |
| Contract | JSON Schema validation on every tool's input/output |
| Off-switch | Integration test: disabled config means zero listening sockets |
| Permission | Test matrix: each role × each tool → allowed / denied |
| Prompt injection | Red-team fixtures: inputs with embedded instructions don't escalate privilege |
| Rate limiting | Burst and sustained load tests |

## What we don't do

Worth stating explicitly:

- **No agent loop inside the MCP server.** It's a tool surface, not a policy engine. The LLM planning happens on the client side.
- **No direct DB access.** Every tool goes through the API layer.
- **No custom LLM features.** We're not training, fine-tuning, or running inference on the platform. We expose tools; what the client does with them is the client's problem.
- **No special "MCP-only" features.** If it exists in MCP, it exists in the REST API. Parity is enforced.



## One-line summary

**A thin, versioned, RBAC-enforced MCP surface over the existing public API — resources for read context, tools for actions, prompts for common LLM workflows, with three-layer off-switches and compile-out support for compliance-sensitive deployments.**