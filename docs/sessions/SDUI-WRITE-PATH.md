# SDUI Write Path — Scope (for review)

Two-way binding for single-value SDUI controls. A `toggle` or `slider` reads its value from a slot, writes back on change, and stays live via SSE — with **zero** handler registration, **zero** per-kind backend code, and **no** `/ui/action` detour for what is really just a slot write.

First concrete deliverables: a working `toggle` and `slider` bound to any boolean / numeric slot on any node. Long-term, this is the shape every "edit a value" control follows across the IR.

Authoritative references:

- [SDUI.md](../design/SDUI.md) — IR vocabulary, resolver shape, subscription plan
- [DASHBOARD.md](../design/DASHBOARD.md) — binding grammar (`$target.*`, `$self.*`, `/` child walk), `expected_generation` on `POST /api/v1/slots`
- [NEW-API.md](../design/NEW-API.md) — five-touchpoint client parity rule
- [NODE-RED-MODEL.md](NODE-RED-MODEL.md) — slot / Msg envelope

---

## The gap

Today every mutation has to go through a registered action handler (`HandlerRegistry` + `POST /api/v1/ui/action`). For a settings-form submit that's fine — it's multi-field, validate-then-commit, maybe transactional. For a single switch or slider it's ceremony: an action that does nothing but forward one value into one slot. And every new kind that wants a toggle has to ship a handler.

We already have a generic write endpoint (`POST /api/v1/slots`, OCC-guarded, ACL-checked, audited). We already emit a subscription plan that echoes changes via SSE. The missing piece is an IR-level **binding** concept that wires read + write + subscribe from one declaration, and a renderer that honours it.

## Non-goals

- **Replacing `form`.** Forms keep their explicit `submit: Action` — multi-field commit semantics are a different shape.
- **Replacing `/ui/action`.** It stays for domain ops (BACnet scan, PR approve, multi-node transactions). Pure slot writes just don't go through it.
- **New transport.** No new endpoints. REST writes + SSE reads, both already live.
- **Client-side business logic.** The renderer debounces and calls the generic slot endpoint. It does not validate, coerce, or branch on slot type.
- **Offline / queued writes.** Online-only, same as SDUI v1.
- **Bulk / multi-slot atomic write from a single control.** One bound control, one slot, one write.
- **Row-level dynamic write bindings.** A `table` row rendering a per-row `toggle` is *not* in scope here — those controls are instantiated client-side from row data and don't exist in the static resolved tree. That requires a `bind_template` concept (per-row parameterised binding) which is a separate design. v1 covers static, author-placed controls only; the `component_id` uniqueness contract for WritePlan keys rests on that.

## Principle

> A control declares which slot it edits. The server is the source of truth. The write goes out via REST, comes back in via SSE — identical path for every client, including the one that wrote.

No local state shortcut. The optimistic patch (S7) is a **display** hint; the SSE echo is authoritative.

---

## Design

### 1. `BindingSpec` in `ui-ir`

New type in `crates/ui-ir`:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BindingSpec {
    /// Sugar: `"$target.enabled"` ⇒ { slot: …, concurrency: Lww }
    Short(String),
    Full {
        slot: String,
        #[serde(default)]
        concurrency: Concurrency,
        #[serde(default)]
        debounce_ms: Option<u32>,
    },
}

pub enum Concurrency { Lww, Occ }  // default: Lww
```

Binding expression grammar is unchanged — same `$target.*` / `$stack.*` / `$self.*` / `/` child-walk rules as read bindings (DASHBOARD.md § Bindings). Must resolve to a concrete node slot at resolve time; resolving to a literal or computed value is a save-time error.

### 2. Two new leaf variants

```json
{ "type": "toggle", "id": "t1", "bind": "$target.enabled", "label": "Enabled" }

{ "type": "slider", "id": "s1", "bind": "$target.brightness",
  "min": 0, "max": 100, "step": 1, "debounce_ms": 150, "label": "Brightness" }
