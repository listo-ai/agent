# Logging Strategy

How the platform logs. One format everywhere — core agent, extensions (native / Wasm / process), Studio client, CLI, MCP. Same field names, same levels, same redaction rules, same shipping path. Plugins get a logger through the SDK; they can't not be consistent.

Long-term maintainability depends on this being boring. If every extension invents its own format, cross-cutting debugging ("why did this flow fail on three sites at 02:14 UTC?") becomes archaeology.

Authoritative references: [NODE-AUTHORING.md](NODE-AUTHORING.md), [CODE-LAYOUT.md](CODE-LAYOUT.md) (`crates/observability`), [VERSIONING.md](VERSIONING.md) (log schema is a versioned contract surface), [RUNTIME.md](RUNTIME.md) (outbox carries log forwarding with the same backpressure rules).

## The thesis

**One log event format, one set of canonical fields, one transport shape — from edge extensions to cloud Control Plane to the browser Studio.**

- Structured JSON, one event per line. Human pretty-print only in `dev` mode; machine parsing is the default.
- Canonical fields are declared in `spi`, used by `observability`, mirrored in `@acme/sdk-ts/log`. No hand-maintained parallels.
- The shared primitive is the only primitive. Extensions that bypass it get rejected in review.
- Log events and audit events are distinct streams, both structured, both governed by the same contract rules.

## Canonical field contract

Every log event includes these fields. The field names are part of the contract surface (frozen add-only, breaking changes bump `log.schema_version`).

| Field | Type | Required | Purpose |
|---|---|---|---|
| `ts` | ISO-8601 string with TZ (`2026-04-19T14:03:22.417Z`) | yes | Event time |
| `level` | string (`trace` / `debug` / `info` / `warn` / `error`) | yes | Severity |
| `msg` | string | yes | Human-readable message |
| `target` | string | yes | Module path (`graph::store`, `com.example.pg.query::handler`) |
| `log.schema_version` | integer | yes | Starts at 1; bumps only on breaking changes |

Scope-dependent fields, added automatically by the logger when the context is present (never by callers):

| Field | When | Purpose |
|---|---|---|
| `tenant_id` | Any event inside a tenant-scoped operation | Filter logs by customer |
| `user_id` | Any event inside an authenticated request | Attribute actions to users |
| `agent_id` | Every event emitted by an edge/standalone agent | Fleet-wide filtering |
| `node_path` | Event emitted during a node-kind invocation | "What did *this* node do?" |
| `kind_id` | Same as above | Aggregate across all instances of a kind |
| `msg_id` | Event emitted while processing a `Msg` | Trace a single message end-to-end through a flow |
| `parent_msg_id` | Same as above, when the msg has a parent | Reconstruct fan-out / fan-in history |
| `flow_id` | Event inside a flow run | Filter to one flow's execution |
| `request_id` | Event inside an HTTP/gRPC request | Correlate server + client |
| `span_id`, `trace_id` | When a tracing span is active | OpenTelemetry propagation |
| `plugin_id`, `plugin_version` | Any event emitted from a plugin | Isolate noisy or misbehaving extensions |

Author-added ad-hoc fields are fine, scoped under a `ctx.<name>` prefix to avoid collisions with platform-reserved names:

```
{"ts":"...","level":"info","msg":"retry queued","target":"...","ctx.retry_count":2,"ctx.reason":"5xx"}
```

**Platform-reserved names**: all top-level keys listed above plus anything matching `_*` (reserved for platform extensions). Author fields live under `ctx.*`.

## Levels — when to use what

| Level | Semantics | Example |
|---|---|---|
| `error` | An operation failed and the caller needs to act. Human attention expected. | Extension failed to start; persistence write rejected |
| `warn` | Something unexpected, but the system continued. Worth investigating in aggregate. | Config override fell back to default; slow query |
| `info` | State transitions and "loud" lifecycle events. Limited rate. | Agent boot, flow started/stopped, extension installed |
| `debug` | Diagnostic detail. Off in production by default. | Slot write with value; RPC request payload shape |
| `trace` | Fine-grained tracing. Off unless actively debugging. | Every tick of an event loop, every packet received |

Production defaults: `info` for platform crates, `warn` for noisy internals. Extensions inherit the agent's filter; they can log at any level but the configured filter drops anything below the threshold before it's serialised.

## Where logs go — per deployment

Same format, different sinks.

