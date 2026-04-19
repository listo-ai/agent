# Fleet Transport

How edge agents and the Cloud / Control Plane talk to each other — and how Studio reaches an edge agent through the cloud without the edge having to open an inbound port.

This is the **fleet** layer: cross-agent messaging, request/reply into remote agents, cloud-originated commands, telemetry fan-in. The local REST surface ([PLUGINS.md](PLUGINS.md), [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md), [RUNTIME.md](RUNTIME.md)) is a separate thing — it's HTTP into a single agent process. Fleet transport is agent ↔ agent over a persistent message fabric.

Companion docs:
- [OVERVIEW.md](OVERVIEW.md) — deployment profiles that decide whether fleet transport is on and in which direction.
- [VERSIONING.md](VERSIONING.md) — capabilities map to fleet-transport flavours (`fleet.nats.v1`, `fleet.zenoh.v1`).
- [PLUGINS.md](PLUGINS.md) — plugins use the same capability matcher but *do not* provide the transport.
- [AUTH.md](AUTH.md) — tenant partitioning on subjects / key expressions.

---

## The one rule

**Fleet transport is core, not a plugin.** It's compiled in, selected by config at startup, exposed through a trait in `spi`. Multiple backends coexist in the codebase gated by Cargo features. The runtime on/off is a config value (`fleet: null` → standalone mode with no cloud), not a plugin load.

Why: every subsystem that talks to cloud (audit stream, command receiver, telemetry pump, graph event mirror) depends on fleet transport being *present*, not *maybe-loaded*. Making it a plugin pushes "what if it's not here?" handling into every caller and contradicts [EVERYTHING-AS-NODE.md § "The agent itself is a node too — no parallel state"](EVERYTHING-AS-NODE.md) — the same parallel-system antipattern applied to a cross-cutting concern.

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
- Not the plugin-UI delivery channel (`/plugins/:id/*` ServeDir — local HTTP).
- Not the bulk-transfer layer for large artefacts. Plugin bundles, firmware, config zips → signed HTTPS URLs published as short control messages over fleet transport, downloaded out-of-band. See § "Bulk transfer" below.
- Not a plugin API surface. Plugins depend on `fleet.<backend>.v1` as a *capability*, never as something they provide.

## The trait

Lives in [`crates/spi`](../../crates/spi/) so any crate can depend on it without pulling in a specific backend.

```rust
pub trait FleetTransport: Send + Sync {
    /// Publish a one-way message. Delivery semantics per backend's
    /// `fleet.<backend>.v1` contract (NATS core = at-most-once,
    /// JetStream = at-least-once, Zenoh put = at-most-once, etc.).
    async fn publish(&self, subject: &Subject, payload: Bytes) -> Result<(), FleetError>;

    /// Request/reply with a bounded timeout. Subject namespace is
    /// `fleet.<tenant>.<agent-id>.<kind>.<...>`.
    async fn request(
        &self,
        subject: &Subject,
        payload: Bytes,
        timeout: Duration,
    ) -> Result<Bytes, FleetError>;

    /// Subscribe to a subject pattern. Returns a stream of inbound
    /// messages. Wildcards follow the backend's syntax — the `Subject`
    /// type is opaque and backend-parsed.
    async fn subscribe(&self, pattern: &Subject) -> Result<SubscriptionStream, FleetError>;

    /// Register a request handler on a subject pattern. The agent uses
    /// this to answer `fleet.<tenant>.<agent-id>.api.v1.*` requests
    /// from cloud/Studio (one internal fn, two transports — same
    /// handlers serve both HTTP and fleet callers).
    async fn serve<H>(&self, pattern: &Subject, handler: H) -> Result<Server, FleetError>
    where
        H: FleetHandler + Send + Sync + 'static;

    /// Connection state as a stream. Drives the `acme.agent.fleet` node.
    fn health(&self) -> HealthStream;
}

pub struct Subject(/* backend-parsed opaque id */);
pub struct SubscriptionStream(/* pin<box<dyn Stream>> */);
pub type FleetError = /* structured per VERSIONING.md */;
```

