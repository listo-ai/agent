# Everything Is a Node

This is the core design commitment of the platform. It changes the shape of the whole system — in good ways. This doc captures both the model and the rules that keep it coherent at scale.

## The unified-graph model

The insight: **everything in the system is a node in a single unified tree.** Devices and data points from an block, user accounts, installed blocks themselves, alarms, schedules, API clients, message queues, database connections, the system's own health metrics — all the same kind of thing, all in one graph.

This pattern shows up in mature integration platforms of all kinds (Tridium Niagara popularised it for building automation; similar shapes appear in Home Assistant, game-engine scene graphs, and object-model desktop environments). We're not copying any one of them; we're adopting the underlying idea because it's right for any platform whose job is to integrate many kinds of things.

Concretely, in one graph:

- An imported device is a node
- A user account is a node
- An installed block is a node
- An alarm condition is a node
- A schedule is a node
- A history log is a node
- The agent's own health metrics are nodes

Because everything is a node, **anything can react to anything**. A flow subscribing to "block X crashed" works the same way as a flow subscribing to "temperature sensor exceeded threshold." A user getting added is a node-level event. An outbound webhook going stale can trigger a notification flow. There's no special-case code for "system events" vs "data events" — they're the same thing.

This is powerful. It's also the single biggest architectural commitment in the platform.

## The core model

**One tree. One event bus. One subscription mechanism. Everything subscribable, everything observable, everything composable in flows.**

| Concept | Our term |
|---|---|
| The tree | **The graph** (colloquially "the station" on a running agent) |
| A thing in the tree | **Node** |
| A property of a node | **Slot** |
| A subscribable data value | **Point** (special kind of slot, usually on a device-like node) |
| A link between slots | **Link** (shown as a "wire" in the UI) |

## What "everything is a node" actually means

Every entity in the system — no exceptions — implements the same base interface:

| Every node has | Purpose |
|---|---|
| A stable ID | Referenceable from anywhere |
| A path in the tree | `station/devices/floor3/ahu-5/zone-temp` |
| A kind / type | `Device`, `User`, `Block`, `AlarmCondition`, `Schedule` |
| Slots (typed properties) | Subscribable, writable or read-only, each with its own type |
| A lifecycle | Created → Active → Disabled → Removed, with events for each transition |
| An event stream | Anything can subscribe to "this node changed" / "this slot's value changed" |
| Tags / metadata | For filtering and discovery (`critical`, `hvac`, `tenant:sys`) |

The node is a trait/interface in Rust. Specific kinds (`DeviceNode`, `UserNode`, `ExtensionNode`) are implementations.

## Concrete implications — the good and the real

### The good (this is why it's powerful)

**Unified observability.** A flow that watches for "any node going into fault state" catches a crashed block, an offline device, a failed scheduled task — all with the same pattern.

**Composability.** "When a user logs in, if it's outside business hours, send a Slack message and log an audit event" is a flow. "When a webhook fires, look up the record in the database, enrich it, publish to a queue" is a flow. "When temperature exceeds setpoint, open the valve" is a flow. Same primitives, same editor.

**Extensibility for free.** A block that adds a new entity type — say, a "Weather Forecast" node — is immediately queryable, subscribable, and usable in flows. No new APIs to design per entity.

**Uniform permissioning.** RBAC is applied to nodes and slots. Want to restrict who can see which users? Same mechanism as restricting who can see which devices.

**Uniform audit.** Every node change flows through the same audit pipeline. User created, block installed, device updated, flow edited — one event stream, one query surface.

**Uniform UI.** The property panel for any node is generated from its slot schema. Built-in nodes and block-contributed nodes render the same way.

### The real (what this costs)

**The tree is the product.** Get the node model wrong and everything downstream is wrong. This is the decision you really can't retrofit.

**Performance at scale.** 10,000 devices × 50 points each = 500,000 nodes. The tree needs to handle that without falling over. Most naive implementations don't.

**Persistence shape.** SQL doesn't love trees. We need a careful schema — probably `nodes(id, parent_id, kind, name)` + `slots(node_id, name, type, value)` with indexes that make subtree queries fast.

**Event fanout.** If every change emits an event and everything can subscribe, you can drown in events. Need to distinguish between *changes worth publishing* vs *internal state updates*.

**Conceptual surface for users.** "Everything is a node" is a big idea. Users coming from Node-RED / Zapier / low-code tools will see "flows" and recognise it; users coming from a unified object-graph tradition will see "the station" and recognise it; users coming from neither need the UI to teach the model gradually without overwhelming them.

