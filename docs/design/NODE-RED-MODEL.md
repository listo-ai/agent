# Node-RED Model — One Shape on the Wire

Status: draft
Owners: platform/graph
Depends on: [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md), [SLOT-STORAGE.md](SLOT-STORAGE.md)

## Purpose

Collapse the current dual-surface ("output port + mirror status slot") that first-party kinds expose today into a single Node-RED-native surface: **input ports, output ports, msg on the wire. Nothing else.**

## Problem

Heartbeat (and every other source node) currently exposes its value twice:

- `out` — output port, carries a Node-RED envelope `{ _msgid, _ts, payload: { count, state } }`.
- `current_count` / `current_state` — status slots, bare values.

Every widget author, every history config, every dashboard binding has to pick which one to use. The two are always in sync because the node writes both on every tick. This is redundancy with a learning tax — "is it `slots.out.payload.count` or `slots.current_count`?" — and it violates the one-slot-per-concern premise of [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md).

## Goals

1. **One shape on the wire: Node-RED msg.** A node emits `Msg { payload, topic, _msgid, _ts, … }` on an output port; nothing else.
2. **Widgets subscribe to output ports.** A KPI / chart / table bound to a node reads the *last msg emitted* on the named output port (Node-RED Dashboard pattern).
3. **History attaches to output ports with typed msg-paths.** Authors pick which fields of the msg to record, and declare the storage type per path. Scalar paths land in the time-series table; JSON paths land in `slot_history`.
4. **Internal node state is invisible.** Timer handles, debounce counters, and other bookkeeping live in node context, not as user-facing slots.
5. **Node-RED mental model transfers 1:1.** Users coming from Node-RED see the same inputs/outputs/msg they already know. No platform-specific "status slot" vocabulary to learn.

## Non-goals

- Importing Node-RED flow JSON verbatim. That's a separate "Node-RED compat mode" story.
- Removing the `Msg` envelope or making payload shapes pluggable. Envelope stays as defined in [spi/src/msg.rs](../../crates/spi/src/msg.rs).
- Re-implementing Node-RED's JavaScript Function node semantics. QuickJS Function already exists and is unchanged.
- Changing the storage contract from [SLOT-STORAGE.md](SLOT-STORAGE.md). Only the *declaration surface* moves (per-slot-type → per-path-in-HistoryConfig). Scalar paths still go to time-series tables; JSON paths still go to `slot_history`.

## Naming invariant — slot name = payload key

Where a slot corresponds to a field in the msg payload, **the slot name is the payload key, exactly**. No `current_` prefix, no rename layer, no translation table.

| Msg field | Slot name |
|---|---|
| `msg.payload.count` | `count` |
| `msg.payload.state` | `state` |
| `msg.payload.temp` | `temp` |

PLC mental model: the tag name IS the address. A Studio user sees `state` on the card, writes `payload.state` in a widget `field`, binds history to `payload.state` — one word, one meaning, appears identically everywhere.

This is why the `current_*` mirror slots are deleted (Stage 4) rather than renamed: renaming them still carries two names for one thing.

### Scope of the invariant

The invariant applies to **scalar-per-slot** kinds — a BACnet point, a single-reading sensor, any kind where one slot = one physical reading. There the slot name equals the payload key, no prefix.

For **composite-Msg** kinds (heartbeat, any multi-field source with one output port carrying a structured payload), the **slot name matches the port name** (`out`), and individual fields like `count` / `state` live as payload keys inside that Msg. The Msg envelope itself is the slot value.

Worked example — heartbeat:

| Thing | Name | Why |
|---|---|---|
| Output slot | `out` | Port name = slot name (composite payload) |
| Field inside payload | `count`, `state` | Payload keys; accessed via `field: "payload.count"` |
| History path declared on the slot | `payload.count` | Matches the payload key exactly — no `current_` prefix |

If heartbeat were instead modelled as **two scalar-per-slot** outputs (one number slot, one bool slot), the slots would be named `count` and `state` — not `current_count` / `current_state`. The invariant forbids the prefix in *both* models; it only governs the naming, not the "one slot vs one Msg" choice.

## Key decisions

