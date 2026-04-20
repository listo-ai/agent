# Runtime Overview — crossflow as the Flow Engine


## The runtime is a long-lived service

This isn't a web server that handles requests and goes idle. This is a **supervisor process** that runs continuously, executes flows reactively, and is expected to stay up for months without intervention. That's a higher reliability bar than most software. The model borrows from industrial controllers (PLCs) because they got 24/7 operation right; that lineage doesn't mean the platform is only for industrial use.

## What "continuous 24/7 operation" actually implies

| Property | What it means for our runtime |
|---|---|
| **Always running** | Starts on boot, restarts on crash, survives power cycles. Never "shut down for the night." |
| **Deterministic timing behavior** | Not hard real-time, but predictable — a flow triggered by a 1-second timer fires at 1-second intervals, not drift |
| **Graceful degradation** | Network drops, cloud outages, block crashes — local flows keep running |
| **Safe state on failure** | If the engine dies mid-flow, external outputs must not be left in half-applied states. What "safe" means is configurable per output — relevant for anything from HVAC actuators to API writes to database updates. |
| **Stoppable, but deliberately** | Operators stop flows (maintenance, bring-up, emergencies) — explicit, logged, authorized |
| **Observable** | An operator must be able to answer "is it running, is it healthy, what is it doing right now" at a glance |
| **Long uptime without degradation** | No memory leaks, no resource exhaustion over weeks/months. Bar is "months between reboots" minimum. |

## Start / stop / pause semantics — three levels

Not all "stops" are the same. This distinction matters and should be in the design from day one:

| Level | What it stops | Who can trigger | When to use |
|---|---|---|---|
| **Individual flow** | One flow pauses; others keep running | Operator with flow-edit role | Editing a flow, commissioning, debugging |
| **Engine enabled/disabled** | All flows pause; agent stays up, still reports health | Site admin | Maintenance windows, firmware updates |
| **Agent stopped** | Entire process exits | System admin / systemd | OS patching, hardware work |

The flow-level pause is what users will use 99% of the time. The engine-level disable is for bigger maintenance. The process-level stop is rare and treated as a real event.

## Safe-state handling — the thing people forget

When a flow stops, something has to happen to its outputs. Three policies, declared per-node or per-flow:

| Policy | Behavior | Example use |
|---|---|---|
| **Hold last value** | Output stays at whatever it was | Most read-only telemetry, idempotent reports |
| **Go to fail-safe** | Output drives to a pre-configured safe value | HVAC actuators, configuration toggles, anything with a known-good default |
| **Release to downstream default** | Output relinquishes to the target system's own default | Protocols that support releasable priorities (e.g. BACnet priority arrays); APIs that accept "unset" or "default" commands |

This is a first-class concept in the node configuration UI. Every writable output declares its safe-state policy. The engine enforces it on stop, on crash, on disconnect. Protocol blocks like BACnet map cleanly onto the "release to downstream default" policy via their native mechanisms; blocks that talk to generic APIs or databases pick whichever policy fits the semantics of the write.

**Where the policy lives — on the node, in the graph.** The safe-state policy is a **config-role slot on the writable output's own node**, not a registry entry the engine owns. The engine is a *reader* of policies, not an *owner*. On shutdown (or crash, or disconnect) the engine walks the graph for nodes with `kind.facets == IsWritable` and a non-null `config.safe_state.policy` slot, and applies each. This keeps the engine out of the "parallel state" antipattern (see [EVERYTHING-AS-NODE.md § "The agent itself is a node too — no parallel state"](EVERYTHING-AS-NODE.md)) — and it means the Studio, audit log, RBAC, and subscription fabric all treat safe-state exactly like any other config slot, with no special case.

## How this changes the runtime design

A few concrete additions to what I've described so far:

**Process supervision.** Systemd unit with `Restart=always`, `RestartSec=5s`, and a watchdog — the engine pings systemd every N seconds; if it stops pinging, systemd kills and restarts it. Same pattern for Docker with a healthcheck. This is standard industrial practice.

**Graceful shutdown protocol.** SIGTERM triggers: (1) stop accepting new work, (2) finish in-flight session messages with a short timeout, (3) drive outputs to their declared safe states, (4) flush the outbox to disk, (5) exit cleanly. SIGKILL only after a grace period — operators should prefer `systemctl stop` which gives SIGTERM first.

