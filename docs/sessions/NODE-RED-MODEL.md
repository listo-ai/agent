# Node-RED Model — Session Notes

Working notes toward full Node-RED-native behaviour. The authoritative design is [../design/NODE-RED-MODEL.md](../design/NODE-RED-MODEL.md) — this file captures *how* we got to the current decisions, *why*, and what's outstanding.

Session owner: platform/graph.
Opened: 2026-04-20.

---

## Problem (as stated by the user)

> "It needs to be like Node-RED. It's that simple."
> "100% the same message output."
> "What you see is what you get."

Today the heartbeat node — and every first-party source kind — publishes the same value twice in two shapes:

- `out` output port: a Node-RED envelope `{ _msgid, _ts, _source, _parentid, payload: { state, count } }`.
- `current_state` / `current_count` status slots: bare values, mirrors of the same data.

Authors picking a binding (widget, history, flow wire) have to choose which shape, and the two are always in sync because the behaviour writes both. That is redundancy with a learning tax.

The msg shape also diverges from Node-RED: we add `_ts`, `_source`, `_parentid` that Node-RED does not have. New users expecting Node-RED semantics see extra fields; imported flows don't round-trip byte-for-byte.

---

## Decisions landed this session

### D1 — Mirror status slots deleted; internal state stays as slots with `isInternal` facet

Nodes still have slots (Rule A: everything is a node, all state is observable through the graph). What changes is the **render surface**.

**What moves**:

- `current_state`, `current_count` (mirrors of output values) → **deleted**. The output-role slot IS the current value; no duplicate.
- `pending_timer` and other bookkeeping → **kept as status slots with `facet: isInternal: true`**. Graph-observable, RBAC'd, subscribable, in `GET /nodes/{path}?include_internal=true`. Studio default card and `agent nodes get` omit them.

**Why** (revised after review): a "hidden per-node KV" was a parallel-state design that violates [NEW-SESSION.md](../design/NEW-SESSION.md) Rule A. Internal is a **render facet**, not a storage layer. This keeps Rule A intact while still collapsing the user-facing mirror redundancy.

### D2 — Msg shape is literally Node-RED

The wire msg is:

```json
{
  "payload": <any>,
  "topic": "optional",
  "_msgid": "uuid",
  "<customField>": <any>
}
```

**Dropped from the msg**: `_ts`, `_source`, `_parentid`.

**Why**: WYSIWYG. What you see in a Function node's `msg`, what the wire carries, what the debug panel renders, what the Rust behaviour code reads — all one shape. Node-RED flows round-trip.