Key points:

1. **`Subject` is opaque.** NATS calls it a subject (`fleet.acme.edge-42.api.v1.nodes.list`); Zenoh calls it a key expression (`fleet/acme/edge-42/api/v1/nodes/list`). The Rust type hides the difference; the construction helper is backend-provided.
2. **`serve` is symmetric with HTTP routes.** The existing `routes::mount` in `transport-rest` registers HTTP handlers; a parallel `fleet::mount` registers the same handlers on fleet subjects. One handler fn, two surfaces — guaranteed to stay in sync.
3. **Health is a stream**, not a poll. The `acme.agent.fleet` graph node has a `status.connection` slot that mirrors it, so flows can react to "cloud dropped" as a first-class event.

## Backend selection

One crate per backend. Each implements `FleetTransport`. Cargo features gate compile-time inclusion, config picks at runtime.

| Crate | Cargo feature | Provides capability | Positioning |
|---|---|---|---|
| `transport-fleet-zenoh` | `fleet-zenoh` | `fleet.zenoh.v1` | **Simple-stack primary.** Pure-Rust library — embeds in-process, no broker sidecar, no separate binary. Right default for developer laptops, standalone appliances, single-tenant clouds, demos. |
| `transport-fleet-nats` | `fleet-nats` | `fleet.nats.v1` | **SaaS primary.** JetStream for durable buffering, NATS accounts for tenant isolation, first-class WebSocket for browser Studio, years of ops mileage. Right default for multi-tenant cloud at scale and edges with spotty connectivity that need durable command queues. |
| `transport-fleet-mqtt` | `fleet-mqtt` | `fleet.mqtt.v1` | Future. When integrating with existing MQTT device fleets where the broker already exists. Weaker req/reply semantics. |

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

```yaml
# Standalone / developer laptop / single-tenant cloud — embedded Zenoh
fleet:
  backend: zenoh
  listen: ["tcp/0.0.0.0:7447"]     # leave empty for client-only
  connect: []                       # peer discovery handles the rest

# Cloud agent (SaaS)
fleet:
  backend: nats
  url: tls://fleet.yourcloud.com:4443
  cluster: true        # this agent is a cluster member

# Edge agent (SaaS)
fleet:
  backend: nats        # or: zenoh, mqtt
  url: tls://fleet.yourcloud.com:4443
  agent_id: edge-42
  tenant: acme
  jetstream: true

# Standalone — no cloud at all
fleet: null
```

`backend` is validated at startup against the compiled-in features. `fleet: null` is accepted regardless of role — an edge that's meant to operate disconnected legitimately needs this.

## Subject namespace

One canonical layout. Every backend enforces the same hierarchy so the same subscription shape works cross-backend.

```
fleet.<tenant>.<agent-id>.<kind>.<...>
```

| Kind | Used for | Example |
|---|---|---|
| `api.v1.*` | Request/reply matching the local REST surface. Studio (via cloud) → edge. | `fleet.acme.edge-42.api.v1.nodes.list` |
| `event.graph.*` | Edge → cloud: slot changes, lifecycle transitions, link events. Mirrors the local SSE stream. | `fleet.acme.edge-42.event.graph.slot.temp.changed` |
| `event.agent.*` | Agent lifecycle, health, version. | `fleet.acme.edge-42.event.agent.health.memory_mb` |
| `cmd.*` | Cloud → edge: install plugin, pause engine, restart, reload config. | `fleet.acme.edge-42.cmd.plugin.install` |
| `log.*` | Edge → cloud: audit + operational logs. | `fleet.acme.edge-42.log.audit` |

**Wildcard subscriptions** — a Studio view subscribed to `fleet.acme.*.event.agent.health.*` sees every edge's health in the tenant without per-agent config.

**Dot-escape** rule from [PLUGINS.md § "Path-segment encoding"](PLUGINS.md) applies: any graph path segment containing `.` is encoded with `_` when it enters a subject token. `/agent/plugins/com.acme.hello` → subject segment `com_acme_hello`.

## Studio's transport abstraction

