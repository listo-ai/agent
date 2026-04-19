# Coding Stages

Staged so each stage produces something that runs and each stage proves a specific architectural risk. Skip no seams — stub implementations behind final interfaces.

## Stage 0 — Foundations

**Goal:** Repo, tooling, contracts. Nothing runs yet except tests.

**Status:** Rust side done; frontend / CI / crossflow vendoring deferred with explicit markers below. Workspace compiles, tests green, `agent` binary starts + exits cleanly.

- [DONE] Cargo workspace — all 26 crates from [CODE-LAYOUT.md](../design/CODE-LAYOUT.md) wired up under `/crates/`
- [DEFERRED] pnpm workspace — lands with Stage 4 (Studio shell) when frontend code is first needed
- [DEFERRED] CI: fmt, clippy, test, build for `aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-gnu` — repo builds locally; workflow to be added
- [DONE] `/crates/spi/` — the contracts, written before anything that uses them (path updated from the README's legacy `/packages/spi/`):
  - [DONE] `extension.proto` — gRPC schema for extensions (describe, discover, subscribe, invoke, health)
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
- [DONE] Kind registry with reverse-DNS IDs (`acme.*`), facet flags (`Facet`/`FacetSet`), containment schema (`must_live_under`, `may_contain` with kind or facet matchers, `cardinality_per_parent`, `cascade`)
- [DONE] Placement enforcement on every mutation — single code path in `GraphStore::create_child` covering CRUD. Move / import / extension sync paths land later and reuse the same validator.
- [DONE] Cascading delete with link-breakage semantics and `cascade: strict | deny | orphan` per kind (only `strict` and `deny` tested today — `orphan` is parseable but not wired)
- [DONE] Event bus: `NodeCreated`, `NodeRemoved`, `NodeRenamed`, `SlotChanged`, `LifecycleTransition`, `LinkAdded`, `LinkRemoved`, `LinkBroken` — in-process `EventSink` trait with `VecSink` (tests) and `NullSink`. Adapter from `EventSink` to `messaging::MessageBus` will live in the agent composition root. NATS subject mapping in Stage 6.
- [DONE] Seed kinds registered: `acme.core.station` (root, cascade=deny), `acme.core.folder` (free container), `acme.compute.math.add` (free leaf), plus the bound-kind demo trio (`acme.driver.demo`, `acme.driver.demo.device`, `acme.driver.demo.point`) proving placement rules end-to-end.
- [DONE] `tracing` wired into the registry; agent binary already emits structured JSON logs (from Stage 0).

**Proves:** the substrate works. Kinds register, bound nodes refuse wrong placement, free nodes drop anywhere, deletes cascade correctly, events fire.

## Stage 2 — Engine on top of the graph

**Goal:** crossflow executes flows that read and write graph slots.

**Status:** Stage 2a done (crossflow-independent scope). Stage 2b (crossflow vendoring + flow-document execution) still blocked on the upstream URL + pin from the Stage 0 deferred item. The live-wire executor, engine state machine, safe-state plumbing, and SIGTERM handling all ship now; flow-document execution bolts on beside them without replacing them.

### Stage 2a — shipped

- [DONE] `/crates/engine/` — engine crate with 8 focused files (`engine.rs`, `state.rs`, `queue.rs`, `live_wire.rs`, `safe_state.rs`, `kinds.rs`, `error.rs`, `lib.rs`), each under 200 lines
- [DONE] `acme.core.flow` kind registered — flow containers live in the graph, may hold compute nodes and nested flows (facet `IsFlow`)
- [DONE] Flow-internal kinds `acme.engine.read_slot` / `acme.engine.write_slot` registered with their config + I/O slot schemas; execution behaviour arrives in Stage 2b
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

- Register `acme.agent.self` kind (facets: `isSystem`, `isContainer`; `cardinality_per_parent: ExactlyOne` under its station)
- Register `acme.agent.engine` kind (facets: `isSystem`; must-live-under: `acme.agent.self`; `cardinality_per_parent: ExactlyOne`); status-role slots: `state` (string: Starting/Running/Paused/Stopping/Stopped), `last_transition_ts`, `flows_running`, `flows_paused`
- `Engine::transition(new)` writes to `acme.agent.engine.state` via `GraphStore::write_slot` — the emitted `SlotChanged` is the notification. Private `Mutex<EngineState>` becomes a derived read or goes away entirely.
- `SafeStatePolicy` becomes a **config-role slot** on each writable output's node. The in-memory `Vec<SafeStateBinding>` on the engine is replaced by `apply_safe_state` walking the graph (`kind.facets==IsWritable && config.safe_state.policy != null`).
- Seed the agent subtree on boot from `apps/agent` so every running agent has `acme.agent.self` + `acme.agent.engine` present.
- Integration test: a subscriber to `graph.<tenant>.<agent>.engine.slot.state.changed` receives transitions in order; killing the agent on shutdown leaves the DB in `Stopping` per write-through.
- Keep the async worker where it is — execution is an engine concern, state representation is a graph concern. Only state moves.

**Deliverable size**: ~150–200 LOC net, no new dependencies, no new stages blocked — the rest of the Stage 2a surface is untouched.

**Proves**: the "no parallel state" rule applies to the platform's own subsystems, not just user-visible entities. Unblocks flows subscribing to engine/agent state via the same fabric as everything else.

### Stage 2d — observability wiring (follow-up, after observability library ships)

**Goal:** route every `tracing::info!` / `warn!` / `error!` call in `engine` + `graph` + `apps/agent` through `observability::prelude` with the canonical-fields contract from [LOGGING.md](../design/LOGGING.md).

- Prerequisite: the observability library itself exists per its own scope (see the separate library-scaffold prompt).
- Replace direct `tracing::info!` callers with `observability::prelude::info!` (or equivalent) that enforces `node_path` / `kind_id` / `msg_id` / `flow_id` / `request_id` fields.
- Confirm the `println!` / `eprintln!` grep-lint from LOGGING.md passes on the touched crates.
- Engine's `acme.agent.engine.state` transitions (from Stage 2c) each emit an `info`-level log event with `canonical.kind_id = acme.agent.engine` and the from→to transition — one event per transition, correlatable via the same `request_id` that drove the transition.

**Proves**: the "one log format everywhere" thesis holds across the engine/graph/agent boundary before Stage 3 brings plugin logs into the same stream.

**Proves (today, for 2a):** the graph and engine are one system. Graph events drive propagation through the engine's worker without any special cases, the state machine is the canonical on/off switch for the runtime, and safe-state is a first-class shutdown concern from day one. **Proves (later, for 2b):** flow documents execute through crossflow against the same graph substrate that live-wire already uses.

## Stage 3 — The three node flavors + the shared SDK

**Goal:** Ship `extensions-sdk` (Rust) + `@acme/extensions-sdk-ts` (TypeScript) and validate that native, Wasm, and extension-process nodes all execute in the same flow through one authoring API.

**Full scope in [NODE-SCOPE.md](NODE-SCOPE.md).** This section is the stage breakdown; NODE-SCOPE has the manifests, code, decision tables, and deliverable acceptance criteria. Read NODE-SCOPE before starting.

**Why now.** Stages 1–2 ship hand-wired kinds via the graph's internal `register_kind` API. That's fine while the surface is two people's code. The moment we add a third execution model (Wasm) or anyone outside the graph crate registers a kind, we need the real SDK — otherwise every downstream stage embeds an ad-hoc authoring convention that later has to be unwound. Stage 3 is also the earliest time where the contract surface is rich enough (Msg, manifests, capabilities, slots, containment) to make the SDK non-trivial.

**Prerequisite carry-over.** Any kinds registered ad-hoc in Stages 1–2 (seed kinds, flow-engine internal kinds) get migrated to use `#[derive(NodeKind)]` as part of this stage. That migration is the forcing function — if the SDK can't express those existing kinds cleanly, the SDK design is wrong.

Stage 3a is split into four sub-stages; each merges independently with fmt/clippy/tests green. The TS SDK is deferred to Stage 4 (per Stage 0's pnpm deferral); Rust-side wire-shape fixtures land in 3a-4 so the wire is locked before Stage 4 imports it.

### Stage 3a-1 — SDK skeleton + manifest-only migration [DONE]

The forcing function for the SDK design: if the SDK can't cleanly describe the kinds Stages 1–2 already ship, the design is wrong.

- [DONE] **Manifest types moved from `graph` to `spi`** — `KindId`, `NodeId`, `NodePath`, `Facet`/`FacetSet`, `ContainmentSchema`/`ParentMatcher`/`Cardinality`/`CascadePolicy`, `SlotRole`/`SlotSchema`, `KindManifest`. The graph crate retains only runtime state (`KindRegistry`, `SlotMap`/`SlotValue`, `NodeRecord`, events, store). Dep arrow is now `extensions-sdk → spi` (not through `graph`), satisfying NODE-SCOPE rule #1. No plugin crate transitively pulls in the graph runtime.
- [DONE] **`crates/extensions-sdk-macros`** proc-macro crate with `#[derive(NodeKind)]`. Reads the YAML manifest at compile time (path resolved relative to `CARGO_MANIFEST_DIR`), validates it parses as `spi::KindManifest`, checks the `kind` attribute matches the manifest's `id`, and emits `impl NodeKind for T`. Behaviour-less kinds declare `#[node(..., behavior = "none")]`; kinds with their own `impl NodeBehavior` declare `behavior = "custom"`. Omitting the attribute is a compile error — the forcing function so containers can't silently read as no-op behaviour kinds.
- [DONE] **`crates/extensions-sdk`** — `NodeKind` trait (declarative), `NodeBehavior` trait (declared; dispatch lands in 3a-2), `NodeCtx` stub, `NodeError`, `prelude`, contract-surface re-exports of `spi` types, and mutually-exclusive `native` / `wasm` / `process` features with a `compile_error!` guard enforcing "exactly one".
- [DONE] **Seed + flow-internal kinds migrated** — all nine kinds (`acme.core.station`, `.core.folder`, `.compute.math.add`, `.driver.demo`, `.driver.demo.device`, `.driver.demo.point`, `.core.flow`, `.engine.read_slot`, `.engine.write_slot`) now live as YAML manifests under `crates/{graph,engine}/manifests/` wired via `#[derive(NodeKind)]`. Each kind is a unit struct; registration is `kinds.register(<T as NodeKind>::manifest())`.
- [DONE] **Snapshot regression tests** (`crates/graph/tests/seed_snapshot.rs`, `crates/engine/tests/kinds_snapshot.rs`) pin the pre-migration `KindManifest` values via JSON-equality. A YAML edit that drifts placement rules, facets, or slot schemas surfaces as a diff against these files — 3b/3c cannot silently drift either.
- [DONE] `ParentMatcher` serde form switched to a single-key map (`{kind: x}` / `{facet: y}`) via hand-rolled `Serialize`/`Deserialize`; the prior `#[serde(tag = "by")]` did not round-trip through `serde_yml` for transparent-string inner types. Round-trip tests live alongside the type in `crates/spi/src/containment.rs`.
- [DEFERRED] **Move seed kinds from `graph` → `domain-*`** — mechanical relocation; left in place for 3a-1 to keep the diff focused. Seed kinds still register through the graph crate's `register_builtins`.
- [DEFERRED] **`requires!` macro** for capability declarations — the `spi::capabilities::{Requirement, SemverRange}` surface is rich and not consumed by any of the nine migrated kinds. Lands in 3a-2 together with the first behaviour kind that declares a required capability.

### Stage 3a-2 — NodeCtx + BehaviorRegistry + `acme.compute.count`

First real behaviour kind end-to-end through the new dispatch seam.

- `NodeCtx` real surface — `emit(port, msg)`, `read_slot(path, slot)`, `update_status(slot, value)`, `schedule(duration, cb)`, `resolve_settings(&msg)`, structured logger.
- `BehaviorRegistry` in `crates/engine` — registers a `NodeBehavior` impl per kind. On `SlotChanged` where the written slot is declared `trigger: true`, the dispatcher invokes `on_message(port, msg)` on the owning node's registered behaviour. Lives beside (not instead of) the live-wire executor; both fire on the same event.
- `Settings<T>` / `ResolvedSettings<T>` with the resolution order from [NODE-AUTHORING.md](../design/NODE-AUTHORING.md) (msg > config > default), including `msg_overrides` lookup.
- `acme.compute.count` — two inputs (`in`, `reset`) + `msg.reset` override, configurable `step`/`min`/`max`/`wrap`, `status.count` slot, registered through `#[derive(NodeKind)]` with `behavior = "custom"`. Lives in a new `/crates/domain-compute` (volume justifies separation from `domain-flows`).
- `requires!` macro in `extensions-sdk` — emits `&[Requirement]` for install-time capability matching.
- End-to-end test — flow with two `demo.point` nodes, a `count` node between them, write to the input produces the counted output on the downstream point.

### Stage 3a-3 — `NodeCtx::schedule` + `acme.logic.trigger`

- One-shot cancellable timer support in `NodeCtx` (tokio-backed, but exposed as an abstract `TimerHandle` so the Wasm/process adapters have a stable surface in 3b/3c).
- `acme.logic.trigger` — Node-RED-style modes (`once`, `extend`, `manual_reset`), configurable trigger/reset payloads, delay timing via `schedule`.
- Deterministic-time test harness so timer-dependent tests don't wall-clock.

### Stage 3a-4 — Wire-shape contract fixtures (Rust side)

TS consumption is deferred to Stage 4; the Rust half ships now so the wire shape is locked before the TS SDK imports it.

- A committed fixture set of `Msg` values serialised by the Rust SDK with their expected JSON, under `crates/spi/tests/fixtures/msg/*.json`.
- CI verifies Rust round-trips each fixture (serialise → parse → compare).
- Stage 4's `@acme/extensions-sdk-ts` must round-trip the same fixtures as its acceptance test. This preserves Stage 0's "no TS before Stage 4" boundary while making Stage 4's TS work a known quantity.

### Stage 3b — Wasm flavor

- **Wasmtime runtime in `crates/extensions-host`** — loads `.wasm` modules, enforces fuel metering + memory caps, exposes host-function allowlist (`emit`, `read_slot`, `update_status`, `log`, `call_extension`, `schedule`)
- **Wasm adapter feature in `extensions-sdk`** — same `NodeBehavior` trait, wasm32-unknown-unknown target, host-function imports bound via the SDK
- **Wasm Provider trait** abstracted so a browser adapter (`web-sys`-backed) can land later without changes to plugin authors' code
- **Example: `acme.wasm.math_expr`** — a math-expression evaluator, taking expression in config + variables in `msg.payload`, returning the evaluated value
- **End-to-end test** — wasm module compiled in CI, loaded at runtime, fuel-exhaustion and OOM traps produce structured `NodeError` instead of killing the agent

### Stage 3c — Process plugin flavor

- **Extension supervisor in `crates/extensions-host`** — spawn, health-check, restart with exponential backoff, cgroup memory limits, UDS socket setup
- **gRPC implementation of `spi/proto/extension.proto`** — `describe` / `discover` / `subscribe` / `invoke` / `health` per the existing contract
- **Process adapter feature in `extensions-sdk`** — `extensions_sdk::run_process_plugin()` one-liner in a plugin's `main.rs`; SDK multiplexes every registered kind behind it
- **Example: `com.example.pg.query`** — Postgres query node with parameterised SQL, timeout, structured error output
  - Backend binary shipped with manifest + capability declarations
  - `must_live_under: [com.example.pg.connection]` — proves the containment rules cross the process boundary
  - **UI (Module Federation bundle) is deferred to Stage 4** when Studio + MF land; the TS SDK's `defineExtension` entry point is already in place so the bundle can be authored without blocking
- **End-to-end test** — agent spawns plugin, executes a query-flow end-to-end, kills the plugin mid-run and asserts the agent survives + restarts it

### Deferred to Stage 4

- The `com.example.pg.query` MF UI bundle (schema-aware table picker + results viewer) — written against `@acme/extensions-sdk-ts` but only wired into the Studio once the Studio shell exists. Serves as Stage 4's "untrusted federated plugin" acceptance case.
- Browser Wasm provider (`web-sys`-backed) — the abstraction is in place in 3b, the implementation lands alongside the Studio build.

**Proves:** one authoring API (the SDK) covers all three execution models; the Msg wire shape is machine-verified to match between Rust and TS; plugin authors write `NodeBehavior` and pick packaging via one Cargo feature; existing Stage 1–2 kinds port to the SDK cleanly. This is the biggest technical unknown — validate it early and hard.

## Stage 4 — Studio shell + Module Federation

**Goal:** Studio loads a UI plugin at runtime.

- Tauri shell renders basic UI
- React Flow canvas with the three node types from Stage 3
- Hardcoded initial graph, "Run" button triggers engine via IPC
- Module Federation wiring in Rsbuild — React declared as required shared singleton
- **Two federated modules built by separate pipelines** (not just co-located): a trusted one loaded into the host realm, an untrusted one loaded into an iframe with postMessage bridge. Both contribute a property panel via `@acme/extensions-sdk-ts`'s `defineExtension` entry (shipped in Stage 3). Proves both the host-realm and iframe isolation paths before we build extensions on them.
- **Wire in the deferred Stage 3c MF UI bundle for `com.example.pg.query`** — schema-aware table picker + results viewer, serves as the "untrusted federated plugin" acceptance case.
- Plain service registry over React Context (no InversifyJS); verify it's visible across the MF boundary in the trusted path, and correctly *not* visible across the iframe boundary
- Schema-driven forms (`@rjsf/core`) reading `node.schema.json` — consumed via the `PropertyPanel` component shipped in Stage 3
- **Browser Wasm provider** — `web-sys`-backed implementation of the Wasm Provider trait introduced in Stage 3b, so Wasm nodes can run in Studio previews

**Proves:** Module Federation loads third-party UI into the host, *and* the iframe isolation path works for untrusted code. The TS SDK authored in Stage 3 is the same one federated plugins use — no hidden Studio-only surface. The other unknown.

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
- [DONE] **Compile-time feature flags already present** in `apps/agent/Cargo.toml` (`role-edge`, `role-cloud`, `role-standalone`). Stage 6a does not add role-gated code paths \u{2014} it establishes the runtime selector; later stages (Postgres driver, MCP server, native protocol extensions) compile in or out behind these features
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
- **Zitadel users mirrored as `acme.auth.user` nodes** in the graph; roles and service accounts likewise — identity becomes part of the unified tree, permission changes fire graph events
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

## Stage 10 — Extension lifecycle

**Goal:** First real extension shipped end-to-end through the full pipeline.

- Extension manifest format finalized, including the **node-kind declarations** each extension contributes (kind IDs, facets, containment schemas, slot schemas) and the **required-capabilities block** (see [VERSIONING.md](../design/VERSIONING.md))
- **Host capability manifest** exposed at `GET /api/v1/capabilities` and `yourapp capabilities` — enumerates `spi.*`, `runtime.*`, `feature.*`, `data.*` with versions
- **Install-time compat match** — the agent refuses to install extensions whose required capabilities aren't provided, with structured errors naming each missing capability. `yourapp ext check <manifest>` is the dry-run command
- **Kind migrations** — kinds ship forward migrations; the graph service walks them on flow load; old-flow fixture suite runs on every release
- Signing pipeline: extensions signed on publish, verified on install
- Extension registry in the Control Plane
- `yourapp ext publish` / `install` / `enable` / `disable` / `check` / `upgrade` commands
- Capability-based permission system: manifest declares permissions and required capabilities; user approves on install
- **Extension installation registers new kinds with the graph crate**; uninstall refuses if instances still exist (per `cascade: deny` semantics) unless forced
- One real protocol extension — pick MQTT as the lowest-risk (pure Rust stack exists). Kinds: `com.example.mqtt`, `com.example.mqtt.topic`
- Hot path: engine fetches extension binary on sync, Studio fetches UI bundle on flow open
- Rollback: old version retained, one-command revert
- **Deprecation plumbing** in place — `deprecated_since` / `removal_planned` honoured by the installer with warnings during the window

**Proves:** the full extension shape works in production. Everything after this is adding more extensions.

## Stage 11 — CLI

**Goal:** The agent binary is also the CLI.

- `clap` command tree: `run`, `flow`, `device`, `ext`, `login`, `config`, `mcp`
- `--output json|yaml|table` on every command
- `--local` vs `--remote` targeting
- Config file at `~/.config/yourapp/config.toml`
- Shell completions generated for bash, zsh, fish, powershell
- `yourapp login` does the OIDC device flow

**Proves:** the CLI story. Also unlocks scripting for the team.

## Stage 12 — BACnet extension (exemplar protocol integration)

**Goal:** Ship a real, non-trivial protocol extension end-to-end. BACnet is a useful exemplar because it's messy — discovery, subscriptions, priority arrays, licensing — and if the extension model handles it cleanly, it handles most other integrations too. This stage is about proving the platform, not about being a BAS product.

- Separate binary using a BACnet library (research options — this is a real dependency decision)
- **Kinds contributed:** `acme.driver.bacnet` (isProtocol, isDriver, isContainer), `acme.driver.bacnet.device` (isDevice, bound under driver), `acme.driver.bacnet.point` (isPoint, isWritable, bound under device). Placement rules enforced by the graph service — points can't be created outside a device.
- Device discovery (BACnet Who-Is / I-Am) populates device nodes under the driver; discovered points populate point children
- Point read/write via BACnet services, exposed as slot read/write on point nodes
- Priority array support for safe-state behavior, mapped to per-slot safe-state policies
- Subscription via change-of-value or polling; COV events become `SlotChanged` graph events
- Extension's UI plugin contributes BACnet device/point pickers, generated from the kind + slot schemas
- **Commissioning mode** honored per RUNTIME.md — writes allowed to points explicitly designated as commissioning points; dry-run/simulation is a separate mode
- Cascading delete verified: deleting a device removes its points and emits `LinkBroken` to any flows wired to those point slots

**Proves:** the extension architecture handles a messy real-world protocol end-to-end through the node model. The "everything is a node" claim survives contact with reality. Other protocol integrations (Modbus, OPC-UA, MQTT bridges) follow the same shape; non-protocol integrations (REST APIs, databases, message queues, SaaS) are simpler versions of the same pattern.

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

- Chaos testing: kill extensions, drop NATS tunnel, fill the disk, starve memory
- Long-running soak on real ARM hardware under realistic load
- Upgrade testing: v1 agent + v1 extension → v2 agent + v1 extension → v2 both
- Schema migration testing across flow format versions
- Penetration test on the public API and the MCP server
- Documentation audit: the docs match what the code actually does

**Proves:** ready for real customers.

## What's not in these stages

Deferred intentionally — all have stable seams so they add cleanly later:

- Mobile (iOS/Android Tauri) — post-v1
- Additional first-party extensions — each is its own mini-stage, same shape as BACnet; examples: Modbus, OPC-UA, MQTT bridges, HTTP/REST, SQL sources, common SaaS (Slack, email, webhooks)
- Domain-specific semantic layers (e.g. BAS ontologies, ITSM schemas, IoT device models) — if/when pursued, expressed as node facets/tags rather than a parallel ontology. Not in v1.
- Advanced fleet features (blue-green deploys, gradual rollouts, automated rollback policies) — v2

## One rule across all stages

**Every stage merges only if the previous stages still work.** No "we'll fix it later." Breakage caught at the seam you just crossed is cheap; breakage discovered three stages later is not.