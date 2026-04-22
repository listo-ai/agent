# Remote Access — Zenoh-Native TCP Tunnels

How operators get SSH, HTTP, and diagnostic access to edge agents behind NAT and firewalls — without a separate binary, without a second outbound connection, without a separate auth system.

This is the **ops access** layer: human-initiated management access to a specific device. It is orthogonal to [FLEET-TRANSPORT.md](FLEET-TRANSPORT.md), which is the application-level messaging fabric (slot events, graph API over fleet subjects, telemetry fan-in). An operator running `ssh edge-42` to debug a stuck BACnet driver is a different problem from Studio listing nodes on a remote agent.

Companion docs:
- [FLEET-TRANSPORT.md](FLEET-TRANSPORT.md) — app-level agent↔agent messaging over Zenoh. Provides the existing session and router infrastructure that tunnels ride on.
- [OVERVIEW.md](OVERVIEW.md) — deployment profiles: edge ARM, edge x86, standalone, cloud.
- [AUTH.md](AUTH.md) — Zitadel identity, per-agent service accounts.
- [DOCKER.md](DOCKER.md) — container images and compose topology.
- [`listod/SCOPE.md`](../../../listod/SCOPE.md) — the supervisor daemon that manages the agent binary lifecycle.

---

## The problem

Edge agents run behind customer firewalls: carrier-grade NAT, corporate proxies, locked-down outbound rules. The fleet transport layer (Zenoh) already handles application messaging — outbound-only, no inbound ports, embedded in the agent binary. But operators also need:

| Need | Why fleet transport does not cover it |
|---|---|
| **SSH into the edge device** | Interactive terminal, arbitrary commands, debugging. Fleet transport is structured req/reply, not a raw byte stream. |
| **HTTP to the agent's local REST API** | Quick `curl` from a laptop, scripting, health checks, `agent <cmd>` CLI against a remote device. |
| **TCP to local services** | SQLite via `litecli`, MQTT broker, BACnet diagnostic tools on the gateway. |
| **File transfer** | `scp` / `rsync` for logs, database snapshots, block bundles. |

The fleet fabric is the right tool for "Studio → cloud → edge → graph API." Zenoh-native TCP tunnels are the right tool for "operator → edge device → raw bytes." Same infrastructure, different key-expression namespace.

---

## Why Zenoh-native (not a separate tool)

The agent already opens an outbound Zenoh session to the cloud router (`:17447` in dev; configurable in production). That session is:

- **Outbound-only** — NAT-friendly, firewall-friendly. The corporate firewall sees one TLS connection going out.
- **Already authenticated** — Zenoh ACLs scoped per tenant and agent-id.
- **Embedded in the agent binary** — zero extra binaries on the edge device.
- **Protocol-agnostic** — Zenoh pub/sub carries arbitrary byte payloads, not just JSON.
- **zenoh-pico compatible** — the same Zenoh router mesh connects constrained MCU devices (ESP32, STM32) via the C `zenoh-pico` client. Those devices can never run a reverse-proxy sidecar; Zenoh tunnels through the gateway can.

Adding raw TCP tunneling means adding one crate (`transport-tunnel-zenoh`) that reuses the existing session. No new binary, no new outbound connection, no new auth system.

---

## Architecture

```
+--- Operator's machine -------------------+    +--- Cloud ----------------+    +--- Edge (behind NAT) ----------+
|                                          |    |                          |    |                                |
|  ssh -p 2222 localhost                   |    |  Zenoh router            |    |  agent process                 |
|        |                                 |    |  (embedded in cloud      |    |  (Zenoh session already open)  |
|        v                                 |    |   agent, port :17447)    |    |                                |
|  zenoh-tunnel client                     |    |                          |    |  TunnelServer (inside agent)   |
|    accept TCP :2222                      |    |         |                |    |    sub <- tunnel.*.up          |
|    pub -> tunnel.sys.edge-42.ssh.X.up    +---->---------+--------------->+---->    pub -> tunnel.*.down        |
|    sub <- tunnel.sys.edge-42.ssh.X.down  <----<---------+---------------<+----+    connect to localhost:22     |
|    pipe bytes <-> TCP socket             |    +-------------------------+    +--------------------------------+
+------------------------------------------+

No new connections. No new binaries on the edge.
The tunnel rides inside the existing fleet Zenoh session.
```