| Deployment | Primary sink | Secondary | Notes |
|---|---|---|---|
| Edge (512 MB ARM) | Rotating file `/var/log/yourapp/agent.log` (size-capped, age-capped) | Optional NATS-forwarded upstream via outbox | No stdout in production (systemd swallows, and we can't afford `journald` overhead on the slowest targets) |
| Edge (x86) | Same as ARM edge; JetStream-on-edge can buffer more | Optional direct ship to Loki/ELK on local network | |
| Standalone | Rotating file | Web UI tail available via `/api/v1/logs` WebSocket | |
| Cloud (containerised) | `stdout` (JSON line per log event) | Picked up by Kubernetes/container log aggregator | `journald`, `fluent-bit`, Loki, whatever the operator runs |
| CLI | `stderr` (JSON) by default; `pretty` when attached to a TTY and `--log-format=pretty` | — | `--verbose` raises the filter to `debug` |
| Studio (browser) | Console + ring-buffered in-memory; user can download or ship to Control Plane | Control Plane receives via `POST /api/v1/client-logs` | Rate-limited; never PII-leaking |

Rotation defaults for file sinks: 100 MB per file × 7 files, or 7 days' worth, whichever hits first. Configurable per deployment.

## Forwarding to the Control Plane — the outbox path

Edge agents optionally ship logs to the Control Plane. Same outbox pattern as cloud-bound telemetry from [RUNTIME.md](RUNTIME.md):

- **Subjects** — `log.<tenant>.<agent_id>.<level>.<subsystem>`. Wildcards let consumers filter to a site, a level, a subsystem.
- **Outbox-backed** — during a cloud outage, logs accumulate in the bounded disk outbox. Oldest-first drop policy for `trace`/`debug`, newest-first reject for `error` (we don't want to lose critical events). Health events published on NATS when the log outbox approaches caps.
- **Sampling** at the edge for `trace`/`debug` — configurable ratio per subject prefix so a misbehaving noisy extension doesn't exhaust the outbox.
- **Backpressure** — if the outbox is full and ship-rate is saturated, the logger falls back to file-only locally. The system never blocks on log delivery.

## The shared primitive — `crates/observability` + `@acme/sdk-ts/log`

### Rust — `observability` (already in the workspace)

```rust
use observability::prelude::*;

info!(node_path = %ctx.node_path(), msg_id = %msg.id, "queued retry");
warn!(ctx.retry_count = 3, "backoff limit exceeded");
```

Thin wrapper over `tracing` + `tracing-subscriber`. What the crate provides:

| Item | Purpose |
|---|---|
| `observability::init(role, filter_env)` | One-call setup; agent boot calls this. Picks stdout/file/journald based on `role`, applies the env filter, installs the redactor. |
| `observability::prelude::{trace, debug, info, warn, error}` | Macros that enforce the canonical-field contract. Unknown top-level keys fail to compile. |
| `observability::fields` | Constants for every canonical field name — so typos become compile errors, not silent bugs. |
| `observability::redact` | Automatic scrubbing + hooks for extension-declared secret fields. |
| `observability::span` | Span creation with mandatory `trace_id` / `span_id` fields, OpenTelemetry-compatible. |
| `observability::shipper` | NATS forwarder, outbox-backed. Wired in only when the deployment's config enables it. |

### TypeScript — `@acme/sdk-ts/log`

Same surface, same field names, same levels:

```ts
import { log } from '@acme/extensions-sdk-ts';

log.info({ node_path: path, msg_id: msg.id }, 'queued retry');
log.warn({ 'ctx.retry_count': 3 }, 'backoff limit exceeded');
```

Built on `pino` under the hood with a custom formatter enforcing the field contract. Browser ships to `POST /api/v1/client-logs` with rate limiting + ring-buffer-on-fail.

### Shared rules

1. **Field names come from a single source.** The set lives in `spi::log::fields` (Rust) + generated TS constants. Changing a field is a VERSIONING.md concern, not a local decision.
2. **No `println!`, `console.log`, `eprintln!`, `print!` in library code.** Ever. CI rejects. Only permissible in test fixtures or the explicit TTY-interactive branches of the CLI.
3. **The JSON output is stable.** Contract test in CI parses a fixture set and asserts field names, types, and required-field presence. Rust and TS both run the test.
4. **Redactor runs before serialisation.** Secrets never hit the wire, even if a caller passes them in.

## Plugin integration — three flavors, one stream

Plugins use the same logger as core. Which SDK surface they see depends on flavor, but the wire output is identical.

### Core native (in-process)

Direct `use observability::prelude::*;` — same macros as core crates. `plugin_id` + `plugin_version` fields are injected automatically by the SDK when the log event originates inside a kind declared by an extension.

### Wasm

Host function exposed by the SDK:

```
fn host_log(level: u32, msg_ptr: *const u8, msg_len: u32, fields_ptr: *const u8, fields_len: u32);
```

The author calls `log::info!(...)` inside their Wasm code; the SDK's Wasm adapter marshals the call across to the host, where it lands in the same `observability` stream with `plugin_id` + `plugin_version` set from the module's manifest.

### Process plugin (gRPC)

Plugin writes JSON log lines to `stderr`. The extension supervisor captures them, parses them as canonical events, and re-injects into the main stream with `plugin_id` + `plugin_version` + the child's PID. The plugin's SDK gives the author `info!/warn!/…` that produce the right stderr format automatically. Authors never see the marshalling.

A plugin that writes non-JSON to stderr logs a one-time warning and the line is wrapped as `{"level":"warn","msg":"<raw line>","target":"plugin.<id>.raw"}`. Graceful degradation.

### No separate log files per plugin

The value of "one stream" dissolves if plugins write to their own files. All plugin events land in the agent's stream, keyed by `plugin_id` so filtering is easy. The only exception: if a plugin has a *truly* huge log volume (an ML inference plugin at debug level, for instance), the operator can enable a dedicated sink for that plugin via config — but it's opt-in and off by default.

## Correlation — ids that travel with the event

Cross-cutting debugging ("why did *this* message fail?") works only if ids propagate. Rules:

- **`msg_id`** lives on `spi::Msg`. Every log event emitted during a node's handling of a message MUST include it. The SDK's `NodeCtx::log()` sets this automatically from the message being handled.
- **`parent_msg_id`** lets you trace across fan-out. A child message (`Msg::child`) carries it; logs inherit it.
- **`flow_id`** is set by the flow engine when running a flow-document execution. All events inside the run have it.
- **`request_id`** is allocated at the transport layer on every HTTP/gRPC request. Included in the request's auth log, every DB call, every downstream call. Returned as a response header (`X-Request-Id`) so clients can correlate server-side logs with their own events.
- **`trace_id` / `span_id`** come from OpenTelemetry spans. When a span is active, every log event inside it picks up both automatically.

Studio shows logs filtered by any of these. CLI too (`yourapp logs --msg-id <id> --follow`).

## Tracing — spans and OpenTelemetry

For long-running operations, spans are preferable to bare events. `observability::span` wraps `tracing::span` with the mandatory correlation fields:

```rust
let _guard = span!("flow.run", flow_id = %flow.id, trigger = "cron").entered();
// everything logged inside this block automatically picks up flow_id + span_id + trace_id
```

The subscriber exports spans as OpenTelemetry Protocol (OTLP) when configured, so existing observability stacks (Grafana Tempo, Honeycomb, Jaeger, etc.) work out of the box. Exporting is disabled by default to avoid surprise outbound traffic; an operator enables it via config.

## Redaction — automatic and declarative

Two layers:

1. **Automatic** — keys matching known patterns are redacted before serialisation. Defaults: `authorization`, `x-api-key`, `password`, `token`, `secret`, and any key suffixed `_secret` / `_token` / `_key`. Redacted means the value is replaced with `"<redacted>"`; the key stays so it's obvious from logs that a secret was involved.
2. **Declarative** — extensions can declare secret field names in their manifest. The SDK merges those into the redactor. Example: the Postgres query extension declares `connection_password` as secret, so no log anywhere shows its value, even if a caller mistakenly passes it in a log field.

Redaction runs at the logger boundary, not at the call site. You can't forget to redact.

## Logs vs. audit — the boundary

Two distinct streams, both governed by this contract surface:

| Stream | What it is | Examples | Retention |
|---|---|---|---|
| **Logs** (this doc) | Operational diagnostics. High volume. May be sampled. Retention measured in days. | "query took 12ms", "retry #3", "extension started", "flow run completed" | 7–30 days, then rotated |
| **Audit** | Structured, immutable record of security/business-relevant actions. Low volume. Never sampled. Retention measured in years. | "user X granted role admin", "scope Y deleted", "flow Z deployed", "extension A installed" | 1–7 years, depending on compliance |

Same field contract (so filtering works consistently), different sinks, different retention, different access control. The `audit` crate owns event types; `observability` owns transport. Events go to both streams when they're operationally interesting *and* audit-relevant (e.g. "extension installed" — the install trace goes to logs; the `extension.installed` fact goes to audit).

**Rule of thumb**: could a compliance officer ask for this in three years? It's an audit event. Is it operational noise? It's a log event.

## Configuration

Environment variable is the baseline; YAML overrides; runtime API is the escape hatch.

```yaml
# agent.yaml excerpt
logging:
  filter: "info,graph=debug,com.example.pg.query=trace"   # RUST_LOG-compatible syntax
  format: json                                             # json | pretty
  sinks:
    file:
      path: /var/log/yourapp/agent.log
      rotate:
        max_size_mb: 100
        max_files: 7
        max_age_days: 7
    nats:
      enabled: true
      subject_prefix: "log.${tenant_id}.${agent_id}"
      sample:
        trace: 0.01         # 1% sampling for trace
        debug: 0.1
    otlp:
      enabled: false
      endpoint: https://otel.example.com:4317
  redact_extra:
    - "my_custom_secret_field"
```

Runtime reconfiguration: `PATCH /api/v1/logging` + `yourapp log set-filter 'info,graph=debug'`. Change takes effect immediately without restart — the subscriber reloads its filter. This is the operational escape hatch: when something's broken in production, you raise the level on the right module without a redeploy.

Environment variables:

| Var | Purpose |
|---|---|
| `YOURAPP_LOG_FILTER` | Same shape as `RUST_LOG`. Precedence over the config file. |
| `YOURAPP_LOG_FORMAT` | `json` (default) or `pretty` |
| `YOURAPP_LOG_SINKS` | Comma-separated subset of configured sinks (`file,nats`) |

## Edge constraints

On 512 MB / 8 GB-storage edge devices:

- File sink is mandatory; stdout is discouraged (journald overhead is real).
- NATS forwarding is opt-in per deployment, not default-on. A customer who wants offline-only operation gets it with no wasted cycles.
- Rotation defaults are tighter than cloud: 50 MB × 7 files on ARM 512 MB deployments, with a 7-day age cap.
- Sampling for `trace`/`debug` is on by default at 1% / 10% when NATS forwarding is enabled. No point in filling the outbox with noise.
- The logger allocates a small fixed ring buffer for back-pressure on file writes; under disk-full the buffer drops the oldest `trace`/`debug` events first and keeps `error` until the disk clears. The system never blocks on a log write.

## Contract surface — treated like any other contract

Per [VERSIONING.md](VERSIONING.md), the log schema is a versioned contract:

- Capability id `spi.log.schema`, current version `1.0`.
- Extensions that emit structured log fields beyond the defaults declare `requires: [spi.log.schema: "^1"]`.
- Adding a canonical field is a minor bump; renaming or removing is a major bump with a deprecation window.
- CI diffs the field list on every PR; removing a field without a deprecation flag fails the build.

This means log consumers (Grafana queries, alerting rules, customer dashboards) can pin to a major version and know what to expect.

## CI / test expectations

| Test | What it proves |
|---|---|
| Field-contract fixture test | A set of committed JSON fixtures is parsed by both Rust and TS loggers — fields present, types right, required fields required. |
| Redaction test | A log event containing each default-redacted key is serialised and asserted to contain `"<redacted>"` for those values. |
| `println!`/`console.log` lint | CI grep fails if either appears in library code (test files and explicit TTY-interactive CLI branches allowed). |
| Span-propagation test | A logged event inside a span asserts `trace_id` and `span_id` are present. |
| Plugin-flavor test | Each plugin flavor (native / Wasm / process) emits an event that reaches the central stream with `plugin_id` correctly set. |
| Outbox-backpressure test | Fill the log outbox, assert oldest-first drop for low levels, newest-first reject for `error`, no blocking on the caller. |

## One-line summary

**One log event format, one set of canonical fields, one shared primitive across Rust / TypeScript / Wasm / process plugins — structured JSON everywhere, automatic correlation via node-path / msg-id / flow-id / request-id, automatic redaction, configurable runtime filter without restart, outbox-backed shipping to the Control Plane with backpressure — so "why did this flow fail at 02:14 on site X" is a one-query answer, not an archaeology project.**