| Decision | Choice | Why |
|---|---|---|
| What appears on a node card | Input ports (left), output ports (right), config summary, last msg preview on each output | Exactly the Node-RED card. Zero new concepts. |
| Status slots (mirrors of output values, e.g. `current_count`) | **Deleted.** The output-role slot IS the current value. No mirror. | Collapses redundancy without violating Rule A — the remaining slot is still a slot. |
| Internal state (`pending_timer`, debounce counters, etc.) | **Stays as status-role slots, with an `isInternal` facet.** Graph-observable, RBAC'd, subscribable. Studio hides them on the default card; power users and `agent nodes get --include-internal` see them. | Rule A — everything is a node. A private `Mutex<State>` nobody can observe from the graph is exactly what [NEW-SESSION.md](NEW-SESSION.md) Rule A forbids. Internal is a render concern, not a storage concern. |
| Widget ↔ node subscription | Widgets bind to `{ node_id, slot: <port_name> }`. Output ports **are** output-role slots whose value is a `Msg`. Subscription reuses the existing `node.<id>.slot.<name>` subject and slot-change event stream. | Honest framing: an output port persists a value keyed by `(node_id, port_name)` across restart, so it IS a slot. Naming it a "cache" and giving it a parallel subject family would be inventing parallel storage. The existing slots table already does this. |
| History config shape | Attach to an output port. Per-path list with declared type. | Gives users "historize `msg.payload.count` as Number" directly; storage split stays table-level and remains a hard contract. |
| History routing | Declared-type-per-path at config time, not per-slot | Still declarative. Save-time validation only catches errors when the emitting kind has a `msg_schema` covering the path — Function/Wasm blocks don't. Runtime mismatch failure mode spelled out below. |
| Runtime type mismatch on history paths | Non-critical configs: drop + `HistorizerTypeMismatch` health event. Critical configs: `SlotWriteRejected { reason: "type_mismatch" }` at the historizer boundary. | Same tier contract [SLOT-STORAGE.md](SLOT-STORAGE.md) already uses for overflow. No silent corruption, no rows in the wrong table, visible through existing health surface. |

## What the manifest + config look like

Heartbeat:

```yaml
id: sys.logic.heartbeat
ports:
  outputs:
    - name: out
      msg_schema:
        payload:
          type: object
          properties:
            state: { type: boolean }
            count: { type: integer, minimum: 0 }
          required: [state, count]
settings_schema:
  properties:
    interval_ms: { type: integer, default: 1000 }
    start_state: { type: boolean, default: false }
    enabled:     { type: boolean, default: true }
```

No `slots:` block. No `current_*`. The runtime card shows: name, kind, the single `out` port, plus a debug-style preview of the last msg.

HistoryConfig attached to a heartbeat:

```yaml
ports:
  out:
    policy: cov
    paths:
      - path: payload.count
        as: number
        deadband: 0
        max_gap_ms: 60000
      - path: payload.state
        as: boolean
```

Routing:

- `payload.count` → time-series table (Number).
- `payload.state` → time-series table (Bool).
- A JSON path (`as: json`) would route to `slot_history`. Same hard contract as today — just declared per-path instead of per-slot.

## Internal state — status slots with `isInternal` facet

Existing `ctx.read_status` / `ctx.update_status` API stays. No new `ctx.context()`. What changes is **how Studio renders** those slots:

- Manifest declares status slots as usual, adds `facet: isInternal: true` for slots that are bookkeeping not user-facing value.
- `GET /nodes/{path}` excludes internal slots by default; `GET /nodes/{path}?include_internal=true` returns them.
- Studio default card omits internal slots; property panel has an "Advanced / Internal state" section behind a toggle.
- Everything else — history, RBAC, subscriptions, persistence, backup — sees internal slots exactly like any other slot.

This preserves Rule A: every piece of state is graph-observable through the single slot API. "Internal" is a render facet, not a storage or access-control layer.

## Output-port emission contract

For source kinds (0 inputs, 1+ outputs), the manifest declares `outputs.<port>.emit_on_init: true` by default. Authors opt out only when silence is the intended behaviour (e.g. a node that only emits on external trigger).

This closes the cold-start gap: a freshly created node has a defined last-emitted value on its output slot by the end of `on_init`, so widgets mounting before or after node creation both resolve to content within one tick, not "no data yet forever."

## Msg shape change — `_ts` / `_source` / `_parentid` move off the wire

Three fields leave `spi::Msg`:

| Field | Replacement |
|---|---|
| `_ts` | SSE event frame carries `ts` alongside the msg; NATS message header; history writer stamps its own `ts_ms` at storage time. |
| `_source` | Derived from the subject/channel name (`node.<id>.slot.<port>`) + trace span attributes. |
| `_parentid` | W3C TraceContext (`traceparent` / `tracestate`) carried in transport metadata (NATS headers, SSE event frame, gRPC metadata). |

**Prerequisite before stripping**: verify all three transports can carry TraceContext end-to-end — core → Wasm → process-block. NATS headers: native. SSE custom event frame: trivial. UDS gRPC via tonic: gRPC metadata carries `traceparent` natively, but confirm our `block.proto` path actually propagates it. Until this passes, `_parentid` stays on `Msg` as a fallback.

## Widget ↔ output subscription contract

Output ports **are** output-role slots. The binding keeps the existing `slot` field; no new `output` concept on the wire.

- Binding: `{ node_id: <uuid>, slot: "out" }` — unchanged from today.
- Subject: `node.<id>.slot.<port_name>` — unchanged.
- On subscribe, engine returns the current slot value (the last emitted msg) and streams on change — unchanged.

What changes: the **value** stored in an output-role slot is now a Msg object (`{ payload, topic, _msgid }`) instead of a bare scalar. The `current_*` mirror slots are removed (their role collapses into the output slot).

Per-field access in a widget (KPI showing the counter):

```json
{
  "type": "kpi",
  "source": { "node_id": "...", "slot": "out", "subscribe": true },
  "field": "payload.count"
}
```

`field` is a dot-path into the slot value. `payload.count`, `payload.state`, `topic`, `_msgid`, etc. — same syntax as today's `slots.<name>.<path>` in table columns, just applied to a subscribed source.