**Engine state machine.** The engine itself has states: `Starting → Running → Pausing → Paused → Resuming → Running → Stopping → Stopped`. Observable via API and via a status LED on physical appliances. Every transition is logged.

**Flow lifecycle mirrors the engine.** Each flow has the same states. Operators can pause one flow without touching others. A paused flow doesn't consume CPU but keeps its subscriptions so it resumes cleanly.

**Watchdog-based health.** Every flow reports liveness. If a flow's scheduled tick doesn't fire for 3× its period, it's flagged unhealthy — surfaces as a red indicator in the Studio and a NATS health event.

**Memory hygiene.** At 512 MB running for months, leaks kill you. Rust helps but doesn't prevent logical leaks (growing caches, unbounded channels, forgotten subscriptions). Every long-lived data structure has a size bound. We run a soak test as part of CI — engine under realistic load for 24+ hours, assert memory is flat.

**Two distinct operational modes — do not merge these.**

| Mode | Inputs | Outputs | Purpose |
|---|---|---|---|
| **Simulation / dry-run** | Synthetic or recorded | Suppressed (logged, not written) | Flow development, regression testing, CI. No live equipment involved. |
| **Commissioning** | Live | **Allowed, but only to explicitly designated commissioning points**; non-commissioning writable outputs are blocked | Controlled bring-up of new equipment. Writes are expected — that's the point of commissioning — but scoped and audited. Big visible banner in Studio. |

Conflating the two bit us in earlier iterations: operators assumed "commissioning = no writes" and got surprised, or assumed "commissioning = free-for-all" and damaged equipment. Separate modes, separate UI affordances, separate audit events.

**Audit every start/stop.** Who stopped what, when, why. Regulated industries (pharma, critical infrastructure, finance) require this. Ours uses Zitadel's event log + our own audit stream; the same audit works for any deployment that wants a change record.

## What operators see

The Studio's main "operations" view needs:

- **Engine status** — state, uptime, memory, CPU, last restart reason
- **Flow list** — each flow's state, last execution, error rate, last output
- **Stop/pause buttons** — per flow and global, both require confirmation, both logged
- **Safe-state preview** — "if you stop this flow, here's what will happen to each output"
- **Maintenance mode toggle** — bigger hammer for operators doing work on the systems this agent integrates with

This is not a dashboard for developers. It's a calm, dense, trustworthy operator view.

## What this does NOT mean

A few things worth not overclaiming:

- **Not hard real-time.** We don't compete with sub-10ms control loops. We're the layer above — reacting to events, writing setpoints, orchestrating integrations — not running tight PID loops or deterministic sub-millisecond scheduling. Sub-second reactivity is the design target.
- **Not safety-rated.** If you're building anything where a software failure risks life or equipment (life-safety, interlocks, pressure vessels, SIL-rated control), that logic belongs in hardwired or safety-rated systems. This platform is not SIL-rated, not a substitute for them, and shouldn't be used as one. Applies whether the use case is BAS, industrial, medical, or anything else.
- **Not zero-downtime for engine upgrades.** Upgrading the agent itself requires a restart. Mitigations: fast restart (<5s), safe-state on shutdown, and staggered rollouts across a fleet.

## One-line summary

**The runtime is a long-lived, always-on service — stoppable at flow/engine/process levels with explicit audit, with declared safe-state policies on every output, designed for months of uptime and graceful degradation when things break.**

## What crossflow is

