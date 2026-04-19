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

- `/crates/engine/` — crossflow wrapped in our runtime crate
- `acme.core.flow` kind registered — flows are container nodes in the graph
- Two flow-node types: "Read Slot" (takes a path + slot, subscribes) and "Write Slot" (commits a value)
- Live-wire executor: reactive slot-to-slot links outside of flow documents (Niagara-style "simple case — no flow wrapper")
- Diagram loader for Flow-container nodes
- Engine state machine: `Starting → Running → Stopping → Stopped` (per [RUNTIME.md](../design/RUNTIME.md))
- Graceful shutdown on SIGTERM with output safe-state stubs
- End-to-end test: a flow subscribes to a demo-point slot, transforms the value, writes it back — all via the graph

**Proves:** flows and graph are unified. The engine is a consumer of graph events, not a parallel system.

## Stage 3 — The three node flavors

**Goal:** Validate that native, Wasm, and extension-process nodes all execute in the same flow.

- Built-in native node: "Add" — statically linked Rust
- Wasmtime integration: "Multiply" as a `.wasm` file loaded at runtime
  - Fuel metering, memory caps, host functions (`get_input`, `set_output`, `log`)
  - Wasm Provider trait abstracted so browser can swap in `web-sys` later
- Extension process: "Log" as a separate binary
  - gRPC server over Unix domain socket
  - Engine supervisor spawns it, monitors health, restarts on crash
  - cgroup memory limits applied
- End-to-end test: `[Number: 21] → [Multiply ×2] → [Log]` prints `42`

**Proves:** the three-layer plugin model works. This is the biggest technical unknown — validate it early.

## Stage 4 — Studio shell + Module Federation

**Goal:** Studio loads a UI plugin at runtime.

- Tauri shell renders basic UI
- React Flow canvas with the three node types from Stage 3
- Hardcoded initial graph, "Run" button triggers engine via IPC
- Module Federation wiring in Rsbuild — React declared as required shared singleton
- **Two federated modules built by separate pipelines** (not just co-located): a trusted one loaded into the host realm, an untrusted one loaded into an iframe with postMessage bridge. Both contribute a property panel. Proves both the host-realm and iframe isolation paths before we build extensions on them.
- Plain service registry over React Context (no InversifyJS); verify it's visible across the MF boundary in the trusted path, and correctly *not* visible across the iframe boundary
- Schema-driven forms (`@rjsf/core`) reading `node.schema.json`

**Proves:** Module Federation loads third-party UI into the host, *and* the iframe isolation path works for untrusted code. The other unknown.

## Stage 5 — Persistence

**Goal:** The graph and its flows survive restart. Same repo traits, two native-shaped backends.

- Graph persistence: the `nodes`, `slots`, `links`, `tags`, `node_events` tables (see EVERYTHING-AS-NODE.md) land behind the repo trait; ephemeral nodes stay in memory.
- Repository trait implementations: **SQLite-native** (TEXT/INTEGER, single-writer) and **Postgres-native** (UUID, TIMESTAMPTZ, JSONB, partial/GIN indexes, tenant RLS, `ltree` for subtree queries).
- Separate migration sets per backend — physical types, indexes, and partitioning diverge. Logical shape stays consistent.
- Shared repository test suite runs against both; backend-specific tests cover what only matters on one side (e.g. Postgres RLS, SQLite WAL).
- YAML config loader with connection string swap
- Graph CRUD from the Studio writes to the DB
- **Separate telemetry store seam** stubbed: a `TelemetryRepo` trait with a SQLite-rolling-table impl for edge and a placeholder for the cloud TSDB (filled in Stage 7). Telemetry never lands in the OLTP tables.

**Proves:** one set of repo traits, two backends that each use their native strengths. No LCD tax on Postgres. The graph is durable.

## Stage 6 — Deployment profiles

**Goal:** Single binary, `--role` selects behavior.

- `--role=standalone` — everything in one process (dev mode)
- `--role=edge` — engine + local DB + extension supervision
- `--role=cloud` — API + fleet orchestration + Postgres
- Config precedence: flags > env > file > defaults
- Feature flags in Cargo to gate native-only code out of browser builds
- Cross-compile the edge binary to `aarch64-unknown-linux-gnu` and measure memory on real hardware

**Proves:** the "one binary, three roles" story holds. Catches memory surprises on ARM while fixes are cheap.

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