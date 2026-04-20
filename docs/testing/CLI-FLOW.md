# Testing flows via the CLI

Goal: drive flow + node graph state end-to-end with `agent <cmd>` — no Studio, no hand-crafted HTTP. This is the surface AI assistants (Claude Code, Cursor, aider, shell-integrated LLMs) should use when a user prompt says *"create a flow that …"* or *"wire this sensor to that alarm"*.

**Before anything else, read [docs/testing/TESTING.md](./TESTING.md)** — it's the source of truth for local dev: which agent runs where (`make run` = 8080; `make dev` = cloud 8081 + edge 8082), which Studio port pairs with which agent (3000 / 3001 / 3002), and wipe/reset rules. Every CLI example below targets `localhost:8080` by default; point elsewhere with `AGENT_URL=http://localhost:<port>` or `agent -u http://localhost:<port> …`.

See also:

- [docs/design/CLI.md](../design/CLI.md) — command tree, global flags, output contracts (deterministic JSON, stable exit codes, `--help-json`, `agent schema`).
- [docs/testing/CLI-DASHBOARD.md](./CLI-DASHBOARD.md) — sibling doc for `ui.page` authoring.

---

## The mental model

Two shapes of state live side by side:

| | **Nodes** (graph) | **Flows** (document) |
|---|---|---|
| What | Persistent graph of kinds with slots, lifecycle, links | Named JSON documents with revision history |
| Identified by | `NodePath` (e.g. `/flow-1/heartbeat`) or `NodeId` (UUID) | `FlowId` (UUID) |
| Edited by | `agent nodes` / `agent slots` / `agent links` / `agent lifecycle` / `agent config` | `agent flows edit` / `agent flows undo` / `agent flows redo` / `agent flows revert` |
| History | Slot-level audit events; lifecycle transitions | Full revision log (`flows revisions`, `flows document-at`) |

A **flow node** (`sys.core.flow`) is a folder-like container in the graph. Child kinds live under it (heartbeat, math.add, driver points). A **flow document** is an opaque JSON blob whose shape is defined by the flow canvas — it stores canvas geometry + wire intent, separate from the graph's runtime state. Most flow authoring only touches graph nodes; documents show up when you need canvas layout or undo/redo semantics.

LLM heuristic: **prefer graph commands** (`nodes`, `slots`, `links`). Only reach for `flows edit` when you actually need to author a canvas document.

---

## Session bootstrap (always do these two first)

```bash
agent health
agent capabilities -o json
```

If `health` is non-zero the agent is not running — no CLI command will work. `capabilities` prints the API version and flow/node schema versions the running daemon speaks; pin these if you're generating fixtures.

Then discover the surface once:

```bash
agent schema --all -o json > /tmp/schemas.json
agent kinds list -o json    > /tmp/kinds.json
```