**Where the dropped fields go** (they don't vanish, they move to the right layer):

| Field | Old home | New home |
|---|---|---|
| `_ts` | `msg._ts` | SSE event frame `ts`; NATS header; trace span. History writer already stamps its own `ts_ms` at storage time. |
| `_source` | `msg._source` | Subject/channel name (`node.<id>.output.<port>`); trace span attribute. |
| `_parentid` | `msg._parentid` | OpenTelemetry span parent. Not on the wire. |

### D3 — Widgets subscribe to output-role slots (unchanged binding surface)

Output ports **are** output-role slots whose value is a Msg. No new binding API, no new subject family.

Binding shape (unchanged syntax):

```json
{
  "source": { "node_id": "...", "slot": "out", "subscribe": true },
  "field": "payload.count"
}
```

- `slot: "out"` — the existing slot binding; `"out"` is the output port name = slot name.
- `field` is a dot-path into the slot value (the last emitted Msg).
- Subject `node.<id>.slot.<port>` — unchanged from today.

**Why** (revised): framing this as a "last-msg cache" was dishonest — a keyed `(node_id, port) → last_value` store persisted across restart IS a slot. The existing slots table and change-event stream already do the work.

### D4 — History attaches to output ports with typed msg-paths

HistoryConfig shape:

```yaml
ports:
  out:
    policy: cov
    paths:
      - path: payload.count
        as: number          # → time-series table
        deadband: 0
        max_gap_ms: 60000
      - path: payload.state
        as: boolean         # → time-series table
      - path: payload.raw
        as: json            # → slot_history
```

Storage split from [SLOT-STORAGE.md](../design/SLOT-STORAGE.md) stays a hard contract — it just moves from "declared slot type" to "declared per-path type at history-config time." Scalar paths still route to time-series tables; JSON paths still route to `slot_history`.

**Runtime type mismatch** (Function/Wasm blocks whose kind has no `msg_schema`):

| Tier | Mismatch handling |
|---|---|
| Non-critical | Drop record + emit `HistorizerTypeMismatch` health event with `{ node_id, port, path, declared_as, actual_kind }`. |
| Critical | `SlotWriteRejected { reason: "type_mismatch" }` at the historizer boundary; propagates to flow engine. |

Same tier contract [SLOT-STORAGE.md](../design/SLOT-STORAGE.md) already uses for overflow.

This is what the user originally asked for ("let me pick what to store from the msg"). It's coherent once the mirror-status redundancy is gone.

### D5 — Output-slot values persist like any other slot

Output-role slots live in the `slots` table. On restart, widgets see the last-emitted Msg. No separate cache.

Churny kinds (heartbeat, ticker) can flag an output slot **ephemeral** (in-memory only, zeroed on restart) to skip pointless disk writes — reuses the same flag any other non-critical slot uses. Default: persist.

### D6 — Source vs transformer classification

Kinds split into two classes, not a hybrid:

- **Source nodes** (heartbeat, timer, driver points): 0 input ports, 1+ output ports, internal state in `ctx.context()`.
- **Transformer nodes** (trigger, math.*, switch, function): input ports + output ports, no internal state beyond what `ctx.context()` covers.

No node ever mixes "I emit a msg" with "I also expose a status slot."

### D7 — Hard cutover, no transition window

Pre-1.0, graphs are recreate-able, maintaining dual surfaces is a lot of dead code. The `slots` manifest block and `ctx.read_status` / `ctx.update_status` APIs get removed in Stage 8, not feature-flagged.

### D8 — Constant/settings-only nodes expose values via a nominal output port

A settings node gets an `out` port that emits:

1. **On `on_init`** — initial value available to any widget regardless of mount order.
2. **On config change** — subsequent updates stream to subscribers.

Widgets subscribe normally. No special-cased "subscribe to a node's config" API. The `emit_on_init` contract (D9 below) is what closes the cold-start gap.

### D9 — `emit_on_init` contract for source kinds

Manifest flag `outputs.<port>.emit_on_init: true` (default `true` for source kinds, 0 inputs). Authors opt out only when silence is intentional (input-triggered emitters).

Guarantees: by the end of `on_init`, every source-kind output slot has a defined last-emitted value. Widgets mounting before or after node creation both resolve to content within one tick.

### D10a — Slot name equals payload key (WYSIWYG naming)

Where a slot corresponds to a msg payload field, the slot name IS the payload key. No `current_` prefix. No translation table.

| Msg field | Slot name |
|---|---|
| `msg.payload.count` | `count` |
| `msg.payload.state` | `state` |

**Scope**: applies to scalar-per-slot kinds (driver points, single-reading sensors — one slot = one physical reading).

For composite-Msg kinds like heartbeat, the output slot matches the **port name** (`out`) and `count`/`state` are payload keys inside the Msg, accessed via `field: "payload.count"`. Scalar slots named `current_*` are forbidden either way.

PLC mental model: tag name = address. One name, appears identically in the card, the widget `field`, the history path.

### D10b — No migration

Pre-1.0, dev-only. Operators wipe `dev/cloud.db` / `dev/edge.db` at each stage boundary and recreate. No backfill, no dual-read, no upgrade code — on purpose, to keep each stage small.

### D11 — TraceContext verification gates Stage 2

Before `_parentid` / `_source` strip from `Msg`, verify W3C TraceContext propagation end-to-end: core → Wasm → process-block. Transports:

- NATS headers: native support.
- SSE event frame: custom `trace` field.
- UDS gRPC (`block.proto` via tonic): gRPC metadata carries `traceparent` natively, needs confirmation on our specific path.

Stage 1.5 in the design doc. Blocking Stage 2.

---

## Execution plan

Single source of truth for stage numbering lives in [../design/NODE-RED-MODEL.md § Stages](../design/NODE-RED-MODEL.md). This checklist references those stages; don't renumber here.

- [ ] **Stage 1** — `isInternal` facet on slot manifests; `GET /nodes/{path}?include_internal=true`; Studio hides internal slots by default.
- [ ] **Stage 1.5** — Verify TraceContext propagation end-to-end across NATS, SSE, UDS gRPC. Blocks Stage 2.
- [ ] **Stage 2** — Strip `_ts`, `_source`, `_parentid` from `spi::Msg`. Move `_ts` to SSE frame.
- [ ] **Stage 3** — Output-role slots hold Msg values. Widget `field` dot-path. `emit_on_init` default true for source kinds.
- [ ] **Stage 4** — Heartbeat proof of concept: mirror slots gone, `pending_timer` marked `isInternal`, single `out` output slot carrying Msg.
- [ ] **Stage 5** — SDUI widget bindings render subscribed `source.slot` + `field` across kpi/chart/table/sparkline/etc.
- [ ] **Stage 6** — HistoryConfig schema: per-output-slot `paths` with per-path `as:` type; save-time validation; runtime-mismatch tier handling.
- [ ] **Stage 7** — Rewrite remaining first-party kinds (trigger, timer, math.*, driver points).
- [ ] **Stage 8** — Docs sweep + fixture regen.
- [ ] **Stage 9** — 24 h soak.

---

## Open questions still on the table

- **QuickJS Function node**: `msg` is the wire msg. Current impl already strips platform fields on entry — verify it still does after Stage 2.
- **Trace span propagation across process boundaries** (core → Wasm → process-block): `_parentid` used to ride the wire. After D2, parent IDs travel as a W3C TraceContext in the transport layer (NATS header / UDS metadata / SSE event frame). Confirm all three transports can carry it — probably yes for NATS and SSE, needs a check for the UDS gRPC path.
- **History back-compat on existing graphs**: old `slot_history` rows are keyed by `(node_id, slot_name)`. After D4, new rows are keyed by `(node_id, port_name, path)`. Migration: one-shot rewrite at Stage 8 boot, or accept a read-boundary at upgrade time. Lean: rewrite, since volumes are small in dev.
- **Studio property panel**: the "slots" tab on a node disappears. What replaces it — just "ports + config + attached configs" as bullet points, or a richer panel?

---

## What the user will see after all stages ship

Heartbeat card:

```
┌──────────────────────────────┐
│ Heartbeat       sys.logic.heartbeat │
│                                     │
│                         OUT  ●──── │
│  last: payload.count=354           │
│        payload.state=true          │
│                                     │
│  /flow-1/heartbeat    0 in · 1 out │
└──────────────────────────────┘
```

No `current_*`. No `pending_timer`. Last-msg preview shows the two meaningful fields. Same card Node-RED would show.

Debug panel on a wire:

```json
{
  "payload": { "count": 354, "state": true },
  "_msgid": "d76f0eca-7589-4738-b89a-ba512269fb88"
}
```

That's it. Three fields total. Pure Node-RED.