`AgentClient` (in [`clients/ts`](../../clients/ts/)) gets two transports, hidden behind the same API surface:

| Transport | When | How |
|---|---|---|
| **Direct HTTP** | Studio → same-machine or LAN agent | `fetch("/api/v1/nodes")` as today |
| **Fleet via cloud** | Studio → remote edge through cloud | `nats.request("fleet.<tenant>.<agent-id>.api.v1.nodes", ..., timeout)` via NATS-WS (or Zenoh's equivalent) |

The edge agent runs `fleet::mount` at startup — same handlers as the HTTP surface, bound to fleet subjects under its agent-id prefix. One internal handler per route, two transports serving it. Studio's fleet picker toggles which transport the client uses; no code above that layer changes.

## Representing the fleet connection as a node

Per the "no parallel state" rule, the live connection is a node kind in the graph, not hidden state in a subsystem:

```yaml
kind: acme.agent.fleet
facets: [isSystem]
containment:
  must_live_under: [{ kind: acme.agent.self }]
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

Fleet transport is for **control messages**, not blobs. Plugin bundles, firmware, config zips:

1. Cloud uploads the artefact to an object store (S3 / MinIO / R2 / image-baked).
2. Cloud signs a time-limited URL.
3. Cloud publishes a control message over fleet: `fleet.<tenant>.<agent-id>.cmd.plugin.install` with `{id, url, sha256, expires_at}`.
4. Edge downloads via HTTPS directly, verifies SHA256, installs.
5. Edge publishes result: `fleet.<tenant>.<agent-id>.event.plugin.installed` with `{id, version, status}`.

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
| Fleet-contributed capabilities | Plugins can't provide `fleet.*` — same reasoning as "plugins don't provide transports". Capabilities flow core → plugins. |
| Multi-backend per agent | One backend active at a time. Simplification: config chooses; reconfiguring switches. |
| Hot-swap backend at runtime | Rare need; restart is fine. |
| Custom auth providers on the fleet fabric itself | Stick with each backend's native auth (NATS accounts, Zenoh ACLs, MQTT usernames). |

## Evolution path

| Today | Stage N | Stage N+1 |
|---|---|---|
| `transport-fleet-nats` as primary | + `transport-fleet-zenoh` as alternate | + `transport-fleet-mqtt` for IoT bridges |
| `api.v1.*` mirrors the whole REST surface | + streaming replies for long-running ops | + server-push subscriptions (live node watch) |
| Single cloud cluster | + multi-region cloud, leaf chaining | + federated multi-tenant clouds |
| Object-store URLs for bulk | + content-addressed cache at each edge | + peer caching ("ask my neighbour first") |

Each row is additive; the trait signature never changes.

## Decisions locked

1. **Fleet transport is compile-time-plus-config, never a plugin.**
2. **Single trait (`spi::FleetTransport`)**; multiple backend crates gated by Cargo features.
3. **Canonical subject namespace** is `fleet.<tenant>.<agent-id>.<kind>.<...>`, enforced regardless of backend wire format.
4. **Dot-escape rule from PLUGINS.md applies** to every segment that becomes a subject token.
5. **Connection state is a graph node** (`acme.agent.fleet`) — no parallel status struct.
6. **Bulk transfer doesn't go on the bus.** Control message + signed URL + out-of-band HTTPS fetch.
7. **`fleet: null` is a first-class configuration**, not a degraded mode — standalone agents don't pretend they have cloud.
8. **Plugins consume `fleet.<backend>.v1` capabilities, never provide them.**

## One-line summary

**Fleet transport is a compile-time-selected, runtime-toggleable core subsystem — one trait in `spi`, one crate per backend (NATS first, Zenoh next), a canonical `fleet.<tenant>.<agent-id>.<kind>.<…>` subject namespace, connection state represented as a graph node, bulk artefacts moved via signed HTTPS URLs out of band — giving one wire protocol for cloud-originated commands, cross-agent events, and Studio-to-edge request/reply without edges opening inbound ports or plugins owning load-bearing infrastructure.**
