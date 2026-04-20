# Node Flavors — Implementation Scope

Scope for the three node-execution flavors (core, Wasm, process block) and the **shared SDK** that binds them together. The SDK is the load-bearing piece — if it drifts between core and blocks, or between Rust and TypeScript, the whole block ecosystem gets broken silently.

Authoritative references: [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md), [NODE-AUTHORING.md](../design/NODE-AUTHORING.md), [CODE-LAYOUT.md](../design/CODE-LAYOUT.md), [VERSIONING.md](../design/VERSIONING.md). This doc is the concrete scope for implementation, not a replacement for those.

## The three flavors at a glance

| Flavor | Where it runs | Crash scope | Packaging | Typical use |
|---|---|---|---|---|
| **Core native** | Inside the agent process, statically linked | Cannot crash the agent — panics convert to `NodeError` at the SDK boundary | `/crates/domain-*`, compiled into `apps/agent` | Small, hot-path, trusted compute + logic (count, trigger, switch, math) |
| **Wasm** | Wasmtime sandbox inside the agent | Fuel/memory limits; trap = error outcome, sandbox survives | `.wasm` file loaded at runtime via Wasmtime | Sandboxed user compute, language-agnostic blocks, browser-side preview |
| **Process block (gRPC)** | Separate OS process, supervised by the agent | cgroup memory limits; crash → restart with backoff; agent unaffected | Own binary; speaks `block.proto` over UDS gRPC; optional MF UI | Heavy/I/O-bound blocks, language-diverse integrations (Rust/Go/Python), crash-prone dependencies, license segregation |

All three **share one authoring API**. An author who writes a `NodeBehavior` impl can, with a packaging change, move the same code between core native and process block. The Wasm variant uses the same trait with a wasm32 adapter.

**The shared SDK is the only reason this works.** If you find yourself tempted to add a type or a helper that's "native-only" or "process-only," stop — move it to the shared SDK or prove it cannot be shared.

---

## The shared SDK — the key deliverable

Two packages, one contract surface, used everywhere.

### Rust — `blocks-sdk` (`/crates/blocks-sdk`)

One crate. Consumed unchanged by:

- **Core native crates** (`/crates/domain-*`) — statically linked into `apps/agent`.
- **Wasm crates** (block authors' own crates, compiled to `wasm32-unknown-unknown`) — via the SDK's Wasm adapter feature.
- **Process-block binaries** (block authors' own crates, their own binary) — via the SDK's gRPC-server adapter feature.

Same `NodeBehavior` trait. Same `Msg` envelope. Same `Manifest` builder. Same `#[derive(NodeKind)]`. What differs is only the Cargo feature gate that swaps the adapter.

Contents (minimum):

| Item | Source-of-truth home | What it is |
|---|---|---|
| `Msg`, `MessageId` | `spi::msg` (re-exported) | Node-RED-compatible envelope |
| `NodeBehavior` trait | `blocks-sdk::node` | `on_init`, `on_message`, `on_config_change`, `on_shutdown` |
| `NodeCtx` | `blocks-sdk::ctx` | Logger, `resolve_settings`, `emit`, `read_slot`, `update_status`, `schedule` |
| `#[derive(NodeKind)]` | `blocks-sdk-macros` | Wires the manifest + settings-schema + trigger policy + msg-overrides from attributes |
| `Manifest` / `KindId` / facets / containment | `spi::manifest` (generated from `node.schema.json`) | Declarative kind description |
| `Settings<T>` / `ResolvedSettings<T>` | `blocks-sdk::settings` | Merged config + msg overrides per NODE-AUTHORING.md |
| `NodeError` | `blocks-sdk::error` | Structured errors — never panic across the SDK boundary |
| Capability declarations | `spi::capabilities` (re-exported) | `requires!` macro for declaring what an block needs |

Cargo features on the SDK:

| Feature | Adapter pulled in | Used by |
|---|---|---|
| `native` (default) | Direct in-process registration with the graph service | Core native nodes |
| `wasm` | wasm32-specific glue: host-function imports for `emit`, `read_slot`, `update_status`, `log`, `schedule` | Wasm nodes |
| `process` | `tonic`-based gRPC server implementing `block.proto`, multiplexed over kinds registered in this binary | Process blocks |

Exactly one feature active per consumer. Mutually exclusive. CI enforces.

### TypeScript — `@sys/blocks-sdk-ts` (`/sdks/sdk-ts`)

One package. Consumed unchanged by:

- **Built-in Studio kind UIs** — code shipped with the Studio that contributes property panels / widgets for core and first-party kinds.
- **Federated block UI bundles** — code block authors ship as Module Federation remotes, loaded at runtime into the Studio (trusted: host realm; untrusted: iframe with `postMessage` bridge per UI.md).

Contents (minimum):

| Item | Source-of-truth home | What it is |
|---|---|---|
| `Msg` TypeScript types | generated from `spi::msg` | Mirrors the Rust envelope exactly — field names, optionality, serialization |
| `NodeManifest` type | generated from `spi/schemas/node.schema.json` | Type-safe manifest inspection |
| `PropertyPanel` component | `@sys/blocks-sdk-ts/forms` | `@rjsf/core` integration with multi-variant settings support |
| `useSlotValue(path, slot)` | `@sys/blocks-sdk-ts/hooks` | React hook that subscribes to `graph.<tenant>.<path>.slot.<slot>.changed` via NATS-WS |
| `useNode(path)` | `@sys/blocks-sdk-ts/hooks` | Metadata + lifecycle + all slots |
| `Widget` registration API | `@sys/blocks-sdk-ts/widgets` | For contributing dashboard widgets |
| `defineExtension` entry point | `@sys/blocks-sdk-ts/block` | Module Federation remote exposes — declares panels, widgets, views, commands |
| Capability declaration helpers | mirrors Rust side | For the block's manifest |

### What makes it **the** SDK (not **a** SDK)

Four rules. Violating any one turns "shared" into "drifting":

1. **Nothing a block author references by name lives outside `spi` or the two SDK packages.** If you add a helper, it goes in the SDK, never in a transport crate or an app crate.
2. **Generated code is generated once.** Protobuf → Rust via `tonic`/`prost`, Protobuf → TS via `@bufbuild/protoc-gen-es`. JSON Schema → Rust via `schemars`, JSON Schema → TS via `json-schema-to-typescript`. CI regenerates; no hand-maintained parallel definitions.
3. **The Msg wire shape is frozen.** Rust and TS must round-trip identical JSON. A contract test (a fixture set of `Msg` values serialised by each side, parsed by the other) runs on every SDK PR.
4. **Capabilities are declared via the SDK, never by editing raw YAML.** `requires!(spi::block::v1_3, spi::msg::v1)` in Rust; the TS equivalent in the block's `defineExtension` call. The SDK handles manifest emission. This keeps VERSIONING.md's semver rules machine-enforceable.

### Crate and package locations

Already present in [CODE-LAYOUT.md](../design/CODE-LAYOUT.md); calling them out:

```
/crates/spi/                     # contract surface — protobuf, schemas, Msg, capabilities
/crates/blocks-sdk/          # Rust author SDK — NodeBehavior, derive, adapters
/crates/blocks-sdk-macros/   # proc-macros supporting the SDK (derive(NodeKind))
/crates/blocks-host/         # runtime-side: loads and supervises Wasm + process blocks
/sdks/sdk-ts/                    # TypeScript author SDK — types, hooks, PropertyPanel, widgets
```

---

## Flavor 1 — Core native nodes

Rust code in `/crates/domain-*` that registers kinds with the graph service via the SDK. Compiled into `apps/agent`. **Cannot crash the agent** — all SDK entry points catch panics and convert to `NodeError`.

### Example — `sys.compute.count`

A classic counter. Two inputs — one to increment, one to reset. Also honours `msg.reset` so upstream can reset without wiring the second input. Step, bounds, and wrap are configurable.

#### Manifest

```yaml
kind: sys.compute.count
display_name: "Count"
description: "Increments on each input arrival. Emits current count. Resettable via reset input, msg.reset=true, or config change."
facets: [isCompute]
must_live_under: []       # free — drop anywhere
may_contain:     []

slots:
  inputs:
    in:      { type: Msg, role: input, title: "Increment", trigger: true }
    reset:   { type: Msg, role: input, title: "Reset",     trigger: true }
  outputs:
    out:     { type: Msg, role: output, title: "Count" }
  status:
    count:   { type: integer, role: status, title: "Current count" }

settings_schema:
  type: object
  properties:
    initial: { type: integer, default: 0, title: "Initial value" }
    step:    { type: integer, default: 1, title: "Step", description: "Amount to add per input (negative for down-count)" }
    min:     { type: integer, default: null, nullable: true, title: "Minimum (null = unbounded)" }
    max:     { type: integer, default: null, nullable: true, title: "Maximum (null = unbounded)" }
    wrap:    { type: boolean, default: false, title: "Wrap on bound", description: "If true and min/max set, wrap instead of clamp." }

trigger_policy: on_any

msg_overrides:
  step:    step        # msg.step      — one-shot step override for this message
  reset:   reset        # msg.reset=true — resets regardless of which input fired
  initial: initial      # msg.initial   — sets the value used on reset
```

#### Rust

**Behaviours are stateless — state lives in slots.** A node kind's struct carries no per-instance state fields. The "current count" is the `count` status slot on the node in the graph, not a `self.value` on the struct. Behaviours read from and write to the graph via `NodeCtx`; they never mirror slot state in their own fields. This keeps per-instance state single-sourced and obeys Rule A / Rule B at instance granularity — see [EVERYTHING-AS-NODE.md § "The agent itself is a node too — no parallel state"](../design/EVERYTHING-AS-NODE.md) and [NEW-SESSION.md](../design/NEW-SESSION.md).

The trait takes `&self`, not `&mut self`, so the rule is compiler-enforceable.

```rust
use extensions_sdk::prelude::*;

#[derive(NodeKind)]
#[node(
    kind = "sys.compute.count",
    manifest = "manifests/count.yaml",   // single source of truth; compile-time validated
    behavior = "custom",
)]
pub struct Count;                        // unit struct — no per-instance state

#[derive(Deserialize, SettingsSchema)]
pub struct CountConfig {
    pub initial: i64,
    pub step:    i64,
    pub min:     Option<i64>,
    pub max:     Option<i64>,
    pub wrap:    bool,
}

impl NodeBehavior for Count {
    type Config = CountConfig;

    fn on_init(ctx: &NodeCtx, cfg: &CountConfig) -> Result<(), NodeError> {
        ctx.update_status("count", cfg.initial.into())?;
        Ok(())
    }

    fn on_message(ctx: &NodeCtx, port: InputPort, msg: Msg) -> Result<(), NodeError> {
        let cfg: ResolvedSettings<CountConfig> = ctx.resolve_settings(&msg)?;

        let reset = port == "reset"
            || msg.metadata.get("reset") == Some(&serde_json::json!(true));

        if reset {
            ctx.update_status("count", cfg.initial.into())?;
            return Ok(());
        }

        let current = ctx
            .read_status("count")?
            .as_i64()
            .ok_or_else(|| NodeError::runtime("count slot is not an integer"))?;

        let next = apply_step(current, cfg.step, cfg.min, cfg.max, cfg.wrap);
        ctx.update_status("count", next.into())?;
        ctx.emit("out", msg.child(serde_json::json!(next)))?;
        Ok(())
    }
}

fn apply_step(cur: i64, step: i64, min: Option<i64>, max: Option<i64>, wrap: bool) -> i64 {
    let raw = cur.saturating_add(step);
    match (min, max, wrap) {
        (Some(lo), Some(hi), true)  if raw > hi => lo + (raw - hi - 1).rem_euclid(hi - lo + 1),
        (Some(lo), Some(hi), true)  if raw < lo => hi - (lo - raw - 1).rem_euclid(hi - lo + 1),
        (Some(lo), _, _)            if raw < lo => lo,
        (_, Some(hi), _)            if raw > hi => hi,
        _ => raw,
    }
}
```

### Example — `sys.logic.trigger`

Node-RED-style trigger. On input, emit a configured payload; after a delay or on the reset port, optionally emit a second payload. Covers debounce, timeout, arm/disarm patterns.

#### Manifest

```yaml
kind: sys.logic.trigger
display_name: "Trigger"
description: "On input: emit trigger payload. After delay or on reset: optionally emit reset payload."
facets: [isCompute, isLogic]
must_live_under: []

slots:
  inputs:
    in:    { type: Msg, role: input, title: "Trigger", trigger: true }
    reset: { type: Msg, role: input, title: "Reset",   trigger: true }
  outputs:
    out:   { type: Msg, role: output, title: "Out" }
  status:
    armed: { type: boolean, role: status, title: "Armed?" }

settings_schema:
  type: object
  properties:
    mode:
      type: string
      enum: [once, extend, manual_reset]
      default: once
      description: |
        once         — emit trigger on first input, ignore subsequent inputs until delay elapses or reset arrives.
        extend       — each new input resets the delay (debounce).
        manual_reset — emit only on reset input; delay ignored.
    trigger_payload: { default: true, title: "Trigger payload" }
    reset_payload:   { default: null, nullable: true, title: "Reset payload", description: "null = don't emit on reset" }
    delay_ms:        { type: integer, minimum: 0, default: 0 }

trigger_policy: on_any

msg_overrides:
  delay_ms:        delay
  trigger_payload: trigger
  reset_payload:   reset_value
```

Implementation uses `NodeCtx::schedule(Duration, || ctx.fire_internal(...))` — part of the SDK, available in all three adapters, so nodes register one-shot timers without touching tokio directly.

---

## Flavor 2 — Wasm nodes

Same `NodeBehavior` trait. Crate target is `wasm32-unknown-unknown` (or `wasm32-wasip1`). Built by the block author, loaded at runtime by the agent via Wasmtime (edge/cloud) or the browser's WebAssembly engine (Studio preview).

### Sandboxing and limits

- **Fuel metering** caps CPU per invocation. Exceeding → `NodeError::OutOfFuel`.
- **Memory ceiling** set at instantiation. Exceeding → trap → instance destroyed → next call gets a fresh instance.
- **Host-function allowlist** — the Wasm module can only call what the SDK provides: `emit`, `read_slot`, `update_status`, `log`, `call_extension`, `schedule`. No ambient FS / net access.
- **Trap = error outcome**, sandbox continues. A bad module cannot crash the agent.

### Example — `sys.wasm.math_expr`

Evaluate an arithmetic expression in config against variables from `msg.payload`.

#### Manifest

```yaml
kind: sys.wasm.math_expr
display_name: "Math Expression"
description: "Evaluate a math expression against msg.payload variables."
facets: [isCompute, isWasm]
must_live_under: []

slots:
  inputs:
    in:  { type: Msg, role: input }
  outputs:
    out: { type: Msg, role: output }

settings_schema:
  type: object
  properties:
    expression:
      type: string
      default: "a + b"
      description: "Arithmetic expression. Variables are taken from msg.payload."

trigger_policy: on_any

msg_overrides:
  expression: expression   # allow per-message override
```

#### Rust (wasm32)

```rust
use extensions_sdk::prelude::*;
// SDK is compiled with the `wasm` feature when built for wasm32; same imports.

#[derive(NodeKind)]
#[node(kind = "sys.wasm.math_expr", manifest = "manifest.yaml")]
pub struct MathExpr;

#[derive(Deserialize, SettingsSchema)]
pub struct Config { pub expression: String }

impl NodeBehavior for MathExpr {
    type Config = Config;

    fn on_message(&mut self, ctx: &NodeCtx, _port: InputPort, msg: Msg) -> Result<(), NodeError> {
        let cfg: ResolvedSettings<Config> = ctx.resolve_settings(&msg)?;
        let vars = msg.payload.as_object().cloned().unwrap_or_default();
        let value = eval_expr(&cfg.expression, &vars)
            .map_err(|e| NodeError::runtime(format!("expr error: {e}")))?;
        ctx.emit("out", msg.child(serde_json::json!(value)))?;
        Ok(())
    }
}
```

`Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
blocks-sdk = { workspace = true, default-features = false, features = ["wasm"] }
```

Output is a `.wasm` artefact shipped as part of the block's install bundle (signed per VERSIONING.md). Author wrote the same `NodeBehavior` as a core author.

---

## Flavor 3 — Process block nodes (gRPC over UDS)

Separate binary. Speaks `block.proto` over a Unix domain socket. The agent's `blocks-host` crate supervises the process — spawn, health-check, restart with backoff, cgroup memory limit.

Crash isolation is the selling point: a protocol stack with a C dependency that occasionally segfaults, a Python ML inference binary, a Postgres connection pool — none of these should be able to take down the agent.

### Example — `com.example.pg.query` (Postgres query node, with Module Federation UI)

A "run a parameterised SQL query" node. The block binary holds a connection pool; the node kind wraps one query. Adds a Module Federation UI bundle contributing a schema-aware property panel.

#### Manifest

```yaml
kind: com.example.pg.query
display_name: "Postgres Query"
description: "Execute a parameterised SQL query against a configured Postgres connection."
facets: [isIO, isIntegration]
must_live_under: [com.example.pg.connection]   # must live under a connection node that holds the pool config

slots:
  inputs:
    in:      { type: Msg, role: input, title: "Parameters (from msg.payload)" }
  outputs:
    rows:    { type: Msg, role: output, title: "Rows" }
    error:   { type: Msg, role: output, title: "Error" }
  status:
    last_row_count: { type: integer, role: status }
    last_ms:        { type: integer, role: status }

settings_schema:
  type: object
  properties:
    sql:         { type: string, title: "SQL (use $1, $2 placeholders)", format: "sql" }
    fetch_mode:  { type: string, enum: [all, one, count], default: all }
    timeout_ms:  { type: integer, minimum: 1, default: 30000 }
  required: [sql]

trigger_policy: on_any

msg_overrides:
  sql:        sql          # usually not overridable for security; gated by a capability if enabled
  timeout_ms: timeout_ms

requires:
  - id: spi.block.proto
    version: "^1"
  - id: spi.msg
    version: "^1"
  - id: runtime.extension_process
```

#### Rust — process adapter (block binary)

```rust
use extensions_sdk::prelude::*;

#[derive(NodeKind)]
#[node(kind = "com.example.pg.query", manifest = "manifest.yaml")]
pub struct PgQuery {
    pool: std::sync::Arc<deadpool_postgres::Pool>,
}

#[derive(Deserialize, SettingsSchema)]
pub struct Config {
    pub sql: String,
    pub fetch_mode: FetchMode,
    pub timeout_ms: u64,
}

impl NodeBehavior for PgQuery {
    type Config = Config;

    async fn on_message(
        &mut self,
        ctx: &NodeCtx,
        _port: InputPort,
        msg: Msg,
    ) -> Result<(), NodeError> {
        let cfg: ResolvedSettings<Config> = ctx.resolve_settings(&msg)?;
        let params = msg.payload.as_array().cloned().unwrap_or_default();
        let start = std::time::Instant::now();

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(cfg.timeout_ms),
            run_query(&self.pool, &cfg.sql, &params, cfg.fetch_mode),
        )
        .await;

        ctx.update_status("last_ms", (start.elapsed().as_millis() as i64).into())?;

        match result {
            Ok(Ok(rows)) => {
                ctx.update_status("last_row_count", (rows.len() as i64).into())?;
                ctx.emit("rows", msg.child(serde_json::json!(rows)))
            }
            Ok(Err(e)) => ctx.emit("error", msg.child(serde_json::json!({"error": e.to_string()}))),
            Err(_)     => ctx.emit("error", msg.child(serde_json::json!({"error": "timeout"}))),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    extensions_sdk::run_process_plugin()   // serves block.proto over UDS; multiplexes registered kinds
}
```

The SDK's `run_process_plugin()` reads the UDS path from the env var set by the agent on spawn, implements `describe` / `discover` / `subscribe` / `invoke` / `health` against registered kinds, and streams events back. Author writes `NodeBehavior`; adapter handles the wire.

#### TypeScript — Module Federation UI

```ts
// /ui/src/index.ts — federated module entry
import { defineExtension, PropertyPanel } from '@sys/blocks-sdk-ts';
import { SchemaTablePicker } from './schema-table-picker';
import { ResultsViewer } from './results-viewer';

export default defineExtension({
  id: 'com.example.pg.query',
  requires: {
    'spi.msg': '^1',
    'spi.node.schema': '^1',
  },
  contributions: {
    panels: [
      {
        target: 'node-config',
        kind: 'com.example.pg.query',
        render: ({ node }) => (
          <>
            <SchemaTablePicker connectionPath={node.parent.path} />
            <PropertyPanel node={node} />      {/* @rjsf/core form from manifest */}
          </>
        ),
      },
    ],
    widgets: [
      { id: 'pg.results-viewer', render: ResultsViewer },
    ],
  },
});
```

Built via Rsbuild with Module Federation config exposing `./Block` as the remote. Shipped as part of the block bundle. Host loads directly if signed + vetted; iframe-sandboxed if untrusted (per UI.md).

#### Packaging

```
/blocks/com.example.pg.query/
  manifest.yaml                 # kinds, requires, contributions
  /process/
    Cargo.toml                  # features = ["process"]
    src/main.rs                 # run_process_plugin()
    target/release/pg_query     # compiled binary (multi-arch)
  /ui/
    package.json                # depends on @sys/blocks-sdk-ts
    rsbuild.config.ts
    src/index.ts
    dist/                       # built federated module
  SHA256SUMS.sig                # cosign signature
```

Single bundle. Signed. Installed via `yourapp ext install`, which runs the VERSIONING.md capability match before doing anything.

---

## Which flavor for which job

Quick decision table for authors:

| Situation | Pick |
|---|---|
| Simple compute, logic, routing. Trusted. Fast path. | **Core native** |
| Author wants a language other than Rust. Needs sandboxing. | **Wasm** |
| Heavy I/O, large dependency, crash-prone C library, separate license, long-running background work | **Process block** |
| Protocol driver with its own thread pool, connection state, daemon behaviour | **Process block** |
| User-authored scripting inside a flow (one-off JS) | Not in this scope — QuickJS inside the `Function` core node (see EVERYTHING-AS-NODE.md) |
| Needs to run in the browser (Studio preview) | **Wasm** (same module, browser adapter) |

If the answer could be either core native or process block, the split is: **process block if there's any non-trivial reason for isolation** — crash risk, resource ceiling, separate upgrade cadence, license segregation. Otherwise core.

---

## What each deliverable proves

Aligned with [STEPS.md](STEPS.md) Stage 3:

| Deliverable | Proves |
|---|---|
| `blocks-sdk` with `NodeBehavior` + `#[derive(NodeKind)]` | Authoring API is real and single-source |
| `blocks-sdk-ts` with `PropertyPanel`, hooks, MF entry point | Frontend side of the SDK exists and is used by at least one block |
| `sys.compute.count` + `sys.logic.trigger` (native) | Core flavor works end-to-end; count's two-input + `msg.reset` pattern validated |
| `sys.wasm.math_expr` (Wasm) | Wasm flavor works; fuel + memory limits trigger correctly; host-function ABI is stable |
| `com.example.pg.query` (process block + MF UI) | Process flavor + supervised subprocess + signed MF bundle + per-target UI isolation + capability-gated install |
| Contract test round-tripping `Msg` between Rust and TS fixtures | Shared SDK is not drifting; CI gate in place |
| `yourapp ext check` dry-run that catches a missing capability | VERSIONING.md install-time match works against real blocks |

---

## Non-goals for this scope

- Multi-node-per-kind packaging optimisations (one block contributing 50 kinds) — the pattern supports it; UX polish comes later.
- Hot-reload of native kinds — process blocks reload cleanly; native reload is a restart.
- Cross-language SDKs beyond Wasm (Go/Python process-block SDKs) — proto-only for now. The process-block authoring loop works for any language that speaks gRPC; a first-class non-Rust SDK is its own scope.
- Kind-level migrations (schema evolution on a kind across versions) — spec'd in VERSIONING.md, landing in a later stage.

---

## One-line summary

**Three node flavors (core native, Wasm, process block) sharing one Rust SDK (`blocks-sdk`) and one TypeScript SDK (`@sys/blocks-sdk-ts`); a `NodeBehavior` impl and a manifest YAML look identical across flavors; only the Cargo feature and packaging change; the SDK is the contract that keeps core and blocks from drifting, and it is the key deliverable of this scope.**
