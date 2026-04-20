# Fleet Transport

How edge agents and the Cloud / Control Plane talk to each other — and how Studio reaches an edge agent through the cloud without the edge having to open an inbound port.

This is the **fleet** layer: cross-agent messaging, request/reply into remote agents, cloud-originated commands, telemetry fan-in. The local REST surface ([BLOCKS.md](BLOCKS.md), [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md), [RUNTIME.md](RUNTIME.md)) is a separate thing — it's HTTP into a single agent process. Fleet transport is agent ↔ agent over a persistent message fabric.

Companion docs:
- [OVERVIEW.md](OVERVIEW.md) — deployment profiles that decide whether fleet transport is on and in which direction.
- [VERSIONING.md](VERSIONING.md) — capabilities map to fleet-transport flavours (`fleet.nats.v1`, `fleet.zenoh.v1`).
- [BLOCKS.md](BLOCKS.md) — blocks use the same capability matcher but *do not* provide the transport.
- [AUTH.md](AUTH.md) — tenant partitioning on subjects / key expressions.

---

## The one rule

**Fleet transport is core, not a block.** It's compiled in, selected by config at startup, exposed through a trait in `spi`. Multiple backends coexist in the codebase gated by Cargo features. The runtime on/off is a config value (`fleet: null` → standalone mode with no cloud), not a block load.

Why: every subsystem that talks to cloud (audit stream, command receiver, telemetry pump, graph event mirror) depends on fleet transport being *present*, not *maybe-loaded*. Making it a block pushes "what if it's not here?" handling into every caller and contradicts [EVERYTHING-AS-NODE.md § "The agent itself is a node too — no parallel state"](EVERYTHING-AS-NODE.md) — the same parallel-system antipattern applied to a cross-cutting concern.

## What fleet transport IS

| Capability | Example |
|---|---|
| **Outbound-only connection** | Edge opens TCP/TLS/QUIC to cloud on 443; no inbound ports on the edge. |
| **Multiplexed request/reply** | Studio (via cloud) → `fleet.<tenant>.<agent-id>.api.v1.nodes.list` → reply with node list. |
| **Pub/sub** | Edge publishes `fleet.<tenant>.<agent-id>.event.graph.slot.temp.changed` → cloud storage + Studio subscribers both receive it. |
| **Durable buffering** | Cloud sends install command while edge offline; message holds until edge reconnects. |
| **Subject-based auth** | NATS accounts / Zenoh ACLs partition per tenant and per agent-id. |
| **Health signal** | `connected`/`reconnecting`/`disconnected` observable locally without synthesising from packet loss. |

## What it IS NOT

- Not the local REST surface (HTTP routes in `transport-rest` — single-agent, same-host).
- Not the block-UI delivery channel (`/blocks/:id/*` ServeDir — local HTTP).
- Not the bulk-transfer layer for large artefacts. Block bundles, firmware, config zips → signed HTTPS URLs published as short control messages over fleet transport, downloaded out-of-band. See § "Bulk transfer" below.
- Not a block API surface. Blocks depend on `fleet.<backend>.v1` as a *capability*, never as something they provide.

## The trait

Lives in [`crates/spi/src/fleet.rs`](../../crates/spi/src/fleet.rs) so any crate can depend on it without pulling in a specific backend. Current signature — object-safe so `AppState` can hold `Arc<dyn FleetTransport>`:

```rust
pub type Payload = Vec<u8>;               // alias today; may grow to bytes::Bytes later

#[async_trait]
pub trait FleetTransport: Send + Sync {
    async fn publish(&self, subject: &Subject, payload: Payload) -> Result<(), FleetError>;

    async fn request(
        &self,
        subject: &Subject,
        payload: Payload,
        timeout: Duration,
    ) -> Result<Payload, FleetError>;

    async fn subscribe(&self, pattern: &Subject) -> Result<SubscriptionStream, FleetError>;

    /// Register a request handler on a subject pattern. Drop the
    /// returned `Server` to deregister.
    async fn serve(
        &self,
        pattern: &Subject,
        handler: Arc<dyn FleetHandler>,
    ) -> Result<Server, FleetError>;

    /// Connection state as a stream. Drives the `sys.agent.fleet` node.
    fn health(&self) -> HealthStream;

    /// Stable backend id — surfaces in capabilities as `fleet.<id>.v1`.
    fn id(&self) -> &'static str;
}

pub struct FleetMessage { pub subject: Subject, pub payload: Payload, pub reply_to: Option<Subject> }
pub type SubscriptionStream = Pin<Box<dyn Stream<Item = FleetMessage> + Send + 'static>>;
pub type HealthStream       = Pin<Box<dyn Stream<Item = HealthStatus>  + Send + 'static>>;
```

The companion [`Subject`](../../crates/spi/src/subject.rs) type is built via `Subject::for_agent(&tenant, agent_id).kind("api.v1.nodes.list").build()` — the canonical dotted form is stored internally, and `subject.render('/')` gives the Zenoh `/`-separated key expression. `FleetHandler` is an object-safe trait with one async `handle(msg) -> Result<Option<Payload>>` method — the same `Arc<dyn FleetHandler>` value feeds both the axum route and the fleet subscription dispatcher.

A zero-config `NullTransport` impl is shipped in `spi` itself — `AppState` holds one by default, every method returns `FleetError::Disabled`, `health()` yields a single `Disabled` status and ends. That's the `fleet: null` deployment shape.

Key points:

1. **`Subject` is not opaque** — it exposes `as_dotted()` + `render(sep)` so backends map to their native separator without re-implementing escape rules.
2. **`serve` is symmetric with HTTP routes.** [`routes::mount`](../../crates/transport-rest/src/routes.rs) registers HTTP handlers; [`fleet::mount`](../../crates/transport-rest/src/fleet.rs) wraps the same core fn (`list_nodes_core`, soon `write_slot_core`, …) in a `FleetHandler` and registers it on fleet subjects. One fn, two surfaces — tested by [`fleet::tests::fleet_list_nodes_returns_same_shape_as_http`](../../crates/transport-rest/src/fleet.rs) asserting the JSON reply is byte-identical.
3. **Health is a stream**, not a poll. The `sys.agent.fleet` graph node has a `status.connection` slot that mirrors it, so flows can react to "cloud dropped" as a first-class event.

## Backend selection

One crate per backend. Each implements `FleetTransport`. Cargo features gate compile-time inclusion, config picks at runtime.

| Crate | Cargo feature | Provides capability | Status | Positioning |
|---|---|---|---|---|
| [`transport-fleet-zenoh`](../../crates/transport-fleet-zenoh/) | `fleet-zenoh` | `fleet.zenoh.v1` | ✅ shipped | **Simple-stack primary.** Pure-Rust library — embeds in-process, no broker sidecar, no separate binary. Right default for developer laptops, standalone appliances, single-tenant clouds, demos. |
| `transport-fleet-nats` | `fleet-nats` | `fleet.nats.v1` | 🔜 planned | **SaaS primary.** JetStream for durable buffering, NATS accounts for tenant isolation, first-class WebSocket for browser Studio, years of ops mileage. Right default for multi-tenant cloud at scale and edges with spotty connectivity that need durable command queues. |
| `transport-fleet-mqtt` | `fleet-mqtt` | `fleet.mqtt.v1` | ⏳ future | When integrating with existing MQTT device fleets where the broker already exists. Weaker req/reply semantics. |

### Which backend when

| Deployment | Backend | Why |
|---|---|---|
| Developer laptop / standalone appliance | Zenoh (embedded) | Single binary, zero ops, `fleet: zenoh` in config just works. |
| Small single-tenant cloud | Zenoh (embedded) | One container. No nats-server to supervise. |
| Multi-tenant SaaS cloud | NATS cluster | Mature accounts-per-tenant, JetStream, ops tooling. |
| Edges with long offline windows | NATS leaf + JetStream | Durable outbound buffering that's proven at scale. |
| Browser-heavy Studio traffic | NATS (WS) | NATS-WS is production-grade; Zenoh's browser story is thinner. |