## How this changes the architecture

Most of the previous stack stays. The core runtime model shifts.

### What stays the same

- crossflow as the flow engine
- Zitadel for identity
- NATS for messaging backbone
- SeaORM + SQLite/Postgres for persistence
- Three-layer blocks
- Everything in the UI stack
- Docker/deployment story
- CLI, MCP, OpenAPI

### What changes

**There's now a Graph Service at the core of every agent.** It owns the node tree. Every other subsystem interacts with it:

- The engine executes flows; flow nodes read/write slots on graph nodes
- Blocks register their own node kinds; block processes are themselves represented as nodes
- The API exposes CRUD over the graph
- NATS publishes node events; anything can subscribe
- Zitadel identities are mirrored as `UserNode`s in the graph
- Audit is driven by graph events

The **graph is the single source of truth** for "what exists in this system and what is it doing right now."

### Layered view (updated)

```
┌─────────────────────────────────────────────────────────┐
│  Transport — REST, gRPC, NATS, CLI, MCP                 │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  Domain — flows, deployments, auth, business rules      │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  GRAPH SERVICE — the node tree                          │
│  Every entity lives here. Nodes, slots, links, events.  │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│  Data — SeaORM, SQLite/Postgres                         │
└─────────────────────────────────────────────────────────┘
```

## The Node trait (conceptual)

```rust
pub trait Node: Send + Sync {
    fn id(&self) -> NodeId;
    fn kind(&self) -> NodeKind;
    fn path(&self) -> &NodePath;
    fn slots(&self) -> &SlotMap;
    fn lifecycle(&self) -> Lifecycle;
    fn subscribe(&self) -> EventStream;
    fn tags(&self) -> &TagSet;
}

pub enum NodeKind {
    // Core — always present
    Station, Folder, User, Role, Tenant,

    // Identity & access
    ServiceAccount, ApiKey, Session,

    // Data plane
    Device, Point, Schedule, History, Alarm,

    // Flow engine
    Flow, FlowNode,

    // Platform
    Block, ExtensionProcess, Driver,

    // System
    Agent, HealthCheck, Metric,

    // Extensibility — blocks register their own
    Custom(CustomKindId),
}
```

`SlotMap` is a map of named, typed slots. A `Point` is really just a node whose primary slot is its current value. A `User` is a node whose slots include `email`, `enabled`, `last_login`.

## Kind identifiers — reverse-DNS namespacing

Node kinds are dotted identifiers, not an enum. The enum above is only a *category sketch*; the real type system is a string-keyed registry of kinds contributed by first-party modules and blocks.

| Namespace | Owner | Examples |
|---|---|---|
| `sys.*` | Platform (first-party) | `sys.core.folder`, `sys.auth.user`, `sys.compute.math.add` |
| `sys.driver.*` | Platform protocol drivers | `sys.driver.bacnet`, `sys.driver.modbus`, `sys.driver.mqtt` |
| `sys.driver.<proto>.*` | Children of a driver | `sys.driver.bacnet.device`, `sys.driver.bacnet.point` |
| `com.<vendor>.*` | Third-party blocks | `com.acme.weather.forecast` |

The kind ID is a **type**. The node's path in the tree is a **location**. Two different things — a `sys.driver.bacnet.point` kind might appear at thousands of paths.

## Facets — declarative flags on a kind

Each node kind declares a set of boolean facets. These are not the kind itself — they're orthogonal classifications the platform uses for palette grouping, placement rules, permissions, and generic queries.

| Facet | Meaning | Example kinds |
|---|---|---|
| `isProtocol` | Implements a comms protocol | `sys.driver.bacnet`, `sys.driver.modbus` |
| `isDriver` | An I/O driver container | All `sys.driver.*` roots |
| `isDevice` | Represents a physical or virtual device | `sys.driver.bacnet.device`, `sys.driver.modbus.device` |
| `isPoint` | Readable/writable data value | `sys.driver.bacnet.point` |
| `isCompute` | Generic logic/transform node | `sys.compute.math.add`, `sys.compute.logic.and` |
| `isContainer` | Exists only to hold other nodes | `sys.core.folder`, `sys.core.flow` |
| `isSystem` | Platform-managed, not user-created | `sys.agent.self`, `sys.agent.health` |
| `isIdentity` | Identity / auth node | `sys.auth.user`, `sys.auth.role`, `sys.auth.service-account` |
| `isEphemeral` | Not persisted — lives in memory only | `sys.auth.session`, `sys.rpc.in-flight` |
| `isWritable` | Slot values can be written externally | Most device points, most config slots |

