# Coding Stages

Staged so each stage produces something that runs and each stage proves a specific architectural risk. Skip no seams — stub implementations behind final interfaces.

## Stage 0 — Foundations

**Goal:** Repo, tooling, contracts. Nothing runs yet except tests.

**Status:** Rust side done; frontend / CI / crossflow vendoring deferred with explicit markers below. Workspace compiles, tests green, `agent` binary starts + exits cleanly.

- [DONE] Cargo workspace — all 26 crates from [CODE-LAYOUT.md](../design/CODE-LAYOUT.md) wired up under `/crates/`
- [DEFERRED] pnpm workspace — lands with Stage 4 (Studio shell) when frontend code is first needed
- [DEFERRED] CI: fmt, clippy, test, build for `aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-gnu` — repo builds locally; workflow to be added
- [DONE] `/crates/spi/` — the contracts, written before anything that uses them (path updated from the README's legacy `/packages/spi/`):
  - [DONE] `block.proto` — gRPC schema for blocks (describe, discover, subscribe, invoke, health)
  - [DONE] `flow.schema.json` — flow document format with `schema_version: 1`
  - [DONE] `node.schema.json` — node manifest format
  - [DONE] `Msg` / `MessageId` envelope (Node-RED-compatible wire shape) per [NODE-AUTHORING.md](../design/NODE-AUTHORING.md)
- [DONE] Repository traits (`FlowRepo`, `DeviceRepo`) defined in `/crates/data-repos/` — implementations come later, **one per backend** (SQLite, Postgres). No lowest-common-denominator schema.
- [DONE] Message bus trait in `/crates/messaging/` — one interface, in-process implementation (`InProcessBus`) shipped; NATS later.
- [DEFERRED] Tauri app scaffolded, Rsbuild config, Shadcn + Tailwind set up — Stage 4.
- [DEFERRED] **Vendor crossflow** into the monorepo at a pinned commit — blocked on upstream repo URL + pin. The `engine` crate is a stub today; this must land before Stage 2.
- [DONE] **Capability registry in `spi`** (see [VERSIONING.md](../design/VERSIONING.md)) — `CapabilityId`, `CapabilityVersion`, `SemverRange` types + matcher in `crates/spi/src/capabilities.rs`. No host registration yet; just the types.
- [DONE] "Hello world" flow engine binary (`apps/agent`) that logs JSON structured output and exits cleanly.

**Proves:** the contracts compile, the workspace builds, CI is green. Boring on purpose.

## Stage 1 — Graph service

**Goal:** the node tree is alive. Nothing in later stages is possible without this.

**Status:** Done. In-memory substrate with 14 passing tests covering placement, cascade, events, and slot writes. Persistence lands in Stage 5; NATS subject mapping in Stage 6.

- [DONE] `/crates/graph/` — the core crate (see [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md))
- [DONE] `SlotMap`, `Link`, `NodePath`, `NodeId`, `KindId`, `Lifecycle` state machine with legal-transition table (no single `Node` trait — snapshot access via `NodeSnapshot`; internal `NodeRecord` owned by the store per the "graph is THE CORE" rule)
- [DONE] Kind registry with reverse-DNS IDs (`sys.*`), facet flags (`Facet`/`FacetSet`), containment schema (`must_live_under`, `may_contain` with kind or facet matchers, `cardinality_per_parent`, `cascade`)
- [DONE] Placement enforcement on every mutation — single code path in `GraphStore::create_child` covering CRUD. Move / import / block sync paths land later and reuse the same validator.
- [DONE] Cascading delete with link-breakage semantics and `cascade: strict | deny | orphan` per kind (only `strict` and `deny` tested today — `orphan` is parseable but not wired)
- [DONE] Event bus: `NodeCreated`, `NodeRemoved`, `NodeRenamed`, `SlotChanged`, `LifecycleTransition`, `LinkAdded`, `LinkRemoved`, `LinkBroken` — in-process `EventSink` trait with `VecSink` (tests) and `NullSink`. Adapter from `EventSink` to `messaging::MessageBus` will live in the agent composition root. NATS subject mapping in Stage 6.
- [DONE] Seed kinds registered: `sys.core.station` (root, cascade=deny), `sys.core.folder` (free container), `sys.compute.math.add` (free leaf), plus the bound-kind demo trio (`sys.driver.demo`, `sys.driver.demo.device`, `sys.driver.demo.point`) proving placement rules end-to-end.
- [DONE] `tracing` wired into the registry; agent binary already emits structured JSON logs (from Stage 0).

**Proves:** the substrate works. Kinds register, bound nodes refuse wrong placement, free nodes drop anywhere, deletes cascade correctly, events fire.

## Stage 2 — Engine on top of the graph

**Goal:** crossflow executes flows that read and write graph slots.

**Status:** Stage 2a done (crossflow-independent scope). Stage 2b (crossflow vendoring + flow-document execution) still blocked on the upstream URL + pin from the Stage 0 deferred item. The live-wire executor, engine state machine, safe-state plumbing, and SIGTERM handling all ship now; flow-document execution bolts on beside them without replacing them.

### Stage 2a — shipped

- [DONE] `/crates/engine/` — engine crate with 8 focused files (`engine.rs`, `state.rs`, `queue.rs`, `live_wire.rs`, `safe_state.rs`, `kinds.rs`, `error.rs`, `lib.rs`), each under 200 lines
- [DONE] `sys.core.flow` kind registered — flow containers live in the graph, may hold compute nodes and nested flows (facet `IsFlow`)
- [DONE] Flow-internal kinds `sys.engine.read_slot` / `sys.engine.write_slot` registered with their config + I/O slot schemas; execution behaviour arrives in Stage 2b
- [DONE] **Live-wire executor** — `SlotChanged` → `links_from(source)` → target writes, with fixed-point cycle short-circuiting (skip writes when target already holds the incoming value). Niagara-style "simple case, no flow wrapper" per RUNTIME.md
- [DONE] **Engine state machine** — full 7-state form from RUNTIME.md: `Stopped → Starting → Running ↔ (Pausing → Paused → Resuming) → Stopping → Stopped`, with legal-transition table and explicit `IllegalTransition` error. `Running` is the only propagating state
- [DONE] **Async worker** on a tokio task consuming the paired `UnboundedReceiver<GraphEvent>` — decouples synchronous graph emits from async propagation, avoids re-entrant stack recursion
- [DONE] `queue::channel()` — the `EventSink` + `Receiver` pair the agent hands to `GraphStore::new` and `Engine::new`. Unbounded for Stage 2; Stage 7 replaces it with the bounded outbox per RUNTIME.md
- [DONE] **Safe-state policy** — `SafeStatePolicy { Hold, FailSafe{value}, Release }`, `OutputDriver` async trait, `NoopOutputDriver` default, `SafeStateBinding` registry. Applied on every `Stopping` transition, failures logged and shutdown continues
- [DONE] **Graceful SIGTERM / SIGINT** in `apps/agent` — agent composition wires the queue → graph → engine, starts the engine, awaits either signal, calls `engine.shutdown().await`. Falls back to Ctrl-C when SIGTERM is unavailable
- [DONE] Integration tests (`crates/engine/tests/live_wire.rs`) — 6 scenarios: start/stop, pause-blocks-propagation + resume-restores, fan-out, fixed-point cycle quiescence, post-shutdown writes don't panic, illegal transitions return errors
- [DONE] Additive accessors on `GraphStore` — `links_from(SlotRef)` (hot path) and `links()` (introspection)
- [DONE] Workspace lints tightened: `cargo clippy --workspace --all-targets -- -D warnings` green across all 26 crates; `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]` applied to lib roots and integration test files where `unwrap()` is idiomatic
- [DONE] Full test suite: 40 tests pass, `cargo fmt --all --check` clean, `cargo build --workspace` clean

### Stage 2b — deferred

- [DEFERRED] Vendor crossflow from `open-rmf/crossflow` at a pinned commit (blocked on URL + pin from Stage 0). Once landed, the diagram loader and crossflow-service registry wire into `crates/engine` beside the existing live-wire path — they don't replace it
- [DEFERRED] `Read Slot` / `Write Slot` **execution** — kinds + placement exist today; behaviour needs crossflow services to implement them as flow-internal nodes
- [DEFERRED] Diagram loader for flow-container nodes — JSON → crossflow workflow graph
- [DEFERRED] End-to-end flow-document test: "flow subscribes to a demo-point slot, transforms the value, writes it back" — currently proved via live-wire; the flow-document version needs Stage 2b

### Stage 2c — engine-as-a-node (follow-up from 2a review)

**Goal:** eliminate the parallel-state antipattern. Engine lifecycle and safe-state policies currently live in engine-owned structs; promote both into the graph so flows, Studio, RBAC, and audit all observe them through the same machinery they use for everything else. See [EVERYTHING-AS-NODE.md § "The agent itself is a node too — no parallel state"](../design/EVERYTHING-AS-NODE.md).

**Why this matters.** Today a flow cannot subscribe to "engine entered Stopping" the way the EVERYTHING-AS-NODE doc advertises, because engine state lives in a private `Mutex<EngineState>`. Same for safe-state bindings — they're a `Vec` the engine owns, not queryable through the graph. Both are the exact shape the "everything is a node" rule exists to prevent.

- Register `sys.agent.self` kind (facets: `isSystem`, `isContainer`; `cardinality_per_parent: ExactlyOne` under its station)
- Register `sys.agent.engine` kind (facets: `isSystem`; must-live-under: `sys.agent.self`; `cardinality_per_parent: ExactlyOne`); status-role slots: `state` (string: Starting/Running/Paused/Stopping/Stopped), `last_transition_ts`, `flows_running`, `flows_paused`
- `Engine::transition(new)` writes to `sys.agent.engine.state` via `GraphStore::write_slot` — the emitted `SlotChanged` is the notification. Private `Mutex<EngineState>` becomes a derived read or goes away entirely.
- `SafeStatePolicy` becomes a **config-role slot** on each writable output's node. The in-memory `Vec<SafeStateBinding>` on the engine is replaced by `apply_safe_state` walking the graph (`kind.facets==IsWritable && config.safe_state.policy != null`).
- Seed the agent subtree on boot from `apps/agent` so every running agent has `sys.agent.self` + `sys.agent.engine` present.
- Integration test: a subscriber to `graph.<tenant>.<agent>.engine.slot.state.changed` receives transitions in order; killing the agent on shutdown leaves the DB in `Stopping` per write-through.
- Keep the async worker where it is — execution is an engine concern, state representation is a graph concern. Only state moves.

**Deliverable size**: ~150–200 LOC net, no new dependencies, no new stages blocked — the rest of the Stage 2a surface is untouched.

**Proves**: the "no parallel state" rule applies to the platform's own subsystems, not just user-visible entities. Unblocks flows subscribing to engine/agent state via the same fabric as everything else.

### Stage 2d — observability wiring (follow-up, after observability library ships)

**Goal:** route every `tracing::info!` / `warn!` / `error!` call in `engine` + `graph` + `apps/agent` through `observability::prelude` with the canonical-fields contract from [LOGGING.md](../design/LOGGING.md).

- Prerequisite: the observability library itself exists per its own scope (see the separate library-scaffold prompt).
- Replace direct `tracing::info!` callers with `observability::prelude::info!` (or equivalent) that enforces `node_path` / `kind_id` / `msg_id` / `flow_id` / `request_id` fields.
- Confirm the `println!` / `eprintln!` grep-lint from LOGGING.md passes on the touched crates.
- Engine's `sys.agent.engine.state` transitions (from Stage 2c) each emit an `info`-level log event with `canonical.kind_id = sys.agent.engine` and the from→to transition — one event per transition, correlatable via the same `request_id` that drove the transition.

**Proves**: the "one log format everywhere" thesis holds across the engine/graph/agent boundary before Stage 3 brings block logs into the same stream.

**Proves (today, for 2a):** the graph and engine are one system. Graph events drive propagation through the engine's worker without any special cases, the state machine is the canonical on/off switch for the runtime, and safe-state is a first-class shutdown concern from day one. **Proves (later, for 2b):** flow documents execute through crossflow against the same graph substrate that live-wire already uses.

## Stage 3 — The three node flavors + the shared SDK

**Goal:** Ship `blocks-sdk` (Rust) + `@sys/blocks-sdk-ts` (TypeScript) and validate that native, Wasm, and block-process nodes all execute in the same flow through one authoring API.

**Full scope in [NODE-SCOPE.md](NODE-SCOPE.md).** This section is the stage breakdown; NODE-SCOPE has the manifests, code, decision tables, and deliverable acceptance criteria. Read NODE-SCOPE before starting.

**Why now.** Stages 1–2 ship hand-wired kinds via the graph's internal `register_kind` API. That's fine while the surface is two people's code. The moment we add a third execution model (Wasm) or anyone outside the graph crate registers a kind, we need the real SDK — otherwise every downstream stage embeds an ad-hoc authoring convention that later has to be unwound. Stage 3 is also the earliest time where the contract surface is rich enough (Msg, manifests, capabilities, slots, containment) to make the SDK non-trivial.

**Prerequisite carry-over.** Any kinds registered ad-hoc in Stages 1–2 (seed kinds, flow-engine internal kinds) get migrated to use `#[derive(NodeKind)]` as part of this stage. That migration is the forcing function — if the SDK can't express those existing kinds cleanly, the SDK design is wrong.

Stage 3a is split into four sub-stages; each merges independently with fmt/clippy/tests green. The TS SDK is deferred to Stage 4 (per Stage 0's pnpm deferral); Rust-side wire-shape fixtures land in 3a-4 so the wire is locked before Stage 4 imports it.

### Stage 3a-1 — SDK skeleton + manifest-only migration [DONE]

The forcing function for the SDK design: if the SDK can't cleanly describe the kinds Stages 1–2 already ship, the design is wrong.

- [DONE] **Manifest types moved from `graph` to `spi`** — `KindId`, `NodeId`, `NodePath`, `Facet`/`FacetSet`, `ContainmentSchema`/`ParentMatcher`/`Cardinality`/`CascadePolicy`, `SlotRole`/`SlotSchema`, `KindManifest`. The graph crate retains only runtime state (`KindRegistry`, `SlotMap`/`SlotValue`, `NodeRecord`, events, store). Dep arrow is now `blocks-sdk → spi` (not through `graph`), satisfying NODE-SCOPE rule #1. No block crate transitively pulls in the graph runtime.
- [DONE] **`crates/blocks-sdk-macros`** proc-macro crate with `#[derive(NodeKind)]`. Reads the YAML manifest at compile time (path resolved relative to `CARGO_MANIFEST_DIR`), validates it parses as `spi::KindManifest`, checks the `kind` attribute matches the manifest's `id`, and emits `impl NodeKind for T`. Behaviour-less kinds declare `#[node(..., behavior = "none")]`; kinds with their own `impl NodeBehavior` declare `behavior = "custom"`. Omitting the attribute is a compile error — the forcing function so containers can't silently read as no-op behaviour kinds.
- [DONE] **`crates/blocks-sdk`** — `NodeKind` trait (declarative), `NodeBehavior` trait (declared; dispatch lands in 3a-2), `NodeCtx` stub, `NodeError`, `prelude`, contract-surface re-exports of `spi` types, and mutually-exclusive `native` / `wasm` / `process` features with a `compile_error!` guard enforcing "exactly one".
- [DONE] **Seed + flow-internal kinds migrated** — all nine kinds (`sys.core.station`, `.core.folder`, `.compute.math.add`, `.driver.demo`, `.driver.demo.device`, `.driver.demo.point`, `.core.flow`, `.engine.read_slot`, `.engine.write_slot`) now live as YAML manifests under `crates/{graph,engine}/manifests/` wired via `#[derive(NodeKind)]`. Each kind is a unit struct; registration is `kinds.register(<T as NodeKind>::manifest())`.
- [DONE] **Snapshot regression tests** (`crates/graph/tests/seed_snapshot.rs`, `crates/engine/tests/kinds_snapshot.rs`) pin the pre-migration `KindManifest` values via JSON-equality. A YAML edit that drifts placement rules, facets, or slot schemas surfaces as a diff against these files — 3b/3c cannot silently drift either.
- [DONE] `ParentMatcher` serde form switched to a single-key map (`{kind: x}` / `{facet: y}`) via hand-rolled `Serialize`/`Deserialize`; the prior `#[serde(tag = "by")]` did not round-trip through `serde_yml` for transparent-string inner types. Round-trip tests live alongside the type in `crates/spi/src/containment.rs`.
- [DEFERRED] **Move seed kinds from `graph` → `domain-*`** — mechanical relocation; left in place for 3a-1 to keep the diff focused. Seed kinds still register through the graph crate's `register_builtins`.
- [DEFERRED] **`requires!` macro** for capability declarations — the `spi::capabilities::{Requirement, SemverRange}` surface is rich and not consumed by any of the nine migrated kinds. Lands in 3a-2 together with the first behaviour kind that declares a required capability.

### Stage 3a-2 — NodeCtx + BehaviorRegistry + `sys.compute.count`

First real behaviour kind end-to-end through the new dispatch seam.

- [DONE] **`NodeBehavior` flipped to `&self`** — `crates/blocks-sdk/src/node.rs`. Trait methods take `&self`, not `&mut self`; the parallel-state antipattern at instance granularity is now a compile error. `Count` is a unit struct with no fields.
- [DONE] **`NodeCtx` native surface** — `crates/blocks-sdk/src/ctx.rs`. Self-scoped only: `read_status(slot)` / `read_config(slot)` / `update_status(slot, value)` / `emit(port, msg)` / `resolve_settings(msg)`. Graph access goes through the `GraphAccess` trait (mockable); emit goes through `EmitSink`. **Cross-node `read_slot(path, slot)` was deliberately dropped** — nodes communicate by message via ports, not by peeking at peers' slots (the doc's draft surface had it; that contradicted NODE-SCOPE encapsulation, so the dispatch surface scopes to self). `schedule` is a stubbed `Err` until 3a-3.
- [DONE] **`DynBehavior` / `TypedBehavior` adapter** — object-safe wrapper so the engine can hold `Arc<dyn DynBehavior>` per kind despite `NodeBehavior::Config` being an associated type.
- [DONE] **`ResolvedSettings<T>`** — `crates/blocks-sdk/src/settings.rs`. Merge order: schema defaults < persisted node config < `msg_overrides` from `msg.metadata`. Deref-to-`T` so behaviours treat it as the bare struct.
- [DONE] **`requires!` macro** — `crates/blocks-sdk/src/requires.rs`. Emits a `pub fn requires() -> Vec<Requirement>` (not `const`, because `SemverRange::parse` isn't const). First user: `domain_compute::requires!{ "spi.msg" => "1" }`.
- [DONE] **`spi::KindManifest` + `SlotSchema` extended** — added `trigger: bool` on `SlotSchema`, `settings_schema` / `msg_overrides` / `TriggerPolicy` (default `OnAny`) on `KindManifest`. Manifest YAML parses with the new fields optional; existing engine/seed manifests round-trip unchanged.
- [DONE] **`BehaviorRegistry` in `crates/engine`** — `crates/engine/src/behavior.rs`. Holds kind→`Arc<dyn DynBehavior>` plus per-`NodeId` config blobs, plus an internal `GraphAdapter` that implements both `GraphAccess` and `EmitSink` over `Arc<GraphStore>`. `emit(port, msg)` writes the message JSON to the source's output slot; the existing live-wire executor fans it out to linked targets — no separate dispatch path. Wired into `engine::Engine`'s worker loop alongside `LiveWireExecutor`; both fire on the same `SlotChanged` event. Filters: only `role: input` AND `trigger: true` slots dispatch — status/config writes don't re-enter the behaviour, which makes the slot-source regression even possible.
- [DONE] **`crates/domain-compute`** — new crate. `Count` (unit struct, `behavior = "custom"`) + `CountConfig` + pure `apply_step`. Manifest at `crates/domain-compute/manifests/count.yaml` declares two trigger inputs, one output, one status slot, settings schema with defaults, `msg_overrides: {step, reset, initial}`, `trigger_policy: on_any`. Public surface: `register_kinds(&KindRegistry)` + `behavior() -> Arc<dyn DynBehavior>`; the agent composition root calls both. The crate depends on `graph` (for the registry handle) but **not** `engine` (so the layering rule holds).
- [DONE] **Slot-source regression test** — `crates/domain-compute/tests/dispatch.rs::slot_source_regression_external_write_wins`. Initial=10, write `count` slot directly to 42, fire `in`, assert emitted output is **43, not 11**. Plus: increment, reset-via-port, reset-via-`msg.reset`, msg.step override, status-write-doesn't-recurse, `requires()` declares `spi.msg`, and `apply_step` arithmetic unit tests.
- [DEFERRED] **`NodeCtx::schedule` real impl** — stub now; lands in 3a-3 with `sys.logic.trigger`.
- [DEFERRED] **wasm/process `GraphAccess`/`EmitSink` impls** — the SDK feature gates are unchanged; native is the only adapter that has a real `BehaviorRegistry`. Wasm in 3b, process in 3c.
- [DEFERRED] **End-to-end test through `demo.point` → `count` → `demo.point`** — replaced by the focused unit-of-dispatch tests above. The graph integration is exercised by `slot_source_regression_external_write_wins` (uses real `GraphStore` + `BehaviorRegistry`); a multi-node wire test is cheap to add when `domain-devices` is migrated to the SDK.
- [DEFERRED] **Structured logger via `observability::prelude`** — `BehaviorRegistry::handle` uses `tracing::warn!` directly with canonical fields; flagged for Stage 2d's observability wiring to absorb.

### Stage 3a-3 — `NodeCtx::schedule` + `sys.logic.trigger`

- [DONE] **`TimerHandle` + `TimerScheduler` trait + `NodeBehavior::on_timer`** in `crates/blocks-sdk/src/ctx.rs` and `node.rs`. `on_timer` has a default no-op so existing kinds (count) need no changes. Handle is `pub struct TimerHandle(pub u64)` — opaque, `Copy`, hashable. `NodeCtx::schedule(delay_ms) -> TimerHandle` and `NodeCtx::cancel(h)` delegate through the trait.
- [DONE] **`Scheduler` in `crates/engine/src/scheduler.rs`** — tokio-backed one-shot timers; each `schedule` spawns a task that sleeps then sends `TimerFired { node, handle }` over an `mpsc::UnboundedSender`. Cancel via `AbortHandle`. The mpsc breaks the Scheduler ↔ `BehaviorRegistry` cycle (same pattern as graph events). `BehaviorRegistry::new` now returns `(Self, mpsc::UnboundedReceiver<TimerFired>)`.
- [DONE] **`BehaviorRegistry::dispatch_timer`** + worker-loop wiring — `Engine::start` takes both the `GraphEvent` and `TimerFired` receivers; `worker_loop` selects over events / control / timers and routes each to the right dispatcher.
- [DONE] **`crates/domain-logic`** — new crate. `Trigger` (unit struct, `behavior = "custom"`) + `TriggerConfig` + `TriggerMode { Once, Extend, ManualReset }`. Manifest at `crates/domain-logic/manifests/trigger.yaml` matches NODE-SCOPE: two trigger inputs, one output, status slots `armed` + `pending_timer`, settings schema with mode / payloads / delay, `msg_overrides: {delay_ms→delay, trigger_payload→trigger, reset_payload→reset_value}`.
- [DONE] **State lives in slots** — `armed` (bool) and `pending_timer` (nullable u64) are status slots, not struct fields. The `on_timer` handler reads `pending_timer` from the slot and ignores stale fires whose handle no longer matches — defends against the cancel-vs-fire race without per-instance state.
- [DONE] **Deterministic-time tests** in `crates/domain-logic/tests/dispatch.rs` — every timer-dependent test runs under `#[tokio::test(start_paused = true)]` and uses `tokio::time::advance` instead of wall-clock waits. The full engine worker drives dispatch (real `Scheduler` → mpsc → `dispatch_timer`), so the integration is the same code path the agent runs in production.
- [DONE] **Slot-source regression for `armed`** — `armed_slot_source_regression`. Out-of-band write to `armed = false` mid-window must let the next input emit again; a struct-field cache would still ignore the input.
- [DEFERRED] **Reusable test harness in `crates/engine/src/test_support.rs`** — the per-test `setup_with(config)` helper in `domain-logic/tests/dispatch.rs` is small enough that promoting it now would be premature. Lift into `engine` only when the second consumer (`sys.compute.timer`, `sys.io.http_in`, …) lands.
- [DEFERRED] **wasm/process `TimerScheduler` impls** — stubs only; real impl in 3b/3c when those adapters land.

### Stage 3a-bonus — Manual-test HTTP surface

A pull-forward from Stage 9's `transport-rest` work, scoped to *manual testing*. Lets operators drive the running agent by hand before Studio (Stage 4) lands. Not the final public API surface — that gets OpenAPI, auth, pagination, etc. — but the URL shape, versioning, and capability manifest are the real ones, so this slot doesn't get re-cut later.

- [DONE] **`crates/transport-rest`** — axum router under `/api/v1/` per VERSIONING § "Public API":
  * `GET  /healthz` (unversioned — orchestrator probe)
  * `GET  /api/v1/capabilities`
  * `GET  /api/v1/nodes`, `GET /api/v1/node?path=…`, `POST /api/v1/nodes`
  * `POST /api/v1/slots`, `POST /api/v1/config`
  * `GET  /api/v1/events` (SSE)
  * `GET  /` (manual-test UI)
- [DONE] **`AgentSink`** — composite `EventSink` fanning every graph event to both the engine mpsc (live-wire + behaviour dispatch) and a bounded `tokio::sync::broadcast` (SSE). Slow SSE consumers lag, never block the engine.
- [DONE] **Capability manifest** at `/api/v1/capabilities` per VERSIONING § "Host-provided capability manifest". Returns platform version, REST API version, flow/node schema versions, and the host-provided `spi.*` + `data.sqlite` capability list. `runtime.wasmtime` and `runtime.extension_process` are deliberately absent — blocks that require them refuse to install today rather than failing at runtime. List is hand-maintained in `crates/transport-rest/src/capabilities.rs` with a comment pointing at `blocks-host::capability_registry` as the proper home once it lands.
- [DONE] **`GraphStore::snapshots()`** — list-all helper backing `GET /api/v1/nodes`.
- [DONE] **Static UI** at `crates/transport-rest/static/index.html` — vanilla JS, no build step, ~200 lines. Lists nodes, writes slot values inline, streams events into a colour-coded log, displays the agent's `agent X · api vN · flow_schema=… node_schema=…` versions in the header.
- [DONE] **Agent bootstrap** — `apps/agent/src/main.rs` registers `domain-compute` + `domain-logic` behaviours, binds `--http` (default `127.0.0.1:8080`), runs the engine and the HTTP surface concurrently, shuts both down on SIGTERM/SIGINT.
- [DONE] **Tests** — `transport-rest::capabilities::tests` pin `spi.msg@^1` matches and `runtime.wasmtime` is *intentionally* missing (so a future re-add doesn't slip in unannounced).
- [DONE] **Links endpoints** — `GET /api/v1/links`, `POST /api/v1/links` (endpoint addressed by `{path, slot}` or `{node_id, slot}`), `DELETE /api/v1/links/:id`. New `GraphStore::remove_link` emits `LinkRemoved` (never `LinkBroken` — that's reserved for cascade-delete).
- [DONE] **`POST /api/v1/lifecycle`** `{path, to}` — drives `GraphStore::transition`; accepts the `Lifecycle` snake_case form (`active`, `disabled`, …).
- [DONE] **`POST /api/v1/seed`** `{preset: "count_chain" | "trigger_demo"}` — one-click preset that creates a folder subtree, seeds default configs, fires `on_init`, and wires the chain. Lands the first end-to-end browser demo: seed → write `in` → count emits → trigger arms in the same browser session.
- [DONE] **SVG visual graph in the UI** — layered (depth by path-slash count), cubic-bezier links, click-to-select, per-node lifecycle dropdown + transition button, link-creation form, links list with unlink buttons, seed preset buttons in header. Still ~400 lines of vanilla JS; Studio proper lands in Stage 4.
- [DEFERRED] **OpenAPI / `utoipa`** — the original lib.rs comment named utoipa as the eventual schema source. Slots into Stage 9 (Public API + versioning) when external SDKs start consuming this surface.
- [DEFERRED] **Move capability list into `blocks-host::capability_registry`** — current location is acknowledged-stopgap, not architectural. Stage 3c lands the registry; this list moves at that point.

### Stage 3a-4 — Wire-shape contract fixtures (Rust side) [DONE]

- [DONE] **Hand-authored fixture set at `/clients/contracts/fixtures/`** — the cross-language source of truth, per the existing `/clients/contracts/README.md`. Five `Msg` variants (`bare-payload`, `with-topic-and-source`, `with-parent`, `with-custom-fields`, `null-payload`) and eight `GraphEvent` variants (one per enum variant: `node_created` / `node_removed` / `node_renamed` / `slot_changed` / `lifecycle_transition` / `link_added` / `link_removed` / `link_broken`). Pre-existing stale fixtures (ISO-string `ts`, PascalCase `type`, non-UUID `_msgid`) were replaced to match the current Rust wire.
- [DONE] **Round-trip tests** — `crates/spi/tests/contract_fixtures_msg.rs` for `Msg`, `crates/graph/tests/contract_fixtures_events.rs` for `GraphEvent`. Each reads every `*.json`, deserialises, re-serialises, and asserts structural equality (parse both sides as `serde_json::Value`, `assert_eq`). Field order is intentionally not part of the contract.
- [DONE] **Variant-coverage guard** — `every_variant_has_a_fixture` fails if a new `GraphEvent` variant lands without a fixture, forcing the author to think about how it serialises.
- [DEFERRED] **`@sys/blocks-sdk-ts` round-trip** — Stage 4. The TS schemas in `/clients/ts/src/schemas/` are currently stale (they matched the old fixtures) and will be regenerated / aligned when Stage 4 begins, using these fixtures as the target.

**Decisions locked (see `docs/NEXT.md` history):**
- **Structural equality, not byte-level**: the contract is "same JSON value", not "same field order". Avoids forcing both sides to agree on a canonical order and keeps serde-struct-order changes non-breaking.
- **`_ts` is hand-authored per fixture**: every fixture carries an explicit millisecond value; round-trip preserves it. No wall-clock fixtures, no magic placeholders.

### Stage 3b — Wasm flavor

- **Wasmtime runtime in `crates/blocks-host`** — loads `.wasm` modules, enforces fuel metering + memory caps, exposes host-function allowlist (`emit`, `read_slot`, `update_status`, `log`, `call_extension`, `schedule`)
- **Wasm adapter feature in `blocks-sdk`** — same `NodeBehavior` trait, wasm32-unknown-unknown target, host-function imports bound via the SDK
- **Wasm Provider trait** abstracted so a browser adapter (`web-sys`-backed) can land later without changes to block authors' code
- **Example: `sys.wasm.math_expr`** — a math-expression evaluator, taking expression in config + variables in `msg.payload`, returning the evaluated value
- **End-to-end test** — wasm module compiled in CI, loaded at runtime, fuel-exhaustion and OOM traps produce structured `NodeError` instead of killing the agent

### Stage 3c — Process block flavor

- **Block supervisor in `crates/blocks-host`** — spawn, health-check, restart with exponential backoff, cgroup memory limits, UDS socket setup
- **gRPC implementation of `spi/proto/block.proto`** — `describe` / `discover` / `subscribe` / `invoke` / `health` per the existing contract
- **Process adapter feature in `blocks-sdk`** — `extensions_sdk::run_process_plugin()` one-liner in a block's `main.rs`; SDK multiplexes every registered kind behind it
- **Example: `com.example.pg.query`** — Postgres query node with parameterised SQL, timeout, structured error output
  - Backend binary shipped with manifest + capability declarations
  - `must_live_under: [com.example.pg.connection]` — proves the containment rules cross the process boundary
  - **UI (Module Federation bundle) is deferred to Stage 4** when Studio + MF land; the TS SDK's `defineExtension` entry point is already in place so the bundle can be authored without blocking
- **End-to-end test** — agent spawns block, executes a query-flow end-to-end, kills the block mid-run and asserts the agent survives + restarts it

### Deferred to Stage 4

- The `com.example.pg.query` MF UI bundle (schema-aware table picker + results viewer) — written against `@sys/blocks-sdk-ts` but only wired into the Studio once the Studio shell exists. Serves as Stage 4's "untrusted federated block" acceptance case.
- Browser Wasm provider (`web-sys`-backed) — the abstraction is in place in 3b, the implementation lands alongside the Studio build.

**Proves:** one authoring API (the SDK) covers all three execution models; the Msg wire shape is machine-verified to match between Rust and TS; block authors write `NodeBehavior` and pick packaging via one Cargo feature; existing Stage 1–2 kinds port to the SDK cleanly. This is the biggest technical unknown — validate it early and hard.

## Stage 4 — Studio shell + Module Federation

**Goal:** Studio loads a UI block at runtime.

- Tauri shell renders basic UI
- React Flow canvas with the three node types from Stage 3
- Hardcoded initial graph, "Run" button triggers engine via IPC
- Module Federation wiring in Rsbuild — React declared as required shared singleton
- **Two federated modules built by separate pipelines** (not just co-located): a trusted one loaded into the host realm, an untrusted one loaded into an iframe with postMessage bridge. Both contribute a property panel via `@sys/blocks-sdk-ts`'s `defineExtension` entry (shipped in Stage 3). Proves both the host-realm and iframe isolation paths before we build blocks on them.
- **Wire in the deferred Stage 3c MF UI bundle for `com.example.pg.query`** — schema-aware table picker + results viewer, serves as the "untrusted federated block" acceptance case.
- Plain service registry over React Context (no InversifyJS); verify it's visible across the MF boundary in the trusted path, and correctly *not* visible across the iframe boundary
- Schema-driven forms (`@rjsf/core`) reading `node.schema.json` — consumed via the `PropertyPanel` component shipped in Stage 3
- **Browser Wasm provider** — `web-sys`-backed implementation of the Wasm Provider trait introduced in Stage 3b, so Wasm nodes can run in Studio previews

**Proves:** Module Federation loads third-party UI into the host, *and* the iframe isolation path works for untrusted code. The TS SDK authored in Stage 3 is the same one federated blocks use — no hidden Studio-only surface. The other unknown.

## Stage 5 — Persistence

**Goal:** The graph and its flows survive restart. Same repo traits, two native-shaped backends.

**Status:** Stage 5a done (SQLite path + GraphStore write-through). Stage 5b (Postgres impl, tags, audit event log, telemetry seam, YAML config) deferred — all sit behind the same `GraphRepo` trait, so they bolt in without re-working the graph crate.

### Stage 5a — shipped

- [DONE] **`GraphRepo` trait in `data-repos`** — sync-only (matches `GraphStore`'s sync surface), DTO-based (`PersistedNode` / `PersistedSlot` / `PersistedLink` / `GraphSnapshot`) so `data-repos` has no reverse dep on `graph`. See [`crates/data-repos/src/graph_repo.rs`](../../crates/data-repos/src/graph_repo.rs)
- [DONE] **Shared trait-test harness** behind `data-repos` feature `testing` — empty snapshot, roundtrip, delete, generation-bump. `data-sqlite` runs it under `[dev-dependencies]` with `features = ["testing"]` so any future backend (Postgres, in-memory mock) picks up the same acceptance suite
- [DONE] **`SqliteGraphRepo` in `data-sqlite`** using `rusqlite` (bundled build). Single-connection `Mutex<Connection>` matches SQLite's single-writer model; explicit transactions on multi-row deletes; WAL journal + FK enforcement + busy timeout configured on open
- [DONE] **Forward-only migrations** keyed off `PRAGMA user_version` — no external dependency, append-only SQL blocks, v1 ships the `nodes` / `slots` / `links` tables with materialised-path index per EVERYTHING-AS-NODE.md § "Persistence". Rollback explicitly unsupported — rollback is the deprecation-window pattern from VERSIONING.md
- [DONE] **`GraphStore::with_repo(kinds, sink, repo)`** constructor — restores state on startup, reconstructing the in-memory tree in parent-before-child order (materialised paths sort lexicographically so `ORDER BY path` is sufficient). Rejects restoration if the DB references a kind the registry doesn't know
- [DONE] **Write-through mutations** — every `create_root` / `create_child` / `delete` / `write_slot` / `add_link` / `transition` calls the repo *before* touching memory. Backend failure returns `GraphError::Backend(_)` and leaves memory untouched; proved by a `FlakyRepo` test that refuses writes and asserts `store.len() == 0` afterward
- [DONE] **`persist` module in `graph`** — mapping between graph types and DTOs, lifecycle/slot-role string codecs, error-wrapping helper. Isolated from `store.rs` so mutation paths stay readable
- [DONE] **`SlotMap::current_generation` + `restore`** helpers — let the store compute the next generation for the repo call before committing to memory, and let the persist path seed a slot with its historic generation without bumping
- [DONE] **Integration tests** — 3 scenarios in `crates/graph/tests/persistence.rs`: full roundtrip (5-node tree + slot write + link → close → reopen → verify), `UnknownKind` rejection on restore, `FlakyRepo` proving backend failure leaves memory clean. Plus 2 file-backed tests in `crates/data-sqlite/tests/repo.rs`
- [DONE] **Agent wiring** — `AGENT_DB=<path>` env var opens a file-backed SQLite; unset keeps the in-memory path. On boot the station root is created only if absent, so a restored DB seamlessly re-enters service. Smoke-tested: agent boots, takes SIGTERM, DB file persists on disk at 49 KB with the restored schema
- [DONE] Workspace: `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean, 45 tests passing

### Stage 5b — deferred (same seam)

- [DEFERRED] **Postgres-native repo impl** in `data-postgres` — UUID, TIMESTAMPTZ, JSONB, partial/GIN indexes, `ltree` for real subtree queries, tenant RLS. Separate migration set per the "no LCD tax" rule; the trait contract is identical, so restoration + write-through in `GraphStore` reuses unchanged
- [DEFERRED] **`tags` and `node_events` tables** — tags wait on an actual query need; `node_events` waits on the audit stream work that sits naturally with Stage 8 (auth / verified identity in audit entries)
- [DEFERRED] **`TelemetryRepo` seam** — the `data-tsdb` crate already exists; Stage 5b adds the trait + rolling-SQLite edge impl. Cloud TimescaleDB impl lands with Stage 7 messaging
- [DEFERRED] **YAML config loader** — picks the backend by connection string. `config` crate is reserved for it; until it exists, `AGENT_DB` is the one knob
- [DEFERRED] **Postgres-specific tests** — RLS, ltree subtree queries, JSONB indexing. SQLite-specific tests (WAL concurrent reader during writer) also deferred here

**Proves (today, for 5a):** the repo trait is real, SQLite is a fully working backend, the graph store is durable, write-through is atomic against a refusing backend, and the agent rehydrates on restart. **Proves (later, for 5b):** Postgres fits the same trait without bending it, and telemetry takes a separate path.

## Stage 6 — Deployment profiles

**Goal:** Single binary, `--role` selects behavior.

**Status:** Stage 6a done (runtime role + full config precedence). Stage 6b (cross-compile to aarch64 + ARM memory measurement) deferred — needs real hardware. All compile-time feature-gate seams in place; deep role-specific code paths accumulate in later stages behind them.

### Stage 6a — shipped

- [DONE] **Role enum** in the `config` crate (`Role::Standalone` / `Edge` / `Cloud`) with stable `as_str` and `FromStr` codecs. Capability methods (`runs_engine`, `serves_control_plane`, `expects_persistence`) are the seams later stages branch on \u{2014} all three current roles start the engine today, but the agent's `bootstrap` already consults `role.runs_engine()` so future roles (Studio-only, API gateway) slot in without touching call sites
- [DONE] **Full config precedence `cli > env > file > defaults`** via overlay types (`AgentConfigOverlay`, `DatabaseOverlay`, `LogOverlay`). Layers compose with `merge_over`; `resolve(default_db_path_for)` fills in the concrete `AgentConfig`. Role-aware defaults: edge / standalone get `./agent.db`, cloud leaves DB `None` until the Stage 5b Postgres connection-string variant arrives
- [DONE] **YAML file loader** (`from_file`) using `serde_yml`. `deny_unknown_fields` on every overlay struct so typos in the file surface at parse time, not via silent defaults
- [DONE] **Environment layer** (`from_env`) reading the documented subset: `AGENT_ROLE`, `AGENT_DB`, `AGENT_LOG`. Empty strings treated as unset; invalid role value returns `ConfigError::Invalid` rather than falling through
- [DONE] **Clap derive CLI** in `apps/agent`: `--role`, `--config <PATH>`, `--db <PATH>`, `--log <DIRECTIVE>`. Every flag has a corresponding file field; operators pick their source. `agent --help` renders correctly, `--version` wired via Cargo metadata
- [DONE] **Agent integration** \u{2014} the binary composes the three overlays in precedence order, uses the resolved config to pick the DB path and log filter, and logs the resolved role on startup. Smoke-tested: `agent --config foo.yaml` with `AGENT_DB` env set and `--db` flag on the CLI correctly hits CLI > env > file resolution
- [DONE] **Compile-time feature flags already present** in `apps/agent/Cargo.toml` (`role-edge`, `role-cloud`, `role-standalone`). Stage 6a does not add role-gated code paths \u{2014} it establishes the runtime selector; later stages (Postgres driver, MCP server, native protocol blocks) compile in or out behind these features
- [DONE] 6 new config unit tests (precedence, defaults, cloud no-default-DB, YAML parse, unknown-field rejection, partial-YAML layering); 51 total across the workspace. `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` clean

### Stage 6b — deferred

- [DEFERRED] **Cross-compile the edge binary to `aarch64-unknown-linux-gnu`** via `cross` or a Docker toolchain. The target spec exists in OVERVIEW.md; the workflow waits on CI (itself deferred from Stage 0)
- [DEFERRED] **ARM memory soak** \u{2014} measure RSS on real Raspberry Pi / industrial gateway hardware under the 350 MB target. Best run alongside the 24h flat-memory soak test scheduled for Stage 13 "Operations surface" / Stage 17 "Hardening"
- [DEFERRED] **Browser / Studio feature strip** \u{2014} actually proving a build that excludes native-only crates compiles to wasm. Lands with Stage 4 (Studio shell)

**Proves (today, for 6a):** the "one binary, three roles, config from four sources" story is real in code. The role enum and overlay types are stable contracts that future stages hang code off. **Proves (later, for 6b):** the edge binary fits its memory budget on real hardware.

## Stage 7 — Messaging backbone

**Goal:** NATS wired end-to-end.

- Embedded NATS in standalone mode
- NATS leaf node configuration for edge role (Core only by default; JetStream opt-in where hardware allows)
- NATS cluster with JetStream for cloud role
- **NATS accounts per tenant**, subject taxonomy defined with tenant IDs baked in (`tenant.{id}.telemetry.*`, `tenant.{id}.commands.*`, etc.). Studio's WebSocket connection is recognised as tenant-facing — subject permissions derive from Zitadel JWT claims.
- Message bus trait's NATS implementation replaces the in-process stub
- **Graph events mapped to NATS subjects** per EVERYTHING-AS-NODE.md (`graph.<tenant>.<path>.created`, `.slot.<slot>.changed`, `.lifecycle.<from>.<to>`, etc.) — wildcards let flows subscribe to subtrees
- Live telemetry: engine publishes flow execution events, Studio subscribes via WebSocket
- **TSDB for cloud telemetry** wired in: `TelemetryRepo`'s Postgres placeholder from Stage 5 replaced with a TimescaleDB impl (hypertables, continuous aggregates, retention policies). Edge keeps its rolling-SQLite impl.
- **Outbox with bounded disk/age and backpressure signals** (see RUNTIME.md): overflow policy per subject class, health events published when the outbox approaches caps so flows can shed load.
- Reverse-tunnel verified: cloud sends a command through leaf back to edge agent

**Proves:** fleet messaging, offline operation, tenant-scoped subject RBAC, and telemetry-at-scale all work.

## Stage 8 — Auth

**Goal:** Zitadel protecting the public API.

- Zitadel running locally via Docker for dev
- OIDC with PKCE in the Studio via `oidc-client-ts`
- JWT verification on the Control Plane (JWKS fetching and caching, 24h stale ceiling per AUTH.md)
- Edge agent verifies JWTs offline using cached JWKS; long-lived service-account tokens for the agent's own identity
- RBAC claims checked against route-level requirements; revocation deny-list consumed over NATS
- **Zitadel users mirrored as `sys.auth.user` nodes** in the graph; roles and service accounts likewise — identity becomes part of the unified tree, permission changes fire graph events
- Audit log entries include verified identity

**Proves:** auth works edge + cloud + browser + desktop, with offline verification.

## Stage 9 — Public API + versioning

**Goal:** Documented, versioned API that clients can consume.

- `/api/v1/*` routes for flow CRUD, device management, deployment
- `utoipa` derive macros generating OpenAPI spec from handler code
- Generated TypeScript SDK used by the Studio
- Generated Python SDK
- Deprecation header support, structured `api-changes.yaml`
- Versioning rules enforced in CI (CI fails if you remove a field from a v1 type)
- gRPC-Web transport for the Studio via Connect

**Proves:** the API contract is real, generated, versioned, and consumed the same way by internal and external clients.

## Stage 10 — Block lifecycle

**Goal:** First real block shipped end-to-end through the full pipeline.

- Block manifest format finalized, including the **node-kind declarations** each block contributes (kind IDs, facets, containment schemas, slot schemas) and the **required-capabilities block** (see [VERSIONING.md](../design/VERSIONING.md))
- **Host capability manifest** exposed at `GET /api/v1/capabilities` and `yourapp capabilities` — enumerates `spi.*`, `runtime.*`, `feature.*`, `data.*` with versions
- **Install-time compat match** — the agent refuses to install blocks whose required capabilities aren't provided, with structured errors naming each missing capability. `yourapp ext check <manifest>` is the dry-run command
- **Kind migrations** — kinds ship forward migrations; the graph service walks them on flow load; old-flow fixture suite runs on every release
- Signing pipeline: blocks signed on publish, verified on install
- Block registry in the Control Plane
- `yourapp ext publish` / `install` / `enable` / `disable` / `check` / `upgrade` commands
- Capability-based permission system: manifest declares permissions and required capabilities; user approves on install
- **Block installation registers new kinds with the graph crate**; uninstall refuses if instances still exist (per `cascade: deny` semantics) unless forced
- One real protocol block — pick MQTT as the lowest-risk (pure Rust stack exists). Kinds: `com.example.mqtt`, `com.example.mqtt.topic`
- Hot path: engine fetches block binary on sync, Studio fetches UI bundle on flow open
- Rollback: old version retained, one-command revert
- **Deprecation plumbing** in place — `deprecated_since` / `removal_planned` honoured by the installer with warnings during the window

**Proves:** the full block shape works in production. Everything after this is adding more blocks.

## Stage 11 — CLI

**Goal:** The agent binary is also the CLI.

- `clap` command tree: `run`, `flow`, `device`, `ext`, `login`, `config`, `mcp`
- `--output json|yaml|table` on every command
- `--local` vs `--remote` targeting
- Config file at `~/.config/yourapp/config.toml`
- Shell completions generated for bash, zsh, fish, powershell
- `yourapp login` does the OIDC device flow

**Proves:** the CLI story. Also unlocks scripting for the team.

## Stage 12 — BACnet block (exemplar protocol integration)

**Goal:** Ship a real, non-trivial protocol block end-to-end. BACnet is a useful exemplar because it's messy — discovery, subscriptions, priority arrays, licensing — and if the block model handles it cleanly, it handles most other integrations too. This stage is about proving the platform, not about being a BAS product.

- Separate binary using a BACnet library (research options — this is a real dependency decision)
- **Kinds contributed:** `sys.driver.bacnet` (isProtocol, isDriver, isContainer), `sys.driver.bacnet.device` (isDevice, bound under driver), `sys.driver.bacnet.point` (isPoint, isWritable, bound under device). Placement rules enforced by the graph service — points can't be created outside a device.
- Device discovery (BACnet Who-Is / I-Am) populates device nodes under the driver; discovered points populate point children
- Point read/write via BACnet services, exposed as slot read/write on point nodes
- Priority array support for safe-state behavior, mapped to per-slot safe-state policies
- Subscription via change-of-value or polling; COV events become `SlotChanged` graph events
- Block's UI block contributes BACnet device/point pickers, generated from the kind + slot schemas
- **Commissioning mode** honored per RUNTIME.md — writes allowed to points explicitly designated as commissioning points; dry-run/simulation is a separate mode
- Cascading delete verified: deleting a device removes its points and emits `LinkBroken` to any flows wired to those point slots

**Proves:** the block architecture handles a messy real-world protocol end-to-end through the node model. The "everything is a node" claim survives contact with reality. Other protocol integrations (Modbus, OPC-UA, MQTT bridges) follow the same shape; non-protocol integrations (REST APIs, databases, message queues, SaaS) are simpler versions of the same pattern.

## Stage 13 — Operations surface

**Goal:** Production-grade, always-on runtime behaviour.

- Engine state machine exposed via API and UI
- Flow-level pause/resume with audit
- Safe-state policies per writable output (hold / fail-safe / release)
- Watchdog-based flow health (missed ticks → unhealthy)
- Memory soak test in CI (24h+ run asserting flat RSS)
- **Two distinct modes, not one:** simulation/dry-run (inputs synthetic or recorded, all outputs suppressed) and commissioning (live inputs, writes allowed only to explicitly designated commissioning points). Separate UI affordances, separate audit events. See [RUNTIME.md](../design/RUNTIME.md) — do not merge these.
- systemd unit with `Restart=always` and watchdog pings
- Crash-resume: flows restart in their last known state

**Proves:** it behaves like a production service, not just a dev demo.

## Stage 14 — MCP server

**Goal:** Off-by-default MCP adapter.

- `rmcp` SDK integration
- Expose resources (flows, devices, logs, telemetry)
- Expose tools (deploy, query, test-run)
- Three-layer off switch: feature flag, config disable, runtime toggle
- `127.0.0.1`-only default, auth required, per-tool RBAC
- Every MCP call audited with session ID

**Proves:** the 2026 table-stakes capability is there without compromising security defaults.

## Stage 15 — Fleet orchestration

**Goal:** Cloud can manage many edge agents.

- Device registration and provisioning
- Flow deployment targets (single device, device group, tenant-wide)
- Rollout policies: canary, staged, fleet-wide
- Rollback on failure detection
- Fleet-wide scatter-gather queries via NATS
- Agent health dashboard in the Studio

**Proves:** the multi-site / fleet story. This is the feature that differentiates from single-device flow tools like Node-RED.

## Stage 16 — Cross-platform Studio

**Goal:** Studio ships to all targets.

- Windows `.msi` installer + auto-update
- macOS notarized `.dmg` (universal binary)
- Linux `.AppImage`, `.deb`, `.rpm`
- Browser build served by Control Plane
- Runtime feature detection branches for Tauri vs browser
- Deep links, notifications, system tray on desktop
- Release pipeline producing signed artifacts per platform

**Proves:** the cross-platform story holds. Mobile is explicitly deferred to post-v1.

## Stage 17 — Hardening

**Goal:** Things you only find by running under pressure.

- Chaos testing: kill blocks, drop NATS tunnel, fill the disk, starve memory
- Long-running soak on real ARM hardware under realistic load
- Upgrade testing: v1 agent + v1 block → v2 agent + v1 block → v2 both
- Schema migration testing across flow format versions
- Penetration test on the public API and the MCP server
- Documentation audit: the docs match what the code actually does

**Proves:** ready for real customers.

## What's not in these stages

Deferred intentionally — all have stable seams so they add cleanly later:

- Mobile (iOS/Android Tauri) — post-v1
- Additional first-party blocks — each is its own mini-stage, same shape as BACnet; examples: Modbus, OPC-UA, MQTT bridges, HTTP/REST, SQL sources, common SaaS (Slack, email, webhooks)
- Domain-specific semantic layers (e.g. BAS ontologies, ITSM schemas, IoT device models) — if/when pursued, expressed as node facets/tags rather than a parallel ontology. Not in v1.
- Advanced fleet features (blue-green deploys, gradual rollouts, automated rollback policies) — v2

## One rule across all stages

**Every stage merges only if the previous stages still work.** No "we'll fix it later." Breakage caught at the seam you just crossed is cheap; breakage discovered three stages later is not.