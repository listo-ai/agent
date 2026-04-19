Two sessions of Stage 3 work just shipped. Picking what comes next.

---

## What shipped this round

**Option A — Manual-test surface** (Stage 3a-bonus, round 2):
- `POST /api/v1/links`, `GET /api/v1/links`, `DELETE /api/v1/links/:id`
- `POST /api/v1/lifecycle` `{path, to}`
- `POST /api/v1/seed` `{preset: "count_chain" | "trigger_demo"}` — one-click chain with auto-`on_init`
- SVG visual graph in the vanilla-JS UI; click-to-select; per-node lifecycle transition button; link form; unlink buttons
- `GraphStore::remove_link` + regression test
- End-to-end browser demo: seed → write `in` → count emits → trigger arms, live-wire fan-out visible in the event log

**Option B — Stage 3a-4 wire-shape fixtures:**
- `/clients/contracts/fixtures/msg/*.json` (5 variants) + `events/*.json` (8, one per `GraphEvent` variant)
- Round-trip tests in `crates/spi` and `crates/graph`; structural-equality compare
- `every_variant_has_a_fixture` guard fails if a new `GraphEvent` variant lands without a fixture
- Stale pre-existing fixtures (ISO-string `ts`, PascalCase `type`) replaced to match the actual Rust wire
- TS schemas at `/clients/ts/src/schemas/` still stale — Stage 4's problem per the original deferral

Full status: [docs/sessions/STEPS.md](sessions/STEPS.md).

---

## Three options for next session

### Option C — Stage 3b Wasm flavour (the remaining Stage 3 unknown)

The biggest technical bet in the project per Stage 3's "Proves" line: does one `NodeBehavior` trait really cover native + Wasm + process? Scope:

- Wasmtime in `extensions-host`; fuel metering + memory caps
- Host-function allowlist (`emit`, `read_slot`, `update_status`, `log`, `schedule`)
- Wasm adapter feature in `extensions-sdk`
- Example kind `sys.wasm.math_expr`
- End-to-end test that fuel exhaustion produces `NodeError`, not a killed agent

Multi-session. Low visibility until the example runs. Highest architectural value.

---

### Option D — Stage 2c engine-as-a-node + Stage 2d observability

Both are already-scoped "no parallel state" follow-ups sitting in STEPS.md. Together they're ~300 LOC of graph/engine refactor + log-call migration, no new architectural bets. What you get:

- `sys.agent.self` + `sys.agent.engine` kinds with `state` / `flows_running` / `last_transition_ts` status slots
- Engine writes its own state into the graph — a flow can subscribe to engine transitions via the same `SlotChanged` fabric as everything else
- `SafeStatePolicy` becomes a config-role slot on each writable output
- Every `tracing::*!` in `engine` + `graph` + `apps/agent` routes through `observability::prelude` with canonical fields

Value: the "everything is a node" rule stops having asterisks. Cost: one session, no visible UI change.

---

### Option E — More manual-test scaffolding (keep the browser alive)

Build on Option A's momentum with the obvious next scenarios:

- **Kind picker in the "create" row** — fetch `/api/v1/kinds` (new), render a dropdown of available kinds instead of typing the kind id
- **`DELETE /api/v1/nodes/:id`** — delete a node from the UI (currently you can only create)
- **Kind-aware seed presets** — `trigger_chain` (count → trigger → count), `fanout` (one count → three triggers), exercises live-wire fan-out visually
- **Inline config editor** — click a node, see its current config JSON, edit + submit via `POST /api/v1/config`
- **Event-log filter** — checkboxes to suppress `SlotChanged` noise while you're watching lifecycle

Low per-item cost, high demo value, keeps each session "clickable" while 3b brews.

---

## Recommendation: **D then C**.

Rationale: Option A gave us a *visible* runtime; Option B locked the wire. The remaining Stage 3 risk is the Wasm unknown (C), but C is a multi-session slog with no intermediate wins. D is the last piece of Stage 2 cleanup before the graph model is truly consistent — doing it now means everything C / Stage 4 hangs off a graph that doesn't contradict its own rules. E is fun but doesn't unblock anything.

If you disagree: E is the right call if you want to keep the browser-demo momentum, C is the right call if you're tired of infrastructure and want to cash in on the architectural bet.

Tell me C / D / E.