## Surfaces affected

| Surface | Change |
|---|---|
| KindManifest | Status slots declaring `facet: isInternal: true` for bookkeeping state. Output-role slots' `value_schema` becomes a Msg schema. Mirror status slots (`current_*`) deleted. |
| Behaviour runtime | `ctx.read_status` / `ctx.update_status` unchanged. No new API. Authors decide per-slot whether to mark `isInternal`. |
| Flow engine | Output-role slots hold Msg values. `on_init` emits an initial value on each source-kind output by default. |
| Persistence | No new tables. `slots` table stores Msg-valued output slots. `slot_history` routing driven by per-path `as:` in HistoryConfig. |
| REST | `GET /nodes/{path}` gains `?include_internal=true`; default excludes internal slots. Output slots' values are Msg objects. |
| SDUI / Studio | Widget `source.slot` unchanged. `field` dot-path supports subscribed sources. Property panel splits "Slots" from "Internal state" (hidden by default). |
| CLI | No new flags. `--source-slot <port>` is the existing wire syntax. `agent nodes get --include-internal` flag for bookkeeping slots. |
| Every first-party kind | heartbeat, trigger, timer, math.*, driver points — strip mirror slots, mark bookkeeping as `isInternal`, ensure `emit_on_init`. |
| Tests / fixtures | Assertions on output-slot Msg shape replace assertions on mirror status slots. |
| Docs | CLI-FLOW.md, CLI-DASHBOARD.md, SLOT-STORAGE.md, EVERYTHING-AS-NODE.md — updated terminology. |

## Stages

| Stage | Deliverable | Prerequisites |
|---|---|---|
| 0 | This doc landed + reviewed | — |
| 1 | `facet: isInternal` on slot manifests; `GET /nodes/{path}?include_internal=true`; Studio hides internal slots on default card | 0 |
| 1.5 | **Verify TraceContext propagation end-to-end** (core → Wasm → process-block) across NATS headers, SSE event frame, UDS gRPC metadata. Blocking Stage 2. | 1 |
| 2 | Strip `_ts`, `_source`, `_parentid` from `spi::Msg`. Move `_ts` to SSE frame; tracing spans carry source + parent. | 1.5 |
| 3 | Output-role slots hold Msg values. `field` dot-path in widget bindings. `emit_on_init` default true for source kinds. | 2 |
| 4 | Heartbeat rewrite as proof of concept — mirror status slots gone, `pending_timer` marked `isInternal`, single `out` output slot carrying Msg. | 3 |
| 5 | SDUI / dashboard resolve: subscribed `source.slot` + `field` dot-path rendered correctly by all widget kinds. | 4 |
| 6 | HistoryConfig schema: per-output-slot `paths` list with per-path `as:` type. Save-time validation where kind has `msg_schema`; runtime mismatch handling (drop non-critical, reject critical). | 4 |
| 7 | Rewrite remaining first-party kinds: trigger, timer, math.*, driver points. Mirror slots removed; bookkeeping marked internal; emit-on-init applied. | 5, 6 |
| 8 | Docs sweep: CLI-FLOW, CLI-DASHBOARD, SLOT-STORAGE, EVERYTHING-AS-NODE, SDUI. Fixture regen. | 7 |
| 9 | Soak — 24 h heartbeat + driver + dashboard, verify: no mirror slots in any manifest, `include_internal=false` never reveals bookkeeping, history routing survives a Function node emitting mismatched types without corrupting time-series tables. | 8 |

## Open questions

- **Persistence of output-slot values across restart.** **Decided:** output slots persist like any other slot (existing `slots` table behaviour). For churny kinds (heartbeat, ticker) whose value churns faster than it's useful to snapshot, the slot write can be flagged ephemeral — in-memory only, zeroed on restart — to avoid pointless disk writes. Default: persist, since it reuses the existing slot-write path.
- **Msg vs slot-value in `on_message`.** Input-port behaviour still receives `Msg`. Output emission stays `ctx.emit(port, Msg)`. No change to the behaviour-facing API — only the *storage/render* side moves.
- **Back-compat window.** Hard cutover. Pre-1.0, graphs are recreate-able, maintaining dual surfaces is dead code for a small window.
- **Non-port-emitting "constant" / settings nodes.** A settings node gets a nominal output slot that emits on config change **and on `on_init`**. Widgets subscribe to it normally. Consistent with "everything is msg on a slot."
- **`isInternal` granularity.** Per-slot facet initially. If we discover "kinds where all slots are internal-by-default" becomes common, we can promote it to a kind-level flag later — cheaper to add than remove.

## Migration

**None.** Pre-1.0, dev-only deployments. Operators delete the SQLite file (`dev/cloud.db`, `dev/edge.db`) at each stage boundary and recreate graphs from scratch. No in-place schema upgrade code, no backfill, no dual-read period. The absence of migration is a stage-gating simplification, not a limitation.

## One-line summary

**Node-RED model, period: input ports, output ports, msg on the wire, node context for private state. Status slots disappear; widgets subscribe to output ports; history attaches to output ports with typed msg-paths.**