Both backends implement the same trait — a deployment can start on Zenoh for simplicity and switch to NATS when scale demands it. No code above `spi::FleetTransport` changes.

At least one backend must be compiled in when `role != standalone` without `--offline`. CI matrix builds every two-of-three combinations to prove no backend has leaked a direct dependency above the trait.

### Config shape

Owned by [`crates/config/src/model.rs`](../../crates/config/src/model.rs) (`FleetConfig` + `FleetOverlay`). Tagged on `backend` so every variant reads uniformly:

```yaml
# Embedded Zenoh — shipped today.
fleet:
  backend: zenoh
  listen:    ["tcp/0.0.0.0:7447"]   # leave empty for client-only
  connect:   []                     # peers / routers to dial outbound
  tenant:    sys                   # fleet.<tenant>.<agent-id>.<kind>.<...>
  agent_id:  edge-1                 # defaults to $HOSTNAME then "local"
```

```yaml
# NATS (planned) — same overlay, different backend tag.
fleet:
  backend:  nats
  url:      tls://fleet.yourcloud.com:4443
  tenant:   sys
  agent_id: edge-42
  jetstream: true
```

```yaml
# Standalone — no cloud at all. Absent or explicit `null`.
fleet: null
```

`backend` is validated at startup against the compiled-in features. `fleet: null` (or absence) is accepted regardless of role — an edge that's meant to operate disconnected legitimately needs this, and `AppState` just keeps its default `NullTransport`.

CLI flags mirror the overlay and merge into the layered config stack (`cli > env > file > defaults`): `--fleet-zenoh`, `--fleet-zenoh-listen`, `--fleet-zenoh-connect`, `--fleet-tenant`, `--fleet-agent-id`.

## Subject namespace

One canonical layout. Every backend enforces the same hierarchy so the same subscription shape works cross-backend.

```
fleet.<tenant>.<agent-id>.<kind>.<...>
```

| Kind | Used for | Example |
|---|---|---|
| `api.v1.*` | Request/reply matching the local REST surface. Studio (via cloud) → edge. | `fleet.sys.edge-42.api.v1.nodes.list` |
| `event.graph.*` | Edge → cloud: slot changes, lifecycle transitions, link events. Mirrors the local SSE stream. | `fleet.sys.edge-42.event.graph.slot.temp.changed` |
| `event.agent.*` | Agent lifecycle, health, version. | `fleet.sys.edge-42.event.agent.health.memory_mb` |
| `cmd.*` | Cloud → edge: install block, pause engine, restart, reload config. | `fleet.sys.edge-42.cmd.block.install` |
| `log.*` | Edge → cloud: audit + operational logs. | `fleet.sys.edge-42.log.audit` |

**Wildcard subscriptions** — a Studio view subscribed to `fleet.sys.*.event.agent.health.*` sees every edge's health in the tenant without per-agent config.

**Dot-escape** rule from [BLOCKS.md § "Path-segment encoding"](BLOCKS.md) applies: any graph path segment containing `.` is encoded with `_` when it enters a subject token. `/agent/blocks/com.acme.hello` → subject segment `com_acme_hello`.

## Studio's transport abstraction

`AgentClient` (in [`clients/ts`](../../clients/ts/)) gets two transports, hidden behind the same API surface:

| Transport | When | How |
|---|---|---|
| **Direct HTTP** | Studio → same-machine or LAN agent | `fetch("/api/v1/nodes")` as today |
| **Fleet via cloud** | Studio → remote edge through cloud | `nats.request("fleet.<tenant>.<agent-id>.api.v1.nodes", ..., timeout)` via NATS-WS (or Zenoh's equivalent) |

The edge agent runs `fleet::mount` at startup — same handlers as the HTTP surface, bound to fleet subjects under its agent-id prefix. One internal handler per route, two transports serving it. Studio's fleet picker toggles which transport the client uses; no code above that layer changes.

## Scope selection — picking which agent you're looking at

Studio, the CLI, and flows all need one answer to "which agent is this operation targeting?" The scope is what resolves that answer — it's the (tenant, agent-id) pair used to build fleet subjects, or the sentinel *local* meaning "talk to this process over HTTP and skip fleet entirely." *Status: trait + types planned; no code shipped yet. Tracked alongside the next fleet handler.*

Scope is a **dispatch-time routing concept, not persisted graph state**. It lives in `spi` (core Rust), travels through `AgentClient` (TypeScript), and is carried by flow-node inputs — not UI-only, but never written to the database either.

### The remote-agent node

Remote agents are represented as a node kind in the local graph. Selecting a remote-agent node in Studio is the same action as selecting that scope — there's no parallel "agent picker" dropdown backed by hidden state. Per the "no parallel state" rule, the set of known remote agents is a subtree the user browses, not a list in a config file.

```yaml
kind: sys.fleet.remote-agent
# Deliberately not isContainer: the node has no children in the local graph.
# The "expand to see the remote tree" behaviour is client-side redirection
# (see "How Studio descends into a remote" below), not containment.
facets: [isSystem]
containment:
  must_live_under:
    - { kind: sys.fleet.group }
    - { kind: sys.core.station }   # the station root — see EVERYTHING-AS-NODE.md § core kinds
  may_contain: []
  cardinality_per_parent: ManyPerParent

slots:
  tenant:        { role: config, type: string }
  agent_id:      { role: config, type: string }
  display_name:  { role: config, type: string, nullable: true }
  connection:    { role: status, type: string, enum: [connected, reconnecting, disconnected, unknown] }
  last_seen:     { role: status, type: string, format: date-time, nullable: true }
  version:       { role: status, type: string, nullable: true }
```

```yaml
kind: sys.fleet.group
# Ordinary container — a folder for grouping remote-agents by site/region/tenant.
facets: [isSystem, isContainer]
containment:
  must_live_under: [{ kind: sys.core.folder }, { kind: sys.core.station }]
  may_contain:     [{ kind: sys.fleet.remote-agent }, { kind: sys.fleet.group }]
  cardinality_per_parent: ManyPerParent
slots:
  display_name: { role: config, type: string, nullable: true }
```

Nested `sys.fleet.group` is allowed so operators can build region → site → rack hierarchies without inventing a tagging scheme.

### Scope resolution

Every fleet-capable operation resolves a scope before dispatching:

| Scope | Transport | Subject base |
|---|---|---|
| `local` (default) | Direct HTTP on the same process | n/a — hits the axum router |
| `remote(tenant, agent-id)` | Fleet req/reply via the active backend | `fleet.<tenant>.<agent-id>.<kind>.<...>` |

The `Scope` type lives in [`spi::fleet`](../../crates/spi/src/fleet.rs) next to `Subject`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    Local,
    Remote { tenant: Tenant, agent_id: AgentId },
}

impl Scope {
    pub fn subject(&self, kind: &str) -> Option<Subject> { /* ... */ }
    pub fn is_local(&self) -> bool { matches!(self, Scope::Local) }
}
```

The `#[serde(tag = "kind")]` form crosses the TS boundary as a tagged union so the TypeScript `AgentClient` mirrors it exactly:

```ts
export type Scope =
  | { kind: "local" }
  | { kind: "remote"; tenant: string; agent_id: string };
```

`AgentClient` (Studio) and the CLI both carry a current `Scope`. The client dispatcher picks the transport: `Local` → `fetch`, `Remote` → fleet request. The call site writes the same code either way — `client.nodes.list()` works whether scope is local or remote.