Facets are multi-bool — a BACnet driver is `{isProtocol, isDriver, isContainer}`. Uses:

- **UI palette** filters by facet: "show me all `isProtocol` nodes" populates the driver picker.
- **Placement rules** can express "an `isDriver` node must live under `sys.core.folder` or `sys.core.station`."
- **Generic queries** via RSQL: `kind.facets==isDevice` lists every device in the tenant regardless of driver family.
- **Permissions** can be facet-scoped: "grant operator role read access to `isPoint` nodes but not `isIdentity` nodes."

## Containment — the rules that keep the tree sane

Every node kind declares a **containment schema**:

| Field | Meaning |
|---|---|
| `must_live_under` | List of parent kinds (or facet predicates). Empty = free placement, can live anywhere. |
| `may_contain` | List of child kinds (or facet predicates) this node can hold. Empty = leaf node. |
| `cardinality_per_parent` | `ManyPerParent` (default), `OnePerParent` (e.g. one `sys.agent.self` under the station), `ExactlyOne` (required, e.g. station must contain exactly one `sys.auth.realm`) |
| `cascade` | `strict` (default — delete subtree), `deny` (refuse delete if non-empty, for critical system nodes), `orphan` (rare — subtree detached to lost-and-found) |

Example schemas for the user's BACnet hierarchy:

```yaml
kind: sys.driver.bacnet
facets: [isProtocol, isDriver, isContainer]
must_live_under: [sys.core.folder, sys.core.station]   # drivers live at top levels
may_contain:     [sys.driver.bacnet.device, sys.core.folder]
cardinality_per_parent: ManyPerParent   # allow multiple BACnet drivers per folder (e.g. per network segment)

kind: sys.driver.bacnet.device
facets: [isDevice, isContainer]
must_live_under: [sys.driver.bacnet]    # BOUND — only under a BACnet driver
may_contain:     [sys.driver.bacnet.point, { facets: [isCompute] }]   # points AND any compute node
cardinality_per_parent: ManyPerParent

kind: sys.driver.bacnet.point
facets: [isPoint, isWritable]
must_live_under: [sys.driver.bacnet.device]    # BOUND — only under a BACnet device
may_contain:     []                             # leaf
cardinality_per_parent: ManyPerParent

kind: sys.compute.math.add
facets: [isCompute]
must_live_under: []    # FREE — can be dropped anywhere
may_contain:     []
cardinality_per_parent: ManyPerParent
```

## Bound vs free nodes — two placement classes

The containment schema creates two distinct classes of nodes the user sees in the palette:

| Class | Definition | Examples | UX |
|---|---|---|---|
| **Bound** | Non-empty `must_live_under`; the node's existence is meaningful only under a specific parent | BACnet device (under bacnet driver), BACnet point (under bacnet device), Zitadel role (under auth realm) | Palette shows bound nodes contextually — only when the user is inside a valid parent |
| **Free** | Empty `must_live_under`; placeable anywhere | Math/logic compute nodes, timers, function nodes, folders, flows | Always available in the palette |

This resolves the "where can I drop this?" UX question with a declarative rule rather than a hard-coded menu. When the user adds a new block, its kinds show up in the right places automatically because the kinds declare their own placement.

## Placement enforcement — one code path

The graph service validates every mutation against the containment schema. Single check, used everywhere:

| Mutation | Validation |
|---|---|
| `create_child(parent, kind, name)` | `kind ∈ parent.kind.may_contain` AND `parent.kind ∈ kind.must_live_under` (if non-empty) AND cardinality permits |
| `move_node(node, new_parent)` | Same checks against `new_parent` |
| `delete_node(node)` | Cascade policy applied; see below |
| Bulk import / sync from block | Same checks, same code |

A single enforcement path means CRUD, CLI bulk operations, block-driven sync, and flow-authored mutations all respect the same rules. No bypasses.

## Cascading delete

When a node is deleted:

1. **Subtree delete.** All descendants deleted depth-first. Each emits `NodeRemoved`. Transactional — either the whole subtree goes or nothing does.
2. **Link breakage.** Every link whose source or target slot lives inside the deleted subtree is removed. The *other* end of each broken link emits a `LinkBroken` event. Nothing silently disconnects.
3. **External flow references.** Flows that wired to a slot in the deleted subtree don't disappear — their reference becomes a **broken-link stub**. The containing flow transitions to a `Degraded` lifecycle so the UI can surface it. Flow authors decide whether to repair or accept the broken wire.
4. **`cascade: deny` kinds refuse.** The delete fails with a structured error listing non-empty children. Operator must drain first.
5. **Audit.** Every `NodeRemoved` and `LinkBroken` lands in the audit stream with the originating user, parent operation, and reason.