---

## Key-expression namespace

Tunnels live under the `tunnel.` prefix — parallel to `fleet.` on the same session. The Zenoh router forwards both.

```
tunnel.<tenant>.<agent-id>.<service>.<session-id>.up      # operator -> edge (keystrokes, request bytes)
tunnel.<tenant>.<agent-id>.<service>.<session-id>.down    # edge -> operator (output, response bytes)
tunnel.<tenant>.<agent-id>.<service>.ctrl                 # control: open / close / keepalive
```

| Segment | Example | Notes |
|---|---|---|
| `tenant` | `sys` | Same tenant as fleet subjects |
| `agent-id` | `edge-42` | Same agent-id as fleet subjects; dot-escaped per FLEET-TRANSPORT.md rule |
| `service` | `ssh`, `http`, `mqtt` | Named service from the edge agent's tunnel config |
| `session-id` | `a3f9c1d2` | UUID v4, generated per connection by the client. Multiplexes concurrent sessions on the same service. |

Zenoh ACLs on the router scope `tunnel.<tenant>.*` to the same principals that own `fleet.<tenant>.*` — no new auth rules needed.

---

## Components

### `transport-tunnel-zenoh` crate (new)

Lives at `agent/crates/transport-tunnel-zenoh/`. Depends only on `zenoh` (already a workspace dep), `tokio`, and `spi`. ~300–400 lines total.

**`TunnelServer`** — runs inside the edge agent process. Registered at startup if `tunnel.services` is non-empty in config.

```rust
pub struct TunnelServer {
    session:  Arc<zenoh::Session>,
    tenant:   Tenant,
    agent_id: AgentId,
    services: Vec<TunnelService>,   // e.g., { name: "ssh", local_port: 22 }
}

impl TunnelServer {
    /// Subscribes to `tunnel.<tenant>.<agent-id>.*.ctrl`.
    /// On "open" control message: spawn a task that
    ///   1. Connects to localhost:<service.local_port>
    ///   2. Subscribes to tunnel.<...>.<session-id>.up
    ///   3. Publishes  to tunnel.<...>.<session-id>.down
    ///   4. Pipes bytes bidirectionally until EOF or "close" ctrl message.
    pub async fn run(&self) -> Result<(), TunnelError> { /* ~100 lines */ }
}
```

**`TunnelClient`** — used by the `agent tunnel connect` CLI subcommand on the operator's machine. Not part of the edge bundle.

```rust
pub struct TunnelClient {
    session:         Arc<zenoh::Session>,
    target_tenant:   Tenant,
    target_agent_id: AgentId,
    service:         String,        // "ssh"
    local_bind:      SocketAddr,    // 127.0.0.1:2222
}

impl TunnelClient {
    /// Binds a TCP listener on local_bind.
    /// On each accepted connection:
    ///   1. Generate session-id (UUID v4)
    ///   2. Publish "open" to tunnel.<...>.<service>.ctrl
    ///   3. Subscribe to tunnel.<...>.<session-id>.down
    ///   4. Send accepted TCP bytes to tunnel.<...>.<session-id>.up
    ///   5. Pipe .down bytes back to the TCP socket until EOF.
    pub async fn run(&self) -> Result<(), TunnelError> { /* ~100 lines */ }
}
```

### `agent tunnel connect` subcommand (new)

Added to `crates/transport-cli/` alongside the existing `agent` subcommands.

```bash
# Open a local port forwarded to a named service on a remote edge agent
agent tunnel connect \
  --tenant     sys \
  --agent      edge-42 \
  --service    ssh \
  --local-port 2222

# Then from another terminal:
ssh -p 2222 operator@localhost
scp -P 2222 operator@localhost:/var/log/agent.log ./
```

Or wrapped by `repos-cli` convenience commands (proposed):

```bash
repos-cli ssh edge-42            # discovers tenant + router from workspace config, opens tunnel, execs ssh
repos-cli tunnel edge-42 http    # forwards agent HTTP API to localhost:8082
```

---

## Agent configuration

```yaml
# edge.yaml
tunnel:
  enabled: true
  services:
    - name: ssh
      local_port: 22
    - name: http
      local_port: 8082        # agent REST API
    - name: mqtt
      local_port: 1883        # local MQTT broker (if present)
```

`TunnelServer` starts inside the agent at the same time as `FleetTransport`. No separate binary, no separate systemd unit. If `tunnel.enabled` is `false` or the section is absent, no subscribers are registered — zero overhead on sites where ops tunneling is not desired.

---

## Relationship to fleet transport

Both ride the same Zenoh session. Different key-expression namespaces, different purposes:

| | Fleet Transport | Remote Access Tunnel |
|---|---|---|
| **Key prefix** | `fleet.<tenant>.<agent-id>.*` | `tunnel.<tenant>.<agent-id>.*` |
| **Purpose** | App-level: graph API, slot events, telemetry, commands | Ops-level: SSH, raw TCP, HTTP for debugging |
| **Users** | Studio, flows, the agent itself | Human operators, scripts |
| **Payload** | Structured JSON | Raw TCP bytes |
| **Managed by** | `spi::FleetTransport` trait, registered at startup | `TunnelServer`, registered at startup if enabled |
| **Auth** | Zenoh ACL + JWT in message headers | Same Zenoh ACL (same session, same scope) |
| **Graph node** | `sys.agent.fleet` — connection status | Future: `sys.agent.tunnel` — active sessions count |

They share the same outbound connection. The cloud Zenoh router routes both namespaces.

---

## zenoh-pico and constrained devices

[zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico) is a C implementation of the Zenoh protocol for microcontrollers (ESP32, STM32, Zephyr, FreeRTOS, ~50 KB flash). It connects to the same Zenoh router mesh as the full Rust agent.

**Constrained MCUs cannot run SSH or serve TCP.** Ops access to MCU devices goes through their gateway agent:

```
  Operator laptop
      |
      |  agent tunnel connect --agent gateway-1 --service bacnet-diag
      v
  Cloud Zenoh router  ------------------------------------------+
                                                                 |
  Gateway agent (Rust, full Zenoh)                               |
    TunnelServer: localhost:47808 (BACnet diagnostic)  <---------+
    TunnelServer: localhost:22   (SSH)
      |
      |  zenoh-pico session (same router)
      v
  BACnet controller / ESP32 sensor (zenoh-pico)
    pub: telemetry.sys.gateway-1.sensors.*
    sub: cmd.sys.gateway-1.actuators.*
```

The MCU publishes telemetry and receives commands via zenoh-pico. Ops access to the device behind it goes through the gateway's SSH or diagnostic tunnel — same `agent tunnel connect` command. MCUs are never tunnel endpoints.

This is what makes a single Zenoh router mesh cover the full device spectrum — from a 256 MB gateway (full Rust agent) to a 256 KB microcontroller (zenoh-pico) — under one protocol.

| Device class | Zenoh impl | Ops tunnel |
|---|---|---|
| **Edge gateway** (Pi, NUC, industrial PC) | `zenoh` 1.0 (Rust, embedded in agent) | Full — TCP-over-Zenoh for SSH, HTTP, any port |
| **Constrained MCU** (ESP32, STM32) | `zenoh-pico` (C, ~50 KB flash) | Via gateway — gateway `TunnelServer` is the access point |
| **Operator laptop** | `agent tunnel connect` CLI | `TunnelClient` — opens local port, pipes through Zenoh |

---

## Firewall traversal

The tunnel inherits all of Zenoh's transport options:

| Zenoh transport | Firewall profile | When to use |
|---|---|---|
| `tcp/host:17447` | Outbound TCP on a custom port | Development and permissive networks |
| `tls/host:443` | Outbound TLS on port 443 | Default for production edge deployments |
| `quic/host:443` | QUIC/UDP on port 443. Better on lossy links (satellite, cellular) | IoT devices on unstable links |
| `wss/host:443` | WebSocket+TLS on port 443 | Sites with HTTPS proxies that require WebSocket upgrades |

Tunnels ride inside this Zenoh session and automatically benefit from whichever transport the session negotiated. No separate tunnel firewall rules.

---