**Broadcast (fan-out over `fleet.<tenant>.*.<kind>.<...>`) is not in v1.** Deferred to the evolution path — `AgentFilter` semantics, partial-reply handling, and timeout aggregation need their own design pass.

### How Studio descends into a remote

1. User selects an `sys.fleet.remote-agent` node in the tree.
2. Studio pushes the scope onto a URL path segment: `/scope/:tenant/:agent_id/...`. The URL is the source of truth; a Zustand store subscribes to the route and exposes the current `Scope` to the `AgentClient`. Back/forward, tab duplication, and deep links all round-trip correctly.
3. Tree-expansion calls now issue `fleet.<tenant>.<agent-id>.api.v1.nodes.list` via fleet req/reply instead of hitting the local HTTP route. The remote agent's own subtree renders inline under the remote-agent node — one unified tree to the user, scoped fleet calls underneath.
4. Navigating to a local path (`/` or any non-`/scope/...` route) resets the scope to `Local`.

Multiple tabs each hold their own scope via their own URL; there is no window-global singleton.

#### Live updates while descended

Studio opens a single fleet subscription on `fleet.<tenant>.<agent-id>.event.graph.>` when the first child of a remote-agent node renders. Messages are demultiplexed client-side by the path prefix carried in each event. The subscription is refcounted by visible subtree; when the user navigates away from the remote and no component still references the scope, the subscription is dropped. This mirrors how the local SSE stream is managed in `AgentClient` today — one connection per scope, not per row — so tree-heavy views don't fan out into N subscriptions.

The cloud's presence tracking (`fleet.<tenant>.*.event.agent.health.*`) drives the remote-agent node's own status slots independently, so the `connection` badge keeps updating whether or not the user has the remote expanded.

### Scope in flows

Flow nodes that touch the graph (`Read Slot`, `Write Slot`, `Watch Lifecycle`) accept an optional `scope` input, defaulting to `Local`. A flow can watch a slot on a remote agent by wiring an `sys.fleet.remote-agent` node into the scope input — the flow engine reads `tenant` + `agent_id` from that node's config slots, constructs `Scope::Remote`, and translates the call into a fleet subscription on `fleet.<tenant>.<agent-id>.event.graph.<path>.slot.<slot>.changed`. From the flow author's perspective, "read a slot on edge-42" and "read a slot on this agent" are the same operation with a different scope.

### Discovery

Remote-agent nodes are seeded two ways:

- **Manual (v1)** — operator creates the node and fills in `tenant` + `agent_id`. Fine for small fleets and for the standalone-studio-paired-with-one-edge case.
- **Cloud-populated (future)** — when Studio is talking to a cloud scope, the cloud publishes `sys.fleet.remote-agent` children under an `sys.fleet.group` for each tenant it knows about. Status slots (`connection`, `last_seen`) are driven by cloud-side presence tracking on `fleet.<tenant>.*.event.agent.health.*`. See the evolution path.

Either way, the node is the single source of truth for "what remotes exist" — there's no separate fleet inventory API.

## Representing the fleet connection as a node

Per the "no parallel state" rule, the live connection is a node kind in the graph, not hidden state in a subsystem. *Manifest drafted; the kind + seeding are not yet registered — added alongside the next fleet handler.*

```yaml
kind: sys.agent.fleet
facets: [isSystem]
containment:
  must_live_under: [{ kind: sys.agent.self }]
  cardinality_per_parent: one_per_parent

slots:
  backend:      { role: status, type: string,   enum: [none, nats, zenoh, mqtt] }
  connection:   { role: status, type: string,   enum: [connected, reconnecting, disconnected, disabled] }
  connected_since: { role: status, type: string, format: date-time }
  last_error:   { role: status, type: string,   nullable: true }
  messages_in:  { role: status, type: integer }
  messages_out: { role: status, type: integer }
  url:          { role: config, type: string,   nullable: true }
```