A reactive workflow engine written in Rust, built by **open-rmf** (the Open Source Robotics Foundation's Robotics Middleware Framework group). Apache-2.0. Implemented on top of **Bevy ECS** — the entity-component-system from the Bevy game engine — because ECS gives crossflow guaranteed-safe parallelism, deterministic scheduling, and cheap message routing between many concurrent services.

It's not a game engine in use; Bevy is just the execution substrate. From your code's perspective, crossflow is a library for building graphs of async services that exchange messages.

## Core concepts

| Concept | What it is |
|---|---|
| **Service** | The unit of work. Takes an input message, produces an output message. Can be sync or async, can interact with the outside world, can run for any duration. Implemented as a Bevy system. |
| **Workflow** | A directed graph of services wired together — output of one feeds input of another. Supports parallel branches, joins, races, and cycles. A workflow is itself a service, so workflows compose hierarchically. |
| **Session** | A single execution of a workflow. Each `request()` spawns a new session. Sessions run independently and in parallel. |
| **Request / Outcome** | `commands.request(input, service).outcome()` — non-blocking call that returns a receiver. Can be awaited in async contexts. |
| **Message** | Typed data flowing between services. Serializable via serde. What nodes pass to each other. Shape is Node-RED compatible (`payload`, `topic`, metadata, custom fields) but messages are immutable Rust values on the wire. Settings on a node can be overridden per-message via declared `msg_overrides` (e.g. `msg.url` → HTTP client URL, Node-RED style) — see [NODE-AUTHORING.md](NODE-AUTHORING.md) for the full pattern including a worked HTTP-client example. Underlying envelope spec: [EVERYTHING-AS-NODE.md § "Wires, ports, and messages"](EVERYTHING-AS-NODE.md). |
| **Diagram** | A serializable representation of a workflow. Authored as YAML (hand-edited flow files) and stored/transmitted as JSON (DB, NATS, API) — the loader normalises at the boundary. See [NODE-AUTHORING.md § "File formats"](NODE-AUTHORING.md#file-formats--you-always-author-yaml). This is what the Studio saves and the engine executes. |

## Why it fits our product

| Requirement | How crossflow provides it |
|---|---|
| Long-running reactive runtime | Services stay resident, react to inputs as they arrive |
| Cycles (feedback loops, state machines) | Supported natively — unlike a DAG |
| Parallel branches | First-class; ECS schedules them across cores |
| Hierarchical composition | Workflows are services; compose freely |
| Serializable flow format | Diagrams load from JSON, match what the Studio saves |
| Browser execution | Already compiles to Wasm — live demo exists |
| Async I/O | Built on Bevy's task pools, integrates with tokio |
| Deterministic scheduling | ECS gives us reproducible execution for testing |

## How it maps to our stack

| Our concept | crossflow equivalent |
|---|---|
| Built-in node | A registered service |
| Block node | A service that wraps a gRPC call to an block process, or a Wasm invocation |
| Flow on canvas | A workflow diagram |
| Running a flow | `commands.request()` on the workflow service |
| Flow-to-flow call | One workflow invoked as a service from another |
| Message on a wire | crossflow message between services |
| Flow document | crossflow's JSON diagram format, wrapped in our schema versioning |

## Request lifecycle

1. **Studio saves a flow** → diagram stored as JSON in Postgres/SQLite via SeaORM (authored form is YAML — see [NODE-AUTHORING.md § "File formats"](NODE-AUTHORING.md#file-formats--you-always-author-yaml); storage is always JSON)
2. **Control Plane deploys** → diagram pushed to the target engine over NATS
3. **Engine loads diagram** → crossflow parses JSON, instantiates services, wires the graph
4. **External event arrives** → e.g. BACnet value change, MQTT message, HTTP request, scheduled trigger
5. **`commands.request()` fires** → new session spawned, input message enters the workflow
6. **Services execute** → ECS schedules them in parallel where possible, respecting data dependencies
7. **Messages flow along edges** → output of one service feeds the next
8. **Side effects happen** → block processes called over gRPC, writes to KV, publishes to NATS
9. **Outcome resolves** → final message yielded; live telemetry pushed to Studio subscribers via NATS
10. **Session ends** → resources freed; next request spawns a fresh session

## The engine node — how a node type plugs in

Every node on the canvas is backed by one of these:

| Node backing | Implementation | Isolation |
|---|---|---|
| **Built-in native** | Rust function registered as a crossflow service, statically linked into the agent | Trusted, in-process |
| **Block process** | Rust adapter service that calls the block over gRPC-over-UDS | Crash-isolated, cgroup-limited |
| **Wasm block** | Rust service that invokes a Wasm module via Wasmtime, with fuel metering | Sandboxed, memory-capped |
| **Function node** | QuickJS interpreter executing user-authored JS inline | Inline but script-sandboxed |
| **Subflow** | Another crossflow workflow registered as a service | Same as the parent |

Flow authors never see these differences. From the canvas they're all just nodes.

## Runtime components

| Component | Role |
|---|---|
| **crossflow engine crate** | The graph runtime itself — service registry, scheduler, session manager |
| **Diagram loader** | Parses flow JSON, validates against schema, builds the service graph |
| **Service registry** | Map of node type → service implementation; populated at startup by built-ins and discovered blocks |
| **Block supervisor** | Spawns block processes, monitors health, restarts on crash |
| **Wasm runtime** | Wasmtime instance per Wasm block, with fuel and memory limits |
| **Message bus adapter** | Bridges crossflow messages to external transports — NATS for cross-agent, gRPC for external API |
| **Telemetry pump** | Publishes live session events (message flow, timings, errors) to NATS for Studio live monitoring |
| **Outbox** | Durable queue for cloud-bound messages when the NATS tunnel is down. Bounded by disk quota (default 1 GB) and age (default 7 days); on overflow, oldest-first drop for telemetry subjects, newest-first reject for command-ack subjects. Backpressure surfaces to producers via a NATS health event so flows can shed load rather than enqueue forever. Policy per subject class is declared in config. |

## Parallelism model

crossflow inherits Bevy's parallelism guarantees. Services that don't share data run in parallel on a thread pool sized to the host. Services that share state (via ECS resources or components) are automatically serialized — you can't create data races. On a 512 MB edge device this typically means 2–4 worker threads. In the cloud it scales with the pod's CPU allocation.

A single session is single-threaded unless it has parallel branches in the diagram. The engine runs many sessions concurrently — one per in-flight request.

## Failure handling

| Failure | What happens |
|---|---|
| Service panics | Session cancelled, error emitted as outcome, other sessions unaffected |
| Block process crashes | gRPC call fails, service returns error, supervisor restarts the process with backoff |
| Wasm trap (out of fuel, OOM, illegal op) | Call fails with error outcome, Wasm instance destroyed, next call gets a fresh instance |
| NATS tunnel drops | Local execution continues, cloud-bound messages queue in the outbox |
| Diagram parse error | Flow marked invalid, never instantiated, error surfaced to Studio |
| Schema version mismatch | Migration runs if possible, else flow flagged for user upgrade |

## What we add on top of crossflow

| Addition | Why |
|---|---|
| Schema-versioned diagram wrapper | Forward/backward compatibility for flow documents across engine versions |
| Block supervision | Crossflow doesn't manage child processes; we do |
| Wasmtime integration | Crossflow doesn't ship a Wasm executor; we wire one in |
| NATS bridge | Crossflow has zenoh/gRPC; we add NATS for fleet messaging |
| QuickJS function node | Node-RED-style inline JS, not part of crossflow |
| Live telemetry streaming | Hooks into crossflow's session events, publishes to NATS for Studio |
| Flow validation pipeline | Static checks before deployment — cycles only where allowed, type compatibility on wires |
| Subflow versioning | Flows-as-services get version pinning to prevent runaway updates |

## Development loop

| Step | Command |
|---|---|
| Run engine standalone | `yourapp run --role=standalone --config=dev.yaml` |
| Load a flow from file | `yourapp flow deploy ./my-flow.json --to local` |
| Inspect live messages | `yourapp flow logs <flow-id> --follow` |
| Dry-run a flow | `yourapp flow test <flow-id> --input '{"temp": 72}'` |
| Register a new node type | Drop the block in `/blocks/*`, run `yourapp ext install --local ./blocks/my-node` |
| Hot-reload Wasm | Rebuild `.wasm`, engine picks it up on next session |

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Bevy dependency weight | Cargo feature flags strip unused Bevy subsystems; we compile only `bevy_ecs`, `bevy_tasks`, not the render pipeline |
| crossflow is at 0.0.x | **Vendor it in Stage 0.** Pin a commit, mirror into our monorepo, and treat upstream as a source we merge from — not a published crate we depend on. Validates forkability before we're betting a product on it. |
| Bevy 0.16 target | Upstream tracks one Bevy version at a time; plan for version bumps on a 3–6 month cadence, with a dedicated upgrade checklist |
| Learning curve (ECS model) | Most nodes never see ECS directly — they're just `async fn` services; ECS shows up only for engine-core developers |
| Five node execution models (native / Wasmtime / browser-Wasm / QuickJS / gRPC block) | One unified telemetry schema across all five: every node invocation emits the same `{node_id, session_id, duration, outcome, bytes_in, bytes_out}` record regardless of backing. Tracing spans cross the boundary for gRPC and QuickJS. Debuggability is a product requirement, not an afterthought. |

## One-line summary

**crossflow is a reactive workflow engine over Bevy ECS that gives us hierarchical, cyclic, parallel flows with serializable diagrams — we wrap it with block supervision, Wasm sandboxing, NATS messaging, QuickJS scripting, and versioning to turn the library into the product's runtime.**