```

Defaults:

- `toggle` — no debounce; fire immediately on click; `Lww`.
- `slider` — `debounce_ms: 150` trailing + always fire on pointer release; `Lww`.

Both support the same `Full` binding shape if the author wants to override concurrency per control.

### 3. Resolver emits a `WritePlan` parallel to the subscription plan

`ResolveResponse` (and `/ui/render`) gains:

```json
"writes": [
  { "component_id": "t1",
    "path": "/buildings/building-1",
    "slot": "enabled",
    "concurrency": "lww" },
  { "component_id": "settings-form",
    "path": "/buildings/building-1",
    "slot": "settings",
    "concurrency": "occ",
    "generation": 47 }
]
```

**Keying.** Entries key on `component_id`. Authors guarantee `component_id` uniqueness within a resolved tree — same contract the subscription plan already relies on. Row-level dynamic controls (per-row toggles in a `table`) are out of scope; see non-goals.

**Generation on OCC entries.** When `concurrency: "occ"`, the resolver bakes the current slot generation into the WritePlan entry at resolve time. The client holds that number as the initial value and **updates it on every SSE echo** for that slot path — every `slot_changed` event carries the new generation, and the client replaces its held value before the next write. This is the only source of the generation number; the client never derives it independently or stores it in a parallel structure. A stale generation at write-time is exactly the case OCC is designed to detect: 409, conflict banner, re-resolve.

LWW entries omit `generation` entirely (the write goes without `expected_generation`).

**ACL.** If the caller lacks write on the target slot, the entry is dropped from the WritePlan. The component still renders so the current value is visible, but in a disabled state (see §4). One audit event per redaction, same pattern as read-ACL.

**Client-side contract (defensive):** if a bound control has no matching WritePlan entry at render time, the control renders disabled regardless of its type. This covers the ACL case, any future denial cases, and client-side bugs that might otherwise silently no-op a write.

### 4. Renderer wiring (`ui-core/src/sdui/`)

Renderer code lives in `ui-core/src/sdui/` — **not** `studio/` and **not** `frontend/`. Studio has no `sdui/` folder and must not get one; the renderer has to stay in `ui-core` so every downstream frontend (Studio, a mobile admin, a block MF panel embedding an SDUI subtree) picks up bound controls from the same place. See [HOW-TO-ADD-CODE.md § Rule C](../../../HOW-TO-ADD-CODE.md) — the "portable brain" line.

- `toggle` / `slider` components read their initial value from the resolved tree.
- On change, look up the `WritePlan` entry by `component_id`; if none exists, render disabled (see §3 contract). Otherwise debounce per IR spec and issue `POST /api/v1/slots` with `{ path, slot, value, expected_generation? }`. `expected_generation` is included only when the WritePlan entry carries a `generation` (i.e. OCC mode); the client sends the currently-held generation (baked from resolve, updated by every SSE echo on that path+slot).
- SSE echo flows through existing `useSubscriptions` → React Query cache update → component re-renders with the authoritative value. On each echo for a slot that appears in the WritePlan, the client updates the held generation for that entry.
- Optimistic display (reusing the machinery shipped in SDUI.md § S7 for action responses): during the round-trip, show the user's value immediately; the SSE echo (or a REST-level rejection) confirms or overrides. Not a new mechanism — same `applyPatch.ts` pathway, fed from the bound-control change event instead of an action response.
- Pointer-release on a `slider` **cancels any pending debounce and replaces it** with a single release-triggered write. Never both.

### 5. Slot-write endpoint

`POST /api/v1/slots` is already the target. Confirm it handles:

- Writes without `expected_generation` cleanly (LWW mode).
- Type coercion — JSON number into a typed number slot, etc. — is the server's job; the client sends what the control produced.
- Per-slot ACL check (already in place per DASHBOARD.md). No new auth surface.

If anything in the current handler blocks LWW writes, fix it there rather than working around in the renderer.

---

## REST vs SSE — both, orthogonal

- **REST (`POST /api/v1/slots`)** — outbound mutations. Synchronous ack, validation, OCC, audit.
- **SSE (subscription subjects)** — inbound changes. One stream that echoes every mutation back to every subscriber, including the writing client. Also carries the **new generation number** for each `slot_changed` event, which OCC-mode bound controls consume to keep their held generation current.

The writing client does not skip the SSE echo. It renders the optimistic value immediately for responsiveness, then reconciles against the echo. Single source of truth, no divergent local state. A `toggle` in Tab A flipped by the user appears flipped in Tab B and in the dragging tab itself, through the same mechanism.

---

## Debounce

Client-side only, declared in the IR. The server never sees the storm.

| Control | Default | On-release fire | Rationale |
|---|---|---|---|
| `toggle` | 0 ms | n/a | Discrete event; debounce would feel broken |
| `slider` | 150 ms trailing | Always fires on pointer up | Dragging = one write at settle, plus committed final |
| `field` (future) | 400 ms trailing | Fires on blur / Enter | Typing, not dragging |

Authors can override via `BindingSpec::Full { debounce_ms: Some(…) }`. No floor — if the author writes `debounce_ms: 0` they get no debounce, and pointer-release still cancels-and-replaces the pending write as above. A reckless author who writes `debounce_ms: 0` on a slider will flood the server; that's their bug to fix, not the framework's to prevent. The slot-write endpoint has its own rate-limit backstop (existing ACL / audit layer).

---

## Persistence on page load

No new code needed. Current values come from the resolver baking bindings into the render tree at resolve time; this already works for every read binding. A bound `toggle` sees its current slot value on first paint — same pathway as a `text` or `kpi` reading a slot.

---

## Concurrency semantics

Two modes, per-binding, default `Lww`:

- **`lww` (last-write-wins)** — no `expected_generation` sent. WritePlan entry has no `generation` field. Appropriate for continuous controls (slider) and for toggles where "whoever flipped it last wins" is the intuitive model.
- **`occ` (optimistic concurrency)** — WritePlan entry carries a `generation`, baked by the resolver and kept current on the client by SSE `slot_changed` echoes. Every write sends `expected_generation`; 409 on mismatch → non-dismissable conflict banner (same UX the builder already uses per DASHBOARD.md "pre-Stage-1 substrate") + re-resolve. Appropriate when two users editing the same slot simultaneously is a real concern.

Forms default to `occ` (unchanged); single-value bound controls default to `lww`. Authors override when needed.

### Slot validation — who owns it

The **slot schema is the authoritative constraint**, full stop. `min`/`max`/`step` on IR `slider` (and equivalents on other controls) are **rendering hints only** — they shape the pixel widget, they do not constrain the value the server accepts. If an author wants a slot constrained to 0–100, they author that into the slot schema. The transport layer rejecting values based on IR `min`/`max` would mean transport validation depends on IR shape, which inverts the `transport → domain → data` layering rule in CODE-LAYOUT.md.

Consequence: a buggy IR with `slider.max: 50` over a slot schema allowing 0–255 is an **authoring bug**, not a framework gap. The slot accepts 200 because the slot says 0–255. Fix: narrow the slot schema. The framework does not double-source the constraint.

---

## What changes, crate by crate

| Crate / package | Change |
|---|---|
| `crates/ui-ir` | Add `BindingSpec`, `Concurrency`. Add `toggle` + `slider` variants. Bump IR minor version. JSON Schema regenerates. |
| `crates/dashboard-runtime` | Resolver walks `bind` fields; emits `WritePlan` alongside `SubscriptionPlan`; for OCC entries, reads the current slot generation and bakes it into the entry. Save-time validator rejects bindings that resolve to non-slot targets. New unit-test fixtures covering: LWW entry shape, OCC entry with baked generation, ACL-dropped entry, binding to missing slot, binding to non-slot target, collision check on `component_id`. |
| `crates/dashboard-transport` | `ResolveResponse` + `/ui/render` response shape gains `writes`. `/ui/resolve --dry-run` reports binding errors for writes the same way it does for reads. Fixture additions: `ui-resolve/with-writes-lww.json`, `ui-resolve/with-writes-occ.json`, `ui-resolve/write-acl-redacted.json`. |
| Slot-write handler | Confirm LWW path (no `expected_generation`) is clean. Fix here, not in the client, if anything's off. Also confirm REST and in-process writes both go through [`GraphStore::write_slot_inner`](../../crates/graph/src/store.rs) so generation monotonicity and `slot_changed` emission are identical regardless of writer — UI liveness depends on this being one code path, not two. |
| `clients/rs`, `clients/ts`, `clients/dart` | Type mirrors for `BindingSpec`, `Concurrency`, `WritePlan`. No new endpoint methods — `/slots` already ships. Fixtures: add `ui-resolve/with-writes.json`. |
| CLI | `agent ui resolve -o json` shows writes alongside subscriptions. No new subcommands. |
| `ui-kit` | **Prerequisite.** Add Shadcn `switch.tsx` and `slider.tsx` to `ui-kit/src/components/ui/` — neither exists today. Pure visual primitives only, no hooks, no data opinions. SDUI's `toggle` / `slider` wrappers in `ui-core` compose these. If `ui-core` inlines its own primitives, Rule C is violated. |
| `ui-core/src/sdui/` | `toggle.tsx`, `slider.tsx` (wrapping `@listo/ui-kit` primitives). Shared `useBoundWrite(componentId)` hook: looks up WritePlan, debounces, issues slot write, reconciles with SSE. Exported from [ui-core/src/index.ts](../../../ui-core/src/index.ts) so block MF bundles rendering an `SduiRenderer` subtree get bound controls for free. |
| `block-ui-sdk` | Decide re-export surface: a block that renders a plain (non-SDUI) toggle in its own MF panel currently has `useAction` and `useSlot` but no sanctioned slot *writer*. Either re-export `useSlotWriter` from `@listo/ui-core` via [block-ui-sdk/src/index.ts](../../../block-ui-sdk/src/index.ts), or document that non-SDUI block writes go through `useAgentClient().slots.writeSlot` directly. Pick one before S3 so block authors don't reach into `ui-core` and break Rule C. |

No new endpoints. No `HandlerRegistry` changes. No per-kind code.

---

## Acceptance criteria

- Author writes `{type: "toggle", bind: "$target.enabled"}` inside a `ui.page.layout`; renders live against any node with an `enabled` boolean slot. Flipping it persists.
- Author writes `{type: "slider", bind: "$target.brightness", min: 0, max: 100}`; dragging emits a debounced sequence that settles on one authoritative value. One write per drag-settle, not 60.
- Flipping a toggle in Tab A updates Tab B within one SSE hop. No polling.
- Writing tab sees its own write echoed back through SSE and treats the SSE value as authoritative (tested: if the server normalises / clamps the value, the UI reflects the normalised value, not the user-entered one).
- Binding to a slot the caller cannot write yields a read-only rendering + one audit event; reads still work.
- `concurrency: "occ"` on a toggle: two tabs flipping simultaneously — one succeeds, the other shows a conflict banner and refetches. No silent overwrite.
- `concurrency: "lww"` on a slider: rapid concurrent drags from two tabs never 409; last write lands.
- `agent ui resolve --dry-run` reports a precise error when a `bind` points at a missing slot, non-slot, or type-mismatched target.
- Zero new handlers in the `HandlerRegistry` for any of the above.
- Zero domain-specific strings in `ui-ir` or `ui-core/src/sdui/toggle.tsx` / `slider.tsx`.
- Reusability smoke test: a hypothetical second frontend that depends only on `@listo/ui-kit` + `@listo/ui-core` + `@listo/agent-client` (no Studio) can render a bound `toggle` / `slider` with zero additional code. If the test requires reaching into `studio/`, the renderer landed in the wrong repo.

---

## Falsification tests

One per shipped control, plus the cross-tab liveness test:

1. **Toggle on an arbitrary boolean slot** — heartbeat demo page gains a toggle bound to a boolean slot on the heartbeat node. Flick it in CLI (`agent slots write … false`); UI updates via SSE. Flick it in UI; CLI read (`agent slots read …`) reflects it. No handler registered.
2. **Slider on an arbitrary numeric slot** — same page gains a slider bound to `count` (or a dedicated numeric slot). Drag it; observe one settled write via server logs, not a flood.
3. **Two-tab liveness** — two browser tabs open on the same page; a change in either appears in the other within one SSE hop.
4. **OCC conflict** — two tabs with a toggle in `occ` mode flipped simultaneously: one banner, one success, zero silent overwrites.
5. **ACL redaction** — authenticated caller without write on the target slot sees the control rendered read-only (or as a forbidden placeholder — exact UX TBD in review); one audit event per redaction.

---

## Decisions (previously open)

These were flagged in review and are now closed in-spec. Listed here so reviewers see them explicitly rather than buried in prose.

1. **Write-denied UX** → **disabled-with-tooltip.** The component renders normally with its current value visible (matching read-ACL behaviour on the underlying slot) but the control is non-interactive and surfaces a tooltip explaining why. **`ui.widget.forbidden` is not used for write-denied bindings** — it remains reserved for read-denied components where the value itself is hidden. No new `read_only` flag on components either; disabled-state is a renderer concern derived from "no WritePlan entry", not an IR-level field.
2. **Debounce floor** → **no floor.** Authors who write `debounce_ms: 0` on a slider get what they asked for. Pointer-release cancels-and-replaces pending debounce to prevent duplicate writes. Slot endpoint's own rate limiting is the backstop.
3. **`form` + `BindingSpec` consistency** → **unify eventually, not in this delivery.** Long-term intent: `form.bindings` moves to the same `BindingSpec` shape so every part of the IR uses one binding grammar. Not in scope here because forms already work, the migration is mechanical, and doing it alongside the toggle/slider work expands the blast radius without new capability.
4. **Write-only controls** → **out of scope.** A bound control is read-and-write by definition. Pure fire-and-forget triggers remain the job of `button + action`.
5. **Slot validation ownership** → **slot schema is authoritative.** IR `min`/`max`/`step` are rendering hints; see "Slot validation — who owns it" above.

## Remaining open questions

None blocking implementation. Reviewers: call out any you want me to revisit before S1.

---

## Stages

| S | Deliverable |
|---|---|
| S1 | `BindingSpec` + `Concurrency` types in `ui-ir`; JSON Schema updated; IR version bumped. No runtime wiring yet. |
| S2 | Resolver walks `bind`; emits `WritePlan` on `/ui/resolve` and `/ui/render`. Dry-run reports binding errors for writes. Client type mirrors (rs/ts/dart) + fixtures. |
| S3 | `toggle` variant end-to-end: IR → resolver → renderer → slot write → SSE echo. Two-tab liveness test green. |
| S4 | `slider` variant end-to-end, including debounce + on-release fire. Falsification test 2 green. |
| S5 | OCC path: `concurrency: "occ"` on toggle, conflict banner, 409 refetch. Falsification test 4 green. |
| S6 | ACL redaction for write-denied bindings; audit event shape locked in. Falsification test 5 green. |

Each stage is independently shippable. S1 is a pure additive type change; later stages layer on top.

---

## One-line summary

**Two-way binding as an IR primitive: a `toggle` or `slider` declares which slot it edits, the resolver emits a write plan alongside the read tree, the renderer writes via the existing generic `/slots` endpoint and reconciles via the existing SSE stream — no handler, no `/ui/action` detour, no per-kind code, one source of truth.**