No orphan children, no dangling links, no silent data loss.

## Flow documents are nodes too

A `Flow` is a node kind (`sys.core.flow`, facets `{isContainer}`). Its children are the flow's internal nodes — all subject to the same Node trait, same placement rules, same events. Wires between internal slots are Links, the same as wires anywhere else.

This unifies two models that are usually separate:

| Authoring style | What it means in our model | When to use |
|---|---|---|
| **Live wiring in the tree** (unified-graph style) | Drop a `sys.compute.math.add` directly into a folder, wire its inputs to other node slots, wire its output somewhere. No flow document. | Simple reactive logic: three nodes and two wires. Good for "value X should follow value Y" style behaviour. |
| **Flow document container** (document-based, Node-RED style) | Create a `Flow` node, add compute nodes as its children, wire them inside, wire the flow's own input/output slots to slots elsewhere in the tree. | Complex logic, reusable subflows, versioned deployment units, things you want to pause/restart as one unit. |

One node model, two idioms. Users pick based on complexity — not forced to use one or the other. The flow engine executes the Flow container by discovering its internal topology; live wires are executed directly by the graph's reactive layer.

## Slot roles — config, input, output, status

Slots aren't all the same thing. Every slot has a **role** that tells the system how to treat it:

| Role | Purpose | Persisted? | Who writes it | Example |
|---|---|---|---|---|
| `config` | User-authored configuration | Yes (DB) | Authorized users (API / Studio); never flows | Modbus baud rate, device IP, poll interval, safe-state policy |
| `input` | Values flowing into the node at runtime | No (live) | Upstream links, flow engine | Compute node's operands |
| `output` | Values flowing out of the node at runtime | No (live) | The node itself | Compute result, device point's current reading |
| `status` | Engine/driver-computed state | No (live) | The platform | Node health, lifecycle, last-error, last-seen timestamp |

**Settings = the node's `config`-role slots.** Not a separate concept — same primitive, tagged with the role. The same API shape serves "write a value" (flow commanding a device) and "change a setting" (operator updating config); RBAC and audit distinguish them based on slot role.

`config` slots are **versioned** in the event log — config changes produce an audit trail with before/after values. `output` slot changes are telemetry and land in the TSDB, not the audit log. Slot role is what routes the event.

## Wires, ports, and messages

### Ports and fan-in

A node kind declares whatever input and output slots it needs — those are its **ports** on the canvas. Examples:

- `sys.compute.math.add` has inputs `operand_a`, `operand_b`, output `result` (2 in, 1 out)
- `sys.logic.switch` has inputs `signal`, `selector`, outputs `out_0`, `out_1`, `out_2` (2 in, many out)
- `sys.io.webhook` has output `received` (0 in, 1 out — it's a source)

**Many wires can connect to the same input port (fan-in).** Default semantics are **per-message interleaved** — each arrival fires the target node independently, like Node-RED. Synchronised "wait for all sources" or "combine-latest" semantics are explicit via dedicated kinds (`sys.compute.join`, `sys.compute.combine-latest`), not hidden on every input — the right policy depends on intent.

### Trigger policies on multi-input nodes

Declared per kind:

| Policy | Semantics |
|---|---|
| `on_any` (default) | Fire on any input arrival. Other inputs read the current value held on their slot. Good for reactive compute — "recompute whenever anything changes." |
| `on_all` | Fire only after all required inputs have received at least one message since the last fire. Good for join-style gating. |
| `on_specific: [input_name]` | Only one designated input triggers; others are latched values. Good for trigger/value patterns (e.g. "fire when `trigger` arrives, using whatever is currently latched on `value`"). |

### What travels on a wire — the message envelope

Slots are **typed**. The type determines what flows through a wire connecting them.

- **Primitive-typed slots** carry their value directly: a `number` slot's wire carries a number, a `device-reading` slot's wire carries that reading. Zero ceremony. This is the live-wiring mode — the output changes, the input sees the new value, done.
- **Message-typed slots** carry a structured envelope. This is the flow-oriented mode and is **Node-RED compatible by design**:

```rust
struct Msg {
    payload:   JsonValue,                    // primary data — same as Node-RED's msg.payload
    topic:     Option<String>,               // routing/grouping — same as Node-RED's msg.topic
    id:        MessageId,                    // always present (no underscore-prefix hack)
    parent_id: Option<MessageId>,            // provenance across fan-out/fan-in
    metadata:  BTreeMap<String, JsonValue>,  // user-block fields — scoped, not arbitrary root keys
    timestamp: Timestamp,
    source:    NodePath,                     // which node emitted this message
}
```

Flow authors see `msg.payload`, `msg.topic`, and their own custom fields — the same mental model as Node-RED.

### Immutable under the hood, mutable at the JS boundary

Messages are **immutable Rust values** on the wire. A node that "modifies" a message produces a new one. This closes the classic Node-RED fan-out mutation bug, where two downstream branches unknowingly mutate a shared `msg` object and see each other's changes.

The QuickJS **Function node** exposes `msg` as a **mutable JS object** so Node-RED-style authoring works unchanged:

```js
// Identical to Node-RED — this is deliberate
msg.payload = msg.payload * 2;
msg.topic = "doubled";
msg.custom_field = { foo: "bar" };
return msg;
```

The runtime snapshots the JS object on exit and produces a new immutable `Msg`. From the author's perspective: Node-RED. From the engine's perspective: safe immutable values with clean provenance.

### Porting from Node-RED

What survives unchanged:

- `msg.payload`, `msg.topic`, and user-added custom fields in Function nodes
- The JS API for function-authored logic
- The concept of messages flowing through wires

What differs:

- Nodes registered through the block system are Rust/Wasm/process-bound with **declared typed slots** — richer than Node-RED's "any msg in, any msg out." Typed ports give you palette-time validation, better UI affordances, and runtime type checks.
- The block model is more structured (manifests, JSON Schema settings, lifecycle) — more upfront ceremony, more payoff in reliability.
- Node-RED flows can be imported where they use compatible node patterns; anything leaning on mutable-shared-msg semantics needs a small rewrite.

Function nodes remain the untyped-JS escape hatch for incremental porting. Nothing forces a user to adopt the typed-slot model for logic they've already written in Node-RED JS.

## Slot API

All slot operations go through one REST shape. Paths are node paths in the graph.

```
GET    /api/v1/nodes/{path}                               # node metadata + kind + facets + child list
GET    /api/v1/nodes/{path}/slots                         # all slots with schema, role, current value
GET    /api/v1/nodes/{path}/slots/{slot}                  # one slot: value + metadata + last-updated
PATCH  /api/v1/nodes/{path}/slots/{slot}                  # write a writable slot: { "value": ... }
PATCH  /api/v1/nodes/{path}/config                        # bulk config update, body matches the node's settings schema
GET    /api/v1/nodes/{path}/settings-schema               # default (or single) settings schema
GET    /api/v1/nodes/{path}/settings-schema/list          # all schema variants when the kind supports multiple
GET    /api/v1/nodes/{path}/settings-schema/{variant}     # a specific variant by name
```

**Listing nodes** uses the generic RSQL path (see [QUERY-LANG.md](QUERY-LANG.md)): `GET /api/v1/nodes?filter=kind.facets==isDevice;tenant_id==sys&sort=path`. Subtree queries via path prefix: `?filter=path=prefix=/sys/devices/floor3`.

**Writes are RBAC-checked against the slot's role** — an operator might be allowed to write `config` on devices they own but not on protocol-level nodes; a flow might be allowed to write `output` but never `config`. The check is uniform because slot role is declared on the kind's schema.

**Live updates.** Any slot's value stream is available over NATS (WebSocket to Studio, direct for servers) at `graph.<tenant>.<path>.slot.<slot>.changed`. No polling API needed — if you want a value, you subscribe.

**gRPC, CLI, MCP** all expose the same operations through the generic framework. CLI: `yourapp node get <path>`, `yourapp node set <path> <slot> <value>`, `yourapp node config <path> --variant tcp --set host=10.0.0.5`. MCP: `get_node`, `list_slots`, `set_slot`, `update_config`. One shape, four surfaces.

## Settings schemas — single and multi-variant

Every node kind declares a **settings schema** describing its `config`-role slots. The schema is JSON Schema — same format blocks already use for `node.schema.json`. Studio renders forms from it via `@rjsf/core`; the backend validates submitted config against it; OpenAPI picks it up automatically.

### Single schema — the simple case

Most kinds declare one flat schema:

```yaml
kind: sys.compute.math.add
settings_schema:
  type: object
  properties:
    operand_count: { type: integer, minimum: 2, maximum: 10, default: 2 }
  required: [operand_count]
```

### Multi-variant schemas — the Modbus network case

Some kinds have meaningfully different configurations depending on a top-level choice. Modbus is the canonical example: a network node is either **serial (RTU)** or **TCP**. The fields are different, the validation is different, and showing both at once is confusing. Two mechanisms cover this — each right for different cases.

#### Mechanism 1 — named schema variants (recommended for big choices)

The kind declares multiple named schemas and a default. At node-creation time the user picks a variant; the chosen variant drives the form and validates the config. The variant name is stored on the node so the runtime knows which code path to execute.

```yaml
kind: sys.driver.modbus.network
facets: [isProtocol, isDriver, isContainer]
must_live_under: [sys.core.folder, sys.core.station]

settings_schemas:
  supports_multiple: true
  default: tcp
  variants:
    - name: serial
      display_name: "Serial (RTU)"
      description: "Modbus RTU over RS-485 / RS-232"
      schema:
        type: object
        properties:
          port:        { type: string,  title: "Serial port", default: "/dev/ttyUSB0" }
          baud_rate:   { type: integer, title: "Baud rate",   enum: [9600, 19200, 38400, 57600, 115200], default: 9600 }
          parity:      { type: string,  title: "Parity",      enum: ["none", "even", "odd"], default: "none" }
          stop_bits:   { type: integer, title: "Stop bits",   enum: [1, 2], default: 1 }
          byte_timeout_ms: { type: integer, minimum: 10, default: 100 }
        required: [port, baud_rate]

    - name: tcp
      display_name: "Modbus TCP"
      description: "Modbus TCP over IP"
      schema:
        type: object
        properties:
          host:        { type: string,  title: "Host or IP", format: "hostname" }
          tcp_port:    { type: integer, title: "TCP port",   minimum: 1, maximum: 65535, default: 502 }
          timeout_ms:  { type: integer, minimum: 100, default: 1000 }
        required: [host, tcp_port]
```

**Studio flow:**

1. User drops a `sys.driver.modbus.network` into the tree.
2. Studio calls `GET /settings-schema/list`, sees `supports_multiple: true`, shows the selection dialog:
   ```
   ┌─────────────────────────────────────┐
   │ Modbus network type?                │
   │                                     │
   │ ○ Serial (RTU) — RS-485/RS-232      │
   │ ● Modbus TCP — over IP              │
   │                                     │
   │           [Cancel]  [Continue]      │
   └─────────────────────────────────────┘
   ```
3. User picks TCP → Studio fetches `GET /settings-schema/tcp` → renders only the TCP fields.
4. Submitted config is validated against the TCP variant; the node is stored with `settings_variant: "tcp"`.

The variant name is queryable: `?filter=settings_variant==serial` lists every Modbus-serial network in the tenant.

#### Mechanism 2 — conditional fields within a single schema

For smaller conditional choices (one or two fields toggled by another), use JSON Schema's native `if/then/else` — no variant machinery needed:

```json
{
  "type": "object",
  "properties": {
    "use_tls": { "type": "boolean", "default": false }
  },
  "if":   { "properties": { "use_tls": { "const": true } } },
  "then": { "properties": { "ca_cert_path": { "type": "string" } }, "required": ["ca_cert_path"] }
}
```

`@rjsf/core` renders this inline with dynamic visibility. No selection dialog — fields appear and disappear as other fields change.

#### When to use which

| Use | Why |
|---|---|
| Named variants | The choice changes **what kind of thing the node is** operationally — different code path, different validation, different dashboard. Modbus serial vs TCP. BACnet IP vs MSTP. Transport flavours of an integration. |
| Conditional fields | The choice is a **refinement within one thing**. SSL on/off. Auth method in an HTTP request. One or two fields that only matter in some combinations. |

Rule of thumb: more than ~3 fields differ → named variants. Fewer than that → conditional fields. Both are valid; picking badly just makes the form awkward.

### Validation — three layers, one schema

Submitted config is validated in three places, all using the same schema:

1. **Studio (`@rjsf/core`)** — before submit; friendly errors surfaced to the user.
2. **REST/gRPC handler** — on the wire; rejects malformed payloads with structured errors.
3. **Graph service** — before persisting; enforces required fields and type constraints, and runs kind-specific **custom validators** registered by the domain crate (e.g. "a BACnet device's `instance_number` must be unique under its parent network"). Custom validators are Rust predicates over the parsed config — things JSON Schema can't express.

If any layer rejects, the node is not persisted and no events fire. Partial writes are not a thing.

## The event model

Every node emits events. Events are first-class messages on NATS.

| Event | Fired when |
|---|---|
| `NodeCreated` | Node added to the tree |
| `NodeRemoved` | Node deleted |
| `NodeRenamed` | Path changes |
| `SlotChanged` | A slot's value changes |
| `LifecycleTransition` | Active → Disabled, Disabled → Fault, etc. |
| `LinkAdded` / `LinkRemoved` | Wires connecting nodes change |
| `TagAdded` / `TagRemoved` | Tag set changes |

Subject taxonomy:

```
graph.<tenant>.<path>.created
graph.<tenant>.<path>.removed
graph.<tenant>.<path>.slot.<slot>.changed
graph.<tenant>.<path>.lifecycle.<from>.<to>
```

Wildcards let flows subscribe to:

- All events on a subtree: `graph.sys.devices.>`
- A specific slot across all devices: `graph.sys.devices.*.slot.health.changed`
- All lifecycle transitions fleet-wide: `graph.sys.>.lifecycle.>`

This is the substrate that makes "send an email when a block crashes" work as a flow.

## The "everything triggers a flow" examples

With this model, real-world scenarios become trivial flows:

| Scenario | As a flow |
|---|---|
| Email when an block crashes | Subscribe to `graph.*.blocks.*.lifecycle.>`; filter for `→ Fault`; call email node |
| Alert when a user is granted admin | Subscribe to `graph.*.users.*.slot.roles.changed`; filter for `admin` added; notify security |
| Auto-restart a failed integration | Subscribe to block lifecycle; on fault, call `block.restart` |
| Escalate unacknowledged alarms | Subscribe to alarm nodes; check acknowledgment state; escalate after N minutes |
| Log every config change | Subscribe to `graph.*.>.slot.*.changed` where slot is in a configured allowlist |
| Shutdown on memory pressure | Subscribe to `graph.agent.health.slot.memory.changed`; if > threshold, stop noncritical flows |
| Post to Slack on every new user signup | Subscribe to user-node creation; call Slack block |
| Sync a database table to an external API on change | Subscribe to row-node slot changes; call HTTP block |

None of these require new APIs. They're all flows using the same primitives.

## Flow nodes interact with graph nodes

A flow node type like "Read Slot" takes a path/slot and subscribes. "Write Slot" commits a value. "Watch Lifecycle" emits when a node transitions. The flow engine becomes the *consumer* of graph events — it doesn't duplicate them.

This is the bit that makes the whole thing click. Flows are programs that operate on the graph. The graph is the world. Blocks add new parts of the world. It's turtles all the way up.

## Persistence

The graph needs to land in the database cleanly. Proposed schema:

| Table | Contains |
|---|---|
| `nodes` | `id, parent_id, tenant_id, kind, name, path, created_at, lifecycle` |
| `slots` | `id, node_id, name, type, value (jsonb / text), updated_at` |
| `links` | `id, source_slot, target_slot, link_kind` |
| `tags` | `node_id, key, value` |
| `node_events` | Append-only event log for audit and replay |

With indexes on `parent_id`, `(tenant_id, path)`, `(node_id, name)`. Subtree queries use the materialized path (`path LIKE '/sys/devices/%'`) — or `ltree` on Postgres for real graph queries.

Materialized-path approach keeps it SQLite-portable. SQL-heavy graph traversals stay simple and fast.

## Where "everything is a node" might be wrong

Be honest about the edge cases:

**High-frequency telemetry.** A point getting updated 10 times per second shouldn't emit 10 NodeChanged events — that's firehose territory. Solution: slots have a *change-detection policy* (deadband, throttle, coalesce) and only emit events on meaningful changes. Raw values still flow; only events are throttled.

**Transient operational data.** Not everything belongs in the persisted tree. The current sessions of logged-in users are node-like but not something you want in SQL forever. Solution: the tree has both *persisted* and *ephemeral* sections. Ephemeral nodes live only in memory and their events fire, but they don't hit SQL.

**Multi-tenancy.** The tree is per-tenant. `graph.<tenant>.<path>` is the real address. Cross-tenant anything is forbidden at the graph layer, not bolted on.

**Block-owned nodes.** An block's nodes must behave correctly if the block crashes. Solution: when an block goes into Fault state, its nodes transition to a `Stale` lifecycle — readable but marked untrusted — and flows subscribed to those nodes see the transition immediately.

## The agent itself is a node too — no parallel state

The rule applies to the platform's own subsystems, not just user-visible entities. If the engine, the block supervisor, the health monitor, or any other agent subsystem owns runtime state *outside* the graph, it has become a parallel system — and every flow that wants to react to it has to learn a second API. That's the failure mode the whole model exists to prevent.

Concretely, the agent contributes these kinds on boot:

| Kind | Purpose | Slots (non-exhaustive) |
|---|---|---|
| `sys.agent.self` | One per running agent — root of agent-owned subtree. `isSystem`, `isContainer`, `cardinality_per_parent: ExactlyOne` under its station. | `agent_id`, `version`, `role`, `boot_ts` (all `status`) |
| `sys.agent.engine` | The flow engine's state. Under `sys.agent.self`. | `state` (`status`, string: `Starting`/`Running`/`Paused`/`Stopping`/`Stopped`), `last_transition_ts`, `flows_running`, `flows_paused` |
| `sys.agent.health` | Process + host metrics. | `memory_mb`, `cpu_pct`, `fd_count`, `disk_free_mb` — all `status`, throttled per the high-frequency-telemetry rule above |
| `sys.agent.supervisor` | Block-process supervisor state, one child per supervised block. | `extension_id`, `state`, `pid`, `restart_count` |

**The engine does not own its state in a private struct.** The engine owns **execution** (the async worker, the scheduler, the block supervisor, the safe-state walker). State representation lives in the graph:

- `Engine::transition(new)` writes to the `sys.agent.engine.state` slot via `GraphStore::write_slot`. The `SlotChanged` event *is* the notification.
- Private `EngineState` fields, where they exist in the code, are derived reads from the graph, not a parallel cache.
- Safe-state policies are **config-role slots on the writable point's own node**, not entries in an engine-local registry. The engine walks the graph at shutdown (`kind.facets == IsWritable && config.safe_state.policy != null`) to find what to apply.

Why it matters:

- Flows can subscribe to `graph.<tenant>.agent.engine.slot.state.changed` with the same machinery they use for device points. The "shut down non-critical flows on memory pressure" example in this doc works because agent health is a node.
- The Studio renders engine status using the same generic property panel it uses for everything else.
- RBAC on engine state is the same mechanism as RBAC on devices. No special case.
- The audit log captures engine lifecycle transitions through the same stream as any other slot write.

**Litmus test when adding a subsystem.** If you introduce a new long-running component (health monitor, rate limiter, block loader, metrics collector), ask: *where does its state live?* If the answer is a struct with a `Mutex<...>` that nobody outside the subsystem can observe, you're building a parallel system. Promote the state to a kind with status-role slots and make the subsystem an execution-only concern over graph state.

## What this means for the coding stages

Stage 1 — the "engine skeleton" — needs to be **the graph service**, not just crossflow integration. Everything after depends on it.

Updated stage order:

| Stage | What |
|---|---|
| 0 | Foundations (contracts, repo, CI) |
| **1** | **Graph service** — node trait, tree structure, event bus, slot system, basic CRUD, persistence |
| **2** | Engine on top of the graph — crossflow flows read/write slots |
| 3 | The three node flavors (native/Wasm/block) become node kinds |
| 4 | Persistence — formal schema, migrations |
| 5 | Deployment profiles |
| 6 | Messaging — NATS as the transport for graph events |
| 7 | Auth — users are nodes, roles are nodes |
| 8 | Public API over the graph |
| 9 | Block lifecycle — blocks are nodes |
| 10 | CLI, MCP, everything else |

## Engineering rule

> **Everything is a node. This is the core design commitment. Any new entity type in the system — whether a user, a device, a schedule, an alarm, a block, a health check, a metric, the agent's own engine state, or something we haven't thought of yet — is a node in the graph. It has an ID, a path, typed slots, a lifecycle, an event stream, a facet set, and a containment schema. Do not add entities outside this model. The rule applies to the platform's own internals too: if a subsystem owns state in a private struct that nobody outside can observe, you've built a parallel system. If you find yourself tempted to create a top-level concept that isn't a node, stop and ask why.**

This rule is reflected in [CODE-LAYOUT.md](CODE-LAYOUT.md): the `graph` crate is the core, and every domain crate is in effect a node-kind registration with associated business rules.

## One-line summary

**Everything is a node in one unified tree — users, devices, blocks, alarms, flows, health metrics — with reverse-DNS kind IDs, facet flags for cross-cutting classification, declarative containment rules distinguishing bound nodes (like a BACnet point, which only exists under its device) from free nodes (like a math-add that can live anywhere), cascading delete with explicit link-breakage semantics, and flow documents as just another kind of container node — giving uniform events, permissions, audit, UI, and extensibility across the whole platform.**