Consequence: the "send Slack on cloud disconnect" flow is the same shape as every other flow — subscribe to `graph.<tenant>.agent.fleet.slot.connection.changed`, check for `disconnected`, call Slack.

## Bulk transfer

Fleet transport is for **control messages**, not blobs. Block bundles, firmware, config zips:

1. Cloud uploads the artefact to an object store (S3 / MinIO / R2 / image-baked).
2. Cloud signs a time-limited URL.
3. Cloud publishes a control message over fleet: `fleet.<tenant>.<agent-id>.cmd.block.install` with `{id, url, sha256, expires_at}`.
4. Edge downloads via HTTPS directly, verifies SHA256, installs.
5. Edge publishes result: `fleet.<tenant>.<agent-id>.event.block.installed` with `{id, version, status}`.

Why not push bytes through the bus: resumable range requests, parallel fan-out via CDN, operators already know how to debug HTTP + S3, storage cost per GB is much lower than on-bus durable stores. Rule of thumb — if the payload exceeds `smallest_edge_ram / 10`, publish a URL, don't push bytes.

## Auth

Two layers:

| Layer | Enforced by |
|---|---|
| **Connection auth** | Backend's own — NATS accounts / JWT, Zenoh access control, MQTT ACLs. Cloud issues per-edge credentials scoped to `fleet.<tenant>.<agent-id>.*`. |
| **Message-level auth** | OIDC bearer token carried in message headers for Studio-originated requests. Edge validates before dispatching to the handler, same path as REST. |

Credential rotation, revocation, and audit all live in the existing Zitadel + audit-stream plumbing. The fleet backend is the delivery fabric; it does not own identity.

## Failure modes

| Failure | Observable behaviour | Recovery |
|---|---|---|
| Edge loses network | `connection` slot → `reconnecting`; backend's reconnect loop with jitter; outbound events queue to the outbox ([RUNTIME.md § "Outbox"](RUNTIME.md)). | Reconnect resumes; outbox drains; no operator action. |
| Cloud cluster loses quorum (NATS) | `reconnecting` on every edge; JetStream messages buffer on leafs where possible. | Cluster recovery restores normal operation; outbound burst absorbed by JetStream. |
| Backend-level auth rejection | `connection` → `disabled`; `last_error` populated; no reconnect loop. | Rotate credentials; `agent fleet reconnect` CLI forces retry. |
| Message exceeds backend limit | Publisher gets `FleetError::PayloadTooLarge`; error propagates. Caller must chunk or use the URL pattern. | Route through object-store URL (see "Bulk transfer"). |
| Subject namespace drift (edge v1, cloud v2) | Capability matcher at connection time refuses; edge logs "host expects `fleet.v2`, agent provides `v1`". | Staged fleet upgrade with VERSIONING.md-compliant deprecation window. |

## Not in v1

| Not in v1 | Why |
|---|---|
| Peer-to-peer direct edge ↔ edge | NAT traversal complexity for a rare need. Cloud mediates. If we ever need it, libp2p fits the mental model better than another broker. |
| Fleet-contributed capabilities | Blocks can't provide `fleet.*` — same reasoning as "blocks don't provide transports". Capabilities flow core → blocks. |
| Multi-backend per agent | One backend active at a time. Simplification: config chooses; reconfiguring switches. |
| Hot-swap backend at runtime | Rare need; restart is fine. |
| Custom auth providers on the fleet fabric itself | Stick with each backend's native auth (NATS accounts, Zenoh ACLs, MQTT usernames). |
| `Scope::Broadcast` (fan-out across all agents in a tenant) | Partial-reply handling, `AgentFilter` semantics, and timeout aggregation need their own design pass. Revisit once `Remote` is shipped and a real use case lands. |
| Cloud-populated remote-agent discovery | Depends on cloud-side presence tracking and a mirrored `sys.fleet.group` tree — sequenced after the cloud control plane lands. Manual node creation covers v1. |

## What's shipped today

- Trait + types in [`spi::fleet`](../../crates/spi/src/fleet.rs) — `FleetTransport`, `FleetHandler`, `FleetMessage`, `HealthStatus`, `FleetError`, `NullTransport`.
- Subject builder in [`spi::subject`](../../crates/spi/src/subject.rs) — `Subject::for_agent(tenant, agent_id).kind("api.v1.nodes.list").build()`, with dot-escape applied per-token.
- **`FleetScope`** in [`spi::fleet`](../../crates/spi/src/fleet.rs) — `Local | Remote { tenant, agent_id }` with `#[serde(tag = "kind", rename_all = "snake_case")]`. `subject(&self, kind)` builds the canonical fleet subject for remote scopes and returns `None` for local (takes the axum router path). Serde JSON contract locked by tests (`fleet_scope_serde_contract`, `fleet_scope_subject_*`).
- **`FleetScope` TypeScript mirror** in [`clients/ts/src/schemas/fleet.ts`](../../clients/ts/src/schemas/fleet.ts) — `FleetScopeSchema` (zod discriminated union) + `FleetScope` namespace with `local()`, `remote()`, `isLocal()`, `isRemote()` convenience helpers. Exported from the client's public surface.- **`RequestTransport` interface** in [`clients/ts/src/transport/request.ts`](../../clients/ts/src/transport/request.ts) — the five methods (`get`, `post`, `postNoContent`, `put`, `delete`) that every domain module depends on. `HttpClient` implements it directly (no domain code changed). `FleetRequestFn` and `pathToSubject()` live in the same file — `pathToSubject` maps `(method, REST path)` to a fleet subject suffix using an explicit table that mirrors what `transport_rest::fleet::mount` registers on the Rust side.
- **`FleetRequestTransport`** in [`clients/ts/src/transport/fleet_request.ts`](../../clients/ts/src/transport/fleet_request.ts) — implements `RequestTransport` for remote scopes. Each domain call encodes as `(subject, body)` and calls a caller-supplied `fleetRequestFn`; the subject is derived via `pathToSubject`. The caller owns the fleet WS/NATS connection and injects it as a closure — no NATS dependency inside the client package.
- **`AgentClient.connect()` scope switching** in [`clients/ts/src/client.ts`](../../clients/ts/src/client.ts) — `AgentClientOptions` gains `scope?: FleetScope` and `fleetRequestFn?: FleetRequestFn`. When scope is `Remote`, `FleetRequestTransport` is used; otherwise `HttpClient`. `AgentClient.scope` is a readable property. `events` for remote scopes returns a no-op iterator (fleet graph-event subscriptions are separate, managed by the caller on their fleet WS).- **`sys.fleet.remote-agent` and `sys.fleet.group` node kinds** in [`crates/domain-fleet`](../../crates/domain-fleet/) — manifests, `register_kinds()`, tests that assert manifest parsing, facet presence, and containment rules. Registered in `agent run` alongside the other domain crates.
- Embedded Zenoh backend in [`transport-fleet-zenoh`](../../crates/transport-fleet-zenoh/) — covers `publish` / `request` / `subscribe` / `serve` / `health`; `Server` drops via `ServerHandle::shutdown`.
- Handler seam in [`transport_rest::fleet::mount`](../../crates/transport-rest/src/fleet.rs) — currently registers `api.v1.nodes.list` on `fleet.<tenant>.<agent-id>.api.v1.nodes.list`, sharing `list_nodes_core` with the axum route. The `_returns_same_shape_as_http` test locks the contract.
- Config overlay + CLI flags wired through [`agent run`](../../crates/apps/agent/src/main.rs). `fleet: { backend: zenoh, … }` in YAML opens a `ZenohTransport`, swaps it into `AppState`, and calls `fleet::mount`.
- End-to-end integration test [`fleet_zenoh_e2e`](../../crates/transport-rest/tests/fleet_zenoh_e2e.rs) spins up two Zenoh peers on loopback and verifies a real req/reply round-trip.
- [Smoke-test example](../../crates/transport-fleet-zenoh/examples/fleet_get.rs) — one-shot CLI that joins as a third peer and queries any mounted subject.

Verified on [`dev/cloud.yaml`](../../dev/cloud.yaml) + [`dev/edge.yaml`](../../dev/edge.yaml): cloud listens on `tcp/127.0.0.1:17447`, edge connects, each mounts `fleet.sys.<agent-id>.api.v1.*`, and a third peer can query either side and get its node list back.

**Not yet shipped (fleet connection):** the actual fleet WS/NATS connection that Studio uses as its `fleetRequestFn`. The `AgentClient` scope-switching seam is complete; what remains is wiring up a NATS-WS (or Zenoh equivalent) client in Studio and passing it as `fleetRequestFn` when constructing a `Remote`-scoped `AgentClient`. The URL-based scope routing in Studio (`/scope/:tenant/:agent_id/...`) follows once the connection is live.

## Evolution path

| Today | Stage N | Stage N+1 |
|---|---|---|
| `transport-fleet-zenoh` shipped, embedded, one handler mounted | + remaining `api.v1.*` handlers (`nodes.get`, `slots.write`, …) on the same seam | + streaming replies for long-running ops |
| Axum `AuthContext` extractor threaded through first mutating routes | + `AuthContext` threaded into fleet `FleetHandler::handle` via reply-bearing headers | + `StaticTokenProvider` wired from config for real tenant isolation |
| `transport-fleet-nats` planned | + NATS backend alongside Zenoh, same trait | + NATS-WS client for Studio browser, JetStream durable outbox |
| Single-process dev topology | + multi-region cloud, leaf chaining | + federated multi-tenant clouds |
| Object-store URLs for bulk | + content-addressed cache at each edge | + peer caching ("ask my neighbour first") |
| `FleetScope` type + TS mirror shipped; `sys.fleet.remote-agent` / `sys.fleet.group` kinds registered; `RequestTransport` interface + `FleetRequestTransport` + `AgentClient` scope switching shipped | + Studio fleet WS/NATS-WS connection wired as `fleetRequestFn`; Studio `/scope/:tenant/:agent_id/...` URL routing | + cloud-populated discovery under `sys.fleet.group`, then `Scope::Broadcast` with `AgentFilter` |

Each row is additive; the trait signature never changes.

## Decisions locked

1. **Fleet transport is compile-time-plus-config, never a block.**
2. **Single trait (`spi::FleetTransport`)**; multiple backend crates gated by Cargo features.
3. **Canonical subject namespace** is `fleet.<tenant>.<agent-id>.<kind>.<...>`, enforced regardless of backend wire format.
4. **Dot-escape rule from BLOCKS.md applies** to every segment that becomes a subject token.
5. **Connection state is a graph node** (`sys.agent.fleet`) — no parallel status struct.
6. **Bulk transfer doesn't go on the bus.** Control message + signed URL + out-of-band HTTPS fetch.
7. **`fleet: null` is a first-class configuration**, not a degraded mode — standalone agents don't pretend they have cloud.
8. **Blocks consume `fleet.<backend>.v1` capabilities, never provide them.**
9. **Scope is dispatch-time routing, never persisted graph state.** `Scope::Local` vs `Scope::Remote { tenant, agent_id }` chooses the transport; the set of known remotes lives as `sys.fleet.remote-agent` nodes, not as a separate inventory.

## One-line summary

**Fleet transport is a compile-time-selected, runtime-toggleable core subsystem — one trait in `spi`, one crate per backend (Zenoh shipped embedded first, NATS next), a canonical `fleet.<tenant>.<agent-id>.<kind>.<…>` subject namespace, connection state represented as a graph node, bulk artefacts moved via signed HTTPS URLs out of band — giving one wire protocol for cloud-originated commands, cross-agent events, and Studio-to-edge request/reply without edges opening inbound ports or blocks owning load-bearing infrastructure.**
