# CLI Design

The agent binary serves two purposes: **daemon** (`agent run`) and **CLI client** (everything else). The CLI client talks to a running agent over HTTP using the same REST API that the Studio and TS client consume.

## Architecture

```
agent binary (clap)
  ├─ run              starts the long-lived daemon
  └─ <command>        HTTP client → running agent's REST API
```

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│ transport-   │────▶│ agent-client │────▶│ transport-rest   │
│ cli (clap)   │     │ (reqwest)    │     │ (axum, running)  │
└─────────────┘     └──────────────┘     └─────────────────┘
  CLI concerns        reusable lib         server-side
  (args, output)      (no CLI deps)        (no client deps)
```

Three crates, strict separation:

| Crate | Location | Responsibility | Dependencies |
|---|---|---|---|
| **agent-client** | `clients/rs/` | Pure HTTP client library — `reqwest` + wire-shape DTOs | `reqwest`, `serde`, `serde_json`, `semver` |
| **transport-cli** | `crates/transport-cli/` | Clap command tree + output formatting | `agent-client`, `clap`, `serde_json`, `anyhow` |
| **agent** (binary) | `crates/apps/agent/` | Mounts `run` (daemon) + CLI subcommands | `transport-cli`, `transport-rest`, `engine`, `graph`, … |

`agent-client` has **no dependency** on any server crate (`graph`, `engine`, `transport-rest`). Any Rust code — tests, other services, scripts, the CLI — can use it.

## Command tree

### Global options

Every subcommand inherits these:

| Flag | Short | Env | Default | Description |
|---|---|---|---|---|
| `--url <URL>` | `-u` | `AGENT_URL` | `http://localhost:8080` | Agent base URL |
| `--token <TOKEN>` | | `AGENT_TOKEN` | | Bearer token for auth |
| `--output <FORMAT>` | `-o` | | `table` | `table` or `json` |

### `agent run` — start the daemon

Not a CLI client command — this starts the actual agent process.

```
agent run [--role standalone|edge|cloud]
          [--config <PATH>]
          [--db <PATH>]
          [--log <DIRECTIVE>]
          [--http <ADDR>]
```

| Flag | Default | Description |
|---|---|---|
| `--role` | (from config/env) | Deployment role |
| `--config` | | YAML config file path |
| `--db` | | SQLite database path; unset = in-memory |
| `--log` | `info` | Tracing filter (e.g. `info,engine=debug`) |
| `--http` | `127.0.0.1:8080` | HTTP bind address |

### `agent health`

Check if the agent is reachable. Exits 0 if healthy, 1 if not.

```
agent health
agent -u http://10.0.0.5:8080 health
```

### `agent capabilities`

Show the agent's platform version, API version, and capability list.

```
agent capabilities
agent capabilities -o json
```

Table output:

```
agent 0.0.1  ·  api v1  ·  flow_schema=1  node_schema=1

CAPABILITY             VERSION
spi.extension.proto    1.0.0
spi.msg                1.0.0
spi.node.schema        1.0.0
spi.flow.schema        1.0.0
data.sqlite            3.45.0
```

### `agent nodes`

```
agent nodes list                              # table of all nodes
agent nodes get /station/floor1/ahu-5         # single node detail (JSON)
agent nodes create /station acme.core.folder floor1   # create child
```

Table output for `list`:

```
PATH                              KIND                       LIFECYCLE  ID
/station                          acme.core.station          created    a1b2c3…
/station/floor1                   acme.core.folder           created    d4e5f6…
/station/floor1/counter           acme.compute.count         active     789abc…
```

### `agent slots`

```
agent slots write /station/floor1/counter in 42
agent slots write /station/floor1/counter in '"hello"'
agent slots write /station/floor1/counter in '{"x":1}'
```

The value argument is parsed as JSON. If it fails to parse, it's treated as a plain string.

### `agent config`

```
agent config set /station/floor1/counter '{"step":5,"initial":10}'
```

Replaces the node's config blob and re-fires `on_init`.

### `agent links`

```
agent links list
agent links create \
    --source-path /station/floor1/counter --source-slot out \
    --target-path /station/floor1/trigger --target-slot in
agent links remove <uuid>
```

Table output for `list`:

```
ID                                    SOURCE                          TARGET
a1b2c3d4-…                            /station/counter:out            /station/trigger:in
```

### `agent lifecycle`

```
agent lifecycle /station/floor1/counter active
agent lifecycle /station/floor1/counter disabled
```

Transitions a node through the legal-transition table. The `to` argument is the snake_case lifecycle state.

### `agent seed`

```
agent seed count_chain
agent seed trigger_demo
```

Seeds a preset graph for interactive testing.

## Output formatting

`--output table` (default) renders human-readable aligned tables and short status messages. `--output json` renders pretty-printed JSON for every command — suitable for piping into `jq`, scripting, and programmatic consumption.

Examples:

```bash
# Pipe to jq
agent nodes list -o json | jq '.[].path'

# Table for humans
agent nodes list
```

## LLM-friendly surface

The CLI is the primary interface for any agent with shell access (Claude Code, Cursor, aider, CI scripts). Three properties make it legitimately LLM-native instead of something LLMs just barely cope with. These are **contracts**, not niceties — breaking any of them breaks every downstream LLM consumer silently.

See [MCP.md § "Cheap way to get 80% of MCP's value from the CLI"](MCP.md) for the rationale — these three items are why MCP remains on-demand rather than scheduled.

### 1. Deterministic JSON output (contract, not best-effort)

`-o json` output is stable across versions. Specifically:

| Property | Rule |
|---|---|
| **Field order** | Stable. Set via a canonical struct definition; serde's default field order, never reordered alphabetically or by "relevance". Reordering is a breaking change, treated the same as renaming. |
| **Timestamps** | RFC 3339 with explicit offset (`2026-04-19T10:12:33.482Z`). Never Unix epoch, never locale-formatted. |
| **Numbers** | Integers as JSON integers. Durations as milliseconds unless the field name ends `_sec` / `_ns`. Never stringified for "safety". |
| **Nulls** | Explicit `null` — never omitted. A missing key means "the field doesn't exist at this API version"; a present `null` means "no value". |
| **Error shape** | Identical for every command: `{"code": "…", "message": "…", "details": {…}}`. `code` is a stable snake_case enum (`flow_not_found`, `bad_path`, `backend_unavailable`, …); `message` is human-readable and may change; `details` is a structured object whose shape is per-code and documented. |
| **Exit codes** | `0` success, `1` user error (bad args, not-found, precondition failed), `2` infrastructure error (agent unreachable, timeout), `3` internal error (deserialisation failure, panic caught). Stable across versions. |

CI gate: a round-trip test asserts the JSON output of every command against a pinned fixture. Drift = PR fails. Same pattern as [`clients/contracts/fixtures/`](../../clients/contracts/fixtures/) uses for wire-shape contracts.

### 2. `agent schema <command>` — machine-readable discovery

Dumps JSON Schema for a subcommand's inputs and outputs:

```bash
agent schema nodes create
```

```json
{
  "command": "nodes create",
  "input": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "type": "object",
    "required": ["parent", "kind", "name"],
    "properties": {
      "parent": { "type": "string", "format": "node-path" },
      "kind":   { "type": "string", "format": "kind-id"   },
      "name":   { "type": "string", "pattern": "^[a-zA-Z_][a-zA-Z0-9_-]*$" }
    }
  },
  "output": { "$ref": "https://example.com/schemas/CreatedNodeResp.json" },
  "errors": [
    { "code": "bad_path",          "exit": 1 },
    { "code": "kind_not_found",    "exit": 1 },
    { "code": "placement_refused", "exit": 1 }
  ]
}
```

Schemas are derived from the same types the REST handlers use ([routes.rs](../../crates/transport-rest/src/routes.rs)) via `schemars` — one source of truth, no hand-maintained duplicate. Stage 9's OpenAPI generation (see [STEPS.md](../sessions/STEPS.md)) reuses the same schemas.

**Why this matters:** an LLM planning a multi-step interaction reads the schemas once at session start and plans without probing. No "try it and see what breaks" cycles. Cuts LLM-driven automation latency roughly in half on tasks that touch more than one command.

**Bonus:** `agent schema --all -o json` dumps every schema in one document — the LLM's equivalent of MCP's `tools/list`.

### 3. `--help-json` on every subcommand

Every subcommand accepts `--help-json` alongside the usual `--help`:

```bash
agent nodes create --help-json
```

```json
{
  "command": "agent nodes create",
  "summary": "Create a child node under a parent path.",
  "args": [
    { "name": "parent", "required": true, "type": "node-path",  "description": "Parent path, e.g. /station/floor1" },
    { "name": "kind",   "required": true, "type": "kind-id",    "description": "Node kind id, e.g. acme.core.folder" },
    { "name": "name",   "required": true, "type": "identifier", "description": "Child name segment" }
  ],
  "examples": [
    "agent nodes create /station acme.core.folder floor1"
  ],
  "related_commands": ["nodes list", "nodes get"],
  "output_schema_ref": "agent schema nodes create"
}
```

Humans keep reading `--help` (prose), LLMs prefer `--help-json` (structured). Both are generated from the same `clap` definition via a custom renderer — drift is impossible.

### What LLMs get from this trio

- **`agent capabilities -o json`** → platform surface at a glance.
- **`agent schema --all -o json`** → every tool's input/output shape.
- **`agent <cmd> --help-json`** → prose + examples without regex-parsing man pages.
- **Stable JSON outputs** → no defensive field-renaming heuristics.
- **Stable exit codes + error codes** → reliable retry logic.

Together these give an external agent enough information to drive the platform without an MCP server. When MCP does land (on-demand per [MCP.md](MCP.md)), it becomes a thin façade over this same surface — no parallel contract to maintain.

## Environment variables

| Variable | Maps to |
|---|---|
| `AGENT_URL` | `--url` |
| `AGENT_TOKEN` | `--token` |

## Future additions (deferred to later stages)

Per [STEPS.md](../sessions/STEPS.md) Stage 11:

- `agent flow {list,deploy,start,stop,status}` — flow lifecycle
- `agent device {list,discover,commission}` — device management
- `agent ext {install,enable,disable,publish,check,upgrade}` — extension lifecycle
- `agent login` — OIDC device flow
- `agent mcp` — MCP server management
- `--local` vs `--remote` targeting
- Shell completions (bash, zsh, fish, powershell)
- Config file at `~/.config/agent/config.toml`