## Security

### Auth

Tunnel key expressions fall within the same Zenoh ACL scope as fleet subjects:

```
allow publisher:    tunnel.<tenant>.<agent-id>.*   ->  principals with credentials for <tenant>/<agent-id>
allow subscriber:   tunnel.<tenant>.<agent-id>.*   ->  same
```

An operator running `agent tunnel connect` must authenticate to the Zenoh router with a JWT scoped to the correct tenant. The router enforces this before forwarding any bytes. No separate tunnel credentials.

### Service-level auth

The tunnel is a transport. It does not bypass service-level auth:

- **SSH** requires a valid SSH key on the edge device. Password auth disabled.
- **HTTP** (agent REST API) requires a valid Zitadel JWT. Same as direct access.
- **MQTT** requires MQTT credentials. Same as direct access.

### Access control per service

Services absent from `tunnel.services` are never subscribed — zero attack surface for undeclared ports regardless of what an operator requests.

---

## Implementation scope

| Item | Where | Effort |
|---|---|---|
| `TunnelService` config type | `crates/config/src/model.rs` | Small |
| `TunnelServer` | `crates/transport-tunnel-zenoh/src/server.rs` | ~150 lines |
| `TunnelClient` | `crates/transport-tunnel-zenoh/src/client.rs` | ~150 lines |
| `TunnelError` + `lib.rs` | `crates/transport-tunnel-zenoh/src/` | Small |
| Startup wiring | `crates/apps/agent/src/main.rs` | ~10 lines |
| `agent tunnel connect` subcommand | `crates/transport-cli/` | ~80 lines |
| `repos-cli ssh <agent>` wrapper | `repos-cli/cmd/` | ~50 lines Go |
| `sys.agent.tunnel` graph node | `crates/domain-fleet/` | Additive, deferred |

Total: ~450 lines of new Rust. No new dependencies beyond what is already in the workspace (`zenoh`, `tokio`, `uuid`).

---

## Evolution path

| Phase | Remote access | Device scope |
|---|---|---|
| **v1** | `transport-tunnel-zenoh` — SSH, HTTP, any named TCP service | Edge gateways (Rust agent) |
| **v2** | `sys.agent.tunnel` graph node with active-session slots. Studio "Connect" button on remote-agent node. | Same |
| **v3** | zenoh-pico MCU devices on the same Zenoh router mesh. Ops access via gateway tunnel proxy. | Gateways + constrained MCUs (ESP32, STM32, Zephyr) |
| **v4** | Zenoh router mesh with QUIC. Peer-to-peer tunnels when devices share a network. | Full device spectrum, multi-region |

---

## Decisions locked

1. **Remote access is Zenoh-native.** No separate reverse-proxy binary on the edge. Tunnels ride the existing fleet Zenoh session.
2. **Key-expression namespace is `tunnel.<tenant>.<agent-id>.<service>.<session-id>.up/down/ctrl`** — parallel to `fleet.`, same router, same ACLs.
3. **`TunnelServer` lives inside the agent process.** Reuses `Arc<zenoh::Session>`, registered at startup alongside fleet handlers.
4. **Services are opt-in per device.** Absent from `tunnel.services` = key expression never subscribed = zero attack surface.
5. **Tunnel auth is Zenoh ACL + service-level auth.** No separate credential store. Operator JWT scoped to `<tenant>/<agent-id>` is the only credential needed.
6. **zenoh-pico MCU devices have no tunnel endpoint.** Ops access to MCUs goes through the gateway agent's tunnel. One protocol end-to-end.
7. **Firewall traversal is inherited from the Zenoh session transport** (`tcp/`, `tls/`, `quic/`, `wss/`). No tunnel-specific firewall rules.

## One-line summary

**Remote access to edge agents behind NAT is a ~450-line `transport-tunnel-zenoh` crate that pipes raw TCP bytes (SSH, HTTP, any service) through the existing fleet Zenoh session using a `tunnel.<tenant>.<agent-id>.<service>.<session-id>.up/down` key-expression pair — no new binary on the edge, no new outbound connection, no new auth system, and the same infrastructure carries constrained zenoh-pico MCU devices and full Rust gateway agents under one protocol.**