`schemas.json` is every input/output JSON Schema. `kinds.json` lists every registered kind with its containment rules + slot shapes — it tells you *which kinds can live under which parents* (so you don't issue a create that 422s for placement reasons).

---

## Creating a flow — the 90% path (graph-only)

You almost never need `flows create` to ship a working flow. You need a `sys.core.flow` graph node and child nodes under it.

### Step 1 — create the flow container

```bash
agent nodes create / sys.core.flow flow-1
```

Output (`-o json`):

```json
{ "id": "c506ba79bbcf4b189280dc20fab6c0d0", "path": "/flow-1" }
```

Note: ids are the canonical un-hyphenated 32-char form. Don't hand-format them with dashes.

### Step 2 — add behaviour nodes

Heartbeat emits a `Msg` envelope (`{_msgid, payload: {state, count}}`), not a bare number. To feed its tick-count into `math.add` (which expects a scalar on `a`/`b`), drop a `sys.compute.pluck` node between them and configure its `path` to `payload.count`.

```bash
agent nodes create /flow-1 sys.logic.heartbeat  heartbeat
agent nodes create /flow-1 sys.compute.pluck    pluck
agent nodes create /flow-1 sys.compute.math.add add
```

Check what's allowed under `/flow-1` before guessing:

```bash
agent kinds list -o json | jq '.[] | select(.containment.must_live_under[0].kind == "sys.core.flow") | .id'
```

### Step 3 — configure nodes

Each kind's `settings_schema` (from `agent kinds list`) tells you the config shape. `agent config set` replaces the blob and re-fires `on_init`:

```bash
agent config set /flow-1/heartbeat '{"interval_ms": 500, "enabled": true, "start_state": false}'
agent config set /flow-1/pluck     '{"path": "payload.count"}'
```

Individual slot writes — use for trigger inputs or status overrides:

```bash
agent slots write /flow-1/add b 100
```

The value is parsed as JSON first, string fallback. `'"hello"'` is a JSON string; `hello` is too but only if the slot accepts strings.

### Step 4 — wire nodes together

Links route slot outputs to slot inputs. Chain heartbeat → pluck → add so the pluck node reshapes the msg before it reaches the scalar-expecting `a` input:

```bash
agent links create \
  --source-path /flow-1/heartbeat --source-slot out \
  --target-path /flow-1/pluck     --target-slot in

agent links create \
  --source-path /flow-1/pluck --source-slot out \
  --target-path /flow-1/add   --target-slot a
```

Heartbeat's single `out` port carries `{ state, count }` as a Node-RED-style `msg.payload`. `sys.compute.pluck` walks the configured dot-path (`payload.count`) into the incoming msg and emits a child msg whose `payload` is the scalar — exactly what `math.add` wants.

List links to verify:

```bash
agent links list -o table
```

### Step 5 — start the flow

Nodes default to `created` lifecycle. **Every node in the chain** must be flipped to `active` — a source that's ticking into a downstream node in `created` state will push msgs onto its input slot but the behaviour's `on_message` handler won't run, so nothing propagates further.

```bash
agent lifecycle /flow-1/heartbeat active
agent lifecycle /flow-1/pluck     active
agent lifecycle /flow-1/add       active
```

`sys.logic.heartbeat` auto-runs `on_init` on `NodeCreated`, so in practice it's already ticking — but explicitly activating every node keeps LLM-generated scripts idempotent across restarts *and* ensures downstream propagation.

### Step 6 — observe

```bash
agent nodes get /flow-1/heartbeat -o json | jq '.slots[] | select(.name=="out") | .value.payload.count'
```

The `out` output slot holds the whole `Msg` envelope (`{_msgid, payload: {state, count}}`); drill into `payload.count` for the counter. To include bookkeeping slots like `pending_timer` in the response, pass `--include-internal` (or `?include_internal=true` on the REST endpoint).

Or tail the SSE stream (outside the CLI — use `curl`):

```bash
curl -N http://localhost:8080/api/v1/events
```

---

## Editing a running flow

All of these are idempotent and safe to issue while the flow is active.

| Intent | Command |
|---|---|
| Change a setting | `agent config set /flow-1/heartbeat '{"interval_ms": 1000}'` |
| Pause a node (stop dispatching) | `agent lifecycle /flow-1/heartbeat disabled` |
| Resume | `agent lifecycle /flow-1/heartbeat active` |
| Rewire an output | remove old link, create new link (links are immutable) |
| Rename a node | delete + recreate (no rename op in Phase 1) |
| Delete a node + cascade children | `agent nodes delete /flow-1/heartbeat` |
| Delete the whole flow | `agent nodes delete /flow-1` |

Slot writes hit the engine immediately — whether the caller is a link, a behaviour's `on_message`, or a CLI invocation. That means a direct `agent slots write` is a legitimate "inject a test input" command.

---

## Flow documents — when you actually need them

`agent flows …` manipulates the canvas document (geometry, wire intent, comments), not the runtime graph. You need this when:

- The user prompt asks for *"a flow that I can open in Studio"* with a specific canvas layout.
- You need undo/redo semantics (graph mutations are not revisioned; flow documents are).
- You're regenerating a flow from a saved snapshot.

### Create, list, fetch

```bash
agent flows create my-flow --document '{"nodes":[],"wires":[]}' --author claude-code
agent flows list
agent flows get <flow-uuid> -o json
```

`--document` is opaque JSON — the flow-canvas schema is not policed by the agent. What *is* policed is that `head_revision_id` is always set after `create`, and every subsequent edit is OCC-guarded.

### Edit with OCC

Every mutation carries `--expected-head <rev-id>` for optimistic concurrency. Flow:

```bash
# Fetch head
HEAD=$(agent flows get <flow-id> -o json | jq -r '.head_revision_id')

# Edit
agent flows edit <flow-id> '{"nodes":[{"id":"n1"}]}' \
  --expected-head "$HEAD" \
  --summary "add node n1" \
  --author claude-code
```

If someone else appended a revision between your `get` and your `edit`, the `edit` fails with `conflict` (exit 1). Re-fetch head and retry.

### Undo / redo / revert

```bash
agent flows undo   <flow-id> --expected-head <head-rev-id>
agent flows redo   <flow-id> --expected-head <head-rev-id>
agent flows revert <flow-id> --to <older-rev-id> --expected-head <head-rev-id>
```

All three *append* a new revision — the log is immutable. `document-at` reconstructs the materialised document at any historical revision:

```bash
agent flows document-at <flow-id> --rev-id <rev-id>
```

### List revisions

```bash
agent flows revisions <flow-id> --limit 50
```

Output includes `op` (`create | edit | undo | redo | revert`), `summary`, `author`, `created_at`, and `target_rev_id` (non-null only for undo/redo/revert).

---

## Seed presets — skip authoring for test scenarios

```bash
agent seed count_chain
agent seed trigger_demo
agent seed ui_demo
```

Each preset creates a fresh folder (fails fast if it already exists) with a known-good node topology. Use these for reproducible test harnesses — they're the CI-gate fixture for "does the engine still behave?"

---

## Error handling (the AI-reliable contract)

Every failure follows the shape from [CLI.md § 1](../design/CLI.md#1-deterministic-json-output-contract-not-best-effort):

```json
{ "code": "placement_refused", "message": "kind `sys.logic.heartbeat` cannot live under `sys.core.station`", "details": { "http_status": 422 } }
```

Exit codes are stable: `0` success, `1` user error (bad path, placement refused, conflict, not-found), `2` agent unreachable, `3` internal. A retry loop should only retry on `2`.

Common codes you'll hit while authoring flows:

| Code | Cause | Fix |
|---|---|---|
| `bad_path` | Malformed node path | Use `/`-separated segments, each `[a-zA-Z_][a-zA-Z0-9_-]*` |
| `bad_request` | Unknown kind id (e.g. `sys.nonexistent.kind`) | `agent kinds list` — copy the exact id |
| `placement_refused` | Kind can't live under this parent | Check `containment.must_live_under` on the kind |
| `not_found` | Path doesn't exist | `agent nodes list --filter 'parent_path==/flow-1'` to see what's there |
| `conflict` (on `flows edit`) | Stale `--expected-head` | Re-fetch, retry |
| `illegal_transition` | Lifecycle state machine refused | Check legal transitions in `agent kinds list` / behaviour docs |

---

## Recipe — "create a flow that heartbeats every second and counts"

One-shot script:

```bash
set -e

# 1. Container
agent nodes create / sys.core.flow ticker-demo

# 2. Heartbeat @ 1 Hz
agent nodes create /ticker-demo sys.logic.heartbeat ticker
agent config set   /ticker-demo/ticker '{"interval_ms": 1000, "enabled": true}'

# 3. Counter that sums the heartbeat's tick
agent nodes create /ticker-demo sys.compute.count counter

# 4. Wire the tick to the counter's increment input
#    (heartbeat's single `out` port carries { state, count } under msg.payload)
agent links create \
  --source-path /ticker-demo/ticker  --source-slot out \
  --target-path /ticker-demo/counter --target-slot in

# 5. Activate every node in the chain (downstream nodes in `created` state
#    won't run their on_message handler)
agent lifecycle /ticker-demo/ticker  active
agent lifecycle /ticker-demo/counter active

# 6. Verify
sleep 3
agent nodes get /ticker-demo/counter -o json | jq '.slots[] | select(.name=="count")'
```

This is the exact script an LLM should emit from *"make me a counter that ticks every second."* Each command is independently auditable, the failure mode of each is documented, and the final `jq` proves it's working.

---

## Discovery checklist for LLMs

Before emitting flow-authoring commands from a user prompt, read (once per session):

1. `agent schema --all -o json` — every input/output shape.
2. `agent kinds list -o json` — every registered kind + containment rules.
3. `agent capabilities -o json` — API + schema version pin.

Then author. Don't probe — the above three documents are authoritative.

For anything not covered here: `agent <cmd> --help-json` always works, and the output is stable JSON.
