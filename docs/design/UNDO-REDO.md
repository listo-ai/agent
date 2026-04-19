# Undo / Redo — Design

DB-backed undo/redo for two surfaces: **flow documents** (graph topology, node placement, links) and **node settings** (per-node config payload). Editor-local undo (a JS command stack) is deliberately out of scope — it's a useful nicety but not what this doc covers. We want history that survives reload, device switch, and — for flows — branching and audit.

Authoritative references: [VERSIONING.md](../design/VERSIONING.md), [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md), [RUNTIME.md](../design/RUNTIME.md).

---

## Goals

- **Durable history.** Undo survives page reload, tab close, and switching between Studio desktop and browser.
- **Two granularities.** Whole-flow revisions (graph shape) and per-node settings revisions (property panel edits).
- **Cheap writes.** Editing a slider shouldn't write a 20 kB flow blob on every keystroke.
- **Bounded storage.** History is capped per flow and per node; old entries are pruned, not kept forever.
- **Works on edge.** Must run on SQLite with a 350 MB agent budget. No Postgres-only features on the hot path.
- **Multi-user safe.** Two users editing the same flow must not silently overwrite each other's history.

## Non-goals

- **Real-time collaborative editing (CRDT/OT).** Out of scope. One editor at a time per flow; conflicts surface as "someone else changed this, reload."
- **Branch/merge.** Linear history per flow. No git-style branches in v1.
- **Undo across flow deletions.** Deleting a flow is a hard delete from the user's POV; recovery is a separate "trash" feature.
- **Undo of runtime state** (node inbox contents, counters, etc.). Only authored state is versioned.

---

## Model

Two tables, same shape. Both are append-only event logs keyed by the thing they version.

### `flow_revisions`

| column | type | notes |
|---|---|---|
| `id` | ULID | monotonic, sortable |
| `flow_id` | ULID | FK → `flows.id` |
| `parent_id` | ULID? | previous revision for this flow; NULL for first |
| `seq` | INTEGER | per-flow sequence, `parent.seq + 1`; used for optimistic concurrency |
| `author` | TEXT | user id (Zitadel subject) or `system` |
| `op` | TEXT | `create`, `edit`, `undo`, `redo`, `revert`, `import`, `duplicate`, `paste` — **load-bearing**; used by redo reconstruction |
| `target_rev_id` | ULID? | for `undo`/`redo`/`revert`: the revision whose content was re-materialised. NULL for forward edits. |
| `summary` | TEXT | short human label, e.g. "added 2 nodes, 1 link" |
| `patch` | BLOB | JSON-Patch (RFC 6902) against `parent_id`'s materialised document |
| `snapshot` | BLOB? | full document; written every N revisions (default 20) or on first revision |
| `created_at` | TIMESTAMP | |

`seq` is **authoritative** for optimistic concurrency (simple integer compare on write). `parent_id` is the chain pointer used to walk history; it must be consistent with `seq` but writes do not re-check it — one invariant, checked in one place.

### `node_setting_revisions`

| column | type | notes |
|---|---|---|
| `id` | ULID | |
| `flow_id` | ULID | denormalised for pruning and for the "undo in flow scope" case |
| `node_id` | ULID | FK → node |
| `parent_id` | ULID? | |
| `seq` | INTEGER | per-node sequence |
| `author` | TEXT | |
| `op` | TEXT | same vocabulary |
| `target_rev_id` | ULID? | same semantics as on flow revisions |
| `schema_version` | TEXT | kind schema version the payload was authored against; lets materialisation run migrations (see below) |
| `patch` | BLOB | JSON-Patch against previous settings |
| `snapshot` | BLOB? | full settings blob; written every N or on first |
| `created_at` | TIMESTAMP | |

### Why patches + periodic snapshots

- Patches keep writes cheap (a slider edit is ~40 bytes, not 20 kB).
- Snapshots bound read cost: reconstructing revision K is at most N patch applications, not K.
- Snapshot cadence is a tuning knob per deployment (cloud can afford more; edge stays lean).

### The "current" pointer

`flows.head_revision_id` and `nodes.settings_head_revision_id` are the materialised current state. Reads of the live document never touch the revisions table — they hit the denormalised current row like today. Revisions are only consulted for history UI, undo, and revert.

---

## Operations

### Edit

1. Client submits an edit against `expected_head = flows.head_revision_id` (or the node equivalent).
2. Server begins a transaction:
   - If `flows.head_revision_id != expected_head` → `409 Conflict`. Client must rebase (reload) or show "someone else edited this."
   - Compute patch against current; write `flow_revisions` row; update `flows.head_revision_id` and the materialised document.
3. Return the new revision id.

This is the standard optimistic-concurrency pattern. `seq` is authoritative for conflict detection; `parent_id` is a chain pointer kept consistent with `seq`.

### Undo

Undo is **not** "delete the last revision." It's "append a new revision whose content equals revision N-1."

- Client calls `POST /flows/{id}/undo` with `expected_head`.
- Server walks from head back along the **undo chain** (see below) to find the target revision, materialises that state, writes a new revision with `op = "undo"` and `target_rev_id = <that revision>`, and moves `head` forward.
- This preserves a clean audit trail and makes redo reconstructable from the log alone.

### Redo — reconstructed from the log, not from a cursor

The source of truth is the revisions table. The redo chain is derivable by walking backward from `head` while `op IN ('undo', 'redo')`:

- Each `undo` entry's `target_rev_id` is a revision that can be redone.
- Each `redo` entry advances the pointer by one.
- The first non-`undo`/`redo` op encountered **terminates** the chain — standard editor semantics: any forward edit truncates redo.

So: `POST /flows/{id}/redo` computes the next redo target from the revisions table itself. No in-memory cursor is required for correctness.

**Optional performance cache.** An in-memory `(user, flow_id) → next_redo_rev_id` cache in the agent process is allowed as a latency optimisation. It must be treated as a hint: every redo call revalidates against the log, and cache miss / stale cache just costs one extra query. Explicitly **not** Redis-backed — Redis is not a source of truth here, and introducing it for this would add a dependency we don't need.

**Concurrency.** Two simultaneous `undo` calls from the same user race on `expected_head` — the second gets `409 Conflict` because head has moved. This is the same optimistic-concurrency story as edits, not a second invariant.

### `expected_head` semantics across endpoints

Every mutating endpoint takes `expected_head` and returns `409 Conflict` on mismatch. Specifics:

| endpoint | `expected_head` | additional body | 409 behaviour |
|---|---|---|---|
| edit | current head id | patch or new document | client reloads + rebases |
| undo | current head id | — | client reloads |
| redo | current head id | optional `expected_target` (the redo target the client thinks is next) | 409 if head moved; **also 409 if `expected_target` is provided and doesn't match server's computed next** — handles the two-tab case where tab A undid, tab B's redo cursor is stale |
| revert | current head id | `target_rev_id` | client reloads |
| duplicate / paste | current head id | — | client reloads |

The two-tab case: tab A calls undo, tab B's view is stale. If tab B calls redo without `expected_target`, the server redoes whatever is next per the log — which may surprise the user. If tab B sends `expected_target`, it gets 409 and can reload. Clients **should** send `expected_target`; servers accept absence for simple cases.

### Revert to revision K

Same mechanism as undo but targeted: write a new revision whose content equals revision K. `op = "revert"`. The revisions between K and current are not deleted — they remain in history.

### Node-settings vs flow-scope undo

A user editing node settings in the property panel is in **node scope**: Ctrl+Z undoes the last setting change for that node only. Switching to the canvas (no node selected) puts them in **flow scope**: Ctrl+Z undoes the last flow-level change (add/move/delete/link).

Internally these hit different endpoints (`/nodes/{id}/settings/undo` vs `/flows/{id}/undo`) against different revision tables. The UI decides scope based on focus.

**Interaction with flow revisions**: settings edits do *not* write a flow revision. The flow document references node settings by node id; node settings are versioned independently. This is the whole reason there are two tables — otherwise every slider drag would rewrite the whole flow.

### Cross-scope undo is intentionally independent — and surprising

The two stacks do **not** know about each other. Consider the sequence:

1. User drags node A to a new position → flow revision `F2`.
2. User edits node A's slider → node-settings revision `S2` on A.
3. User clicks the canvas (flow scope) and hits Ctrl+Z → flow reverts to `F1` (A is back at its old position). **A's slider value stays at `S2`.**

The user now sees a visual state — A at old position with new slider value — that never existed as a historical state. This is a consequence of decoupling the stacks, not a bug; merging them would reintroduce the "every slider drag rewrites the flow" problem we explicitly rejected.

**UX mitigations** (required, not optional):

- The undo button tooltip always shows the scope and the action it will undo: "Undo: move node A" vs "Undo: slider on node A."
- When flow-scope undo/redo changes anything, show a brief toast: "Flow reverted. Node settings unchanged." — only when the reverted range contained the currently selected node.
- The revision timeline in the history UI interleaves both streams chronologically with scope badges, so a user inspecting history sees the full picture.

We do **not** try to auto-undo settings when the user undoes a flow change containing that node. That's a rabbit hole (what if the user wanted to keep the slider?) and the toast is the honest answer.

---

## Retention & pruning

Per-flow cap: **200 revisions** default, configurable. Per-node settings cap: **100 revisions**. Prune runs:

- Opportunistically on write when over cap, in the same transaction.
- Nightly via a maintenance task for cleanup after flow/node deletions.

Pruning has **two invariants**:

1. **Snapshot chain integrity.** If we delete revision K and K held a snapshot, the next surviving revision inherits a fresh snapshot. Concretely: pruning rewrites the oldest surviving revision to carry a full snapshot, then deletes anything older.

2. **Redo-chain protection.** Because redo targets are derived from the revision log (the undo chain walking back from head), pruning must **not** remove any revision that appears as `target_rev_id` on an `undo` entry in the live undo chain suffix of head. Concretely, pruning walks backward from head past the contiguous run of `undo`/`redo` ops, collects every `target_rev_id` in that run, and **pins** those revisions as ineligible for deletion. Everything older than the first non-undo/redo op is safe to prune (because any forward edit truncates redo, those older targets are unreachable anyway).

The pin set is small in practice (bounded by the depth of the user's current undo sequence) and computed in a single backward scan inside the same transaction as the prune.

---

## HTTP surface (additions)

```
GET    /flows/{id}/revisions                  → list, paged, newest first
GET    /flows/{id}/revisions/{revId}          → materialised document at that revision
POST   /flows/{id}/undo                       → body: { expected_head }
POST   /flows/{id}/redo                       → body: { expected_head, expected_target? }
POST   /flows/{id}/revert                     → body: { expected_head, target_rev_id }

GET    /nodes/{id}/settings/revisions
GET    /nodes/{id}/settings/revisions/{revId}
POST   /nodes/{id}/settings/undo              → body: { expected_head }
POST   /nodes/{id}/settings/redo              → body: { expected_head, expected_target? }
POST   /nodes/{id}/settings/revert            → body: { expected_head, target_rev_id }
```

All mutating endpoints take `expected_head` for optimistic concurrency and return the new head id. Redo additionally accepts `expected_target` for the two-tab stale-cursor case (see `expected_head` semantics table above); omitting it falls back to whatever the server computes as the next redo target.

---

## Duplicate and copy / paste

Adjacent feature, same revision machinery. A duplicate or paste is **one flow edit** — it writes a single revision whose patch adds N nodes and M links. Undo reverts the whole paste atomically. This is the right default: users think of "I pasted that subflow" as one action.

### Scope options

Three variants, all driven by a single `include_children` flag (and a `rewire` flag for paste):

| Action | What it does | Options |
|---|---|---|
| **Duplicate** | Server-side clone of selected node(s) within the same flow, offset by a small canvas delta | `include_children`, `include_links` |
| **Copy** | Serialise selection to a client-side clipboard (JSON blob, versioned) | `include_children` |
| **Paste** | Deserialise clipboard into a target flow (same or different) | `include_children`, `rewire: "internal-only" \| "dangling-ok"` |

**`include_children`**: when a selected node is a **container** (subflow, group, tab) — what "child" means is [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md)-specific — `true` recursively includes its children; `false` duplicates only the container shell. Default `true`; a subflow without its children is almost never what users want, but we expose the option because advanced users sometimes do (e.g. copy a group's layout/settings to rewrap different content).

**`include_links`** (duplicate only): whether links *between the selected nodes* come along. Default `true`. Links from selected nodes to *outside* nodes are never auto-duplicated — they'd create ambiguous fan-out.

**`rewire`** (paste only):
- `internal-only` (default): only links fully contained within the pasted selection survive; external references become dangling and are dropped.
- `dangling-ok`: keep external link references; if the target flow has a node with the same id, reconnect; otherwise drop. Useful when pasting back into the source flow.

**Dropped-link feedback is mandatory.** Both modes can drop links silently, which is a trust problem — users paste 4 nodes expecting 6 links, get 3, and don't know why. The paste response and the revision summary both surface the count and identifiers:

```json
{
  "head_revision_id": "01K...",
  "summary": "pasted 4 nodes, 3 links; 2 external links dropped, 1 dangling rewired",
  "warnings": [
    { "kind": "link_dropped", "reason": "external_target_missing", "source": "n_abc.out", "target": "n_xyz.in" },
    { "kind": "link_dropped", "reason": "external_target_missing", "source": "n_def.out", "target": "n_qrs.in" },
    { "kind": "link_rewired", "source": "n_abc.out", "target": "n_local.in" }
  ]
}
```

The `summary` string is also persisted on the revision row so history UIs show the same count later. Empty `warnings` array on a clean paste.

### ID handling

Every duplicated / pasted node gets a **fresh ULID**. Never reuse ids across clones — that's how we get silent aliasing bugs. Links inside the selection are rewritten to the new ids in one pass before the revision is written.

### Clipboard format

Client-side clipboard is a versioned JSON blob:

```json
{
  "version": 1,
  "kind": "us.clipboard/v1",
  "source_flow_id": "01J...",
  "nodes": [ /* full node docs, including settings snapshot */ ],
  "links": [ /* links among the selection only */ ],
  "children_included": true
}
```

Version-gated so we can evolve it. The **server** owns version enforcement — if the posted clipboard's `version` or `kind` is unrecognised, the paste endpoint returns `400` with a structured `unsupported_clipboard_version` error. The client may show a friendlier message first, but the server never trusts the client to gate. Pasting a `v1` blob into a `v2` server runs server-side migration; `v2` → `v1` fails at the server, not the client.

**Cross-flow paste** is just paste where `source_flow_id != target_flow_id`. No special path.

**Cross-tenant enforcement is server-derived, not clipboard-derived.** The `source_flow_id` inside the clipboard blob is user-controlled and cannot be trusted for the tenant check — a modified blob could claim any source. Instead:

1. The server resolves the **target flow's tenant** from the authenticated session + `flow_id` path parameter.
2. For each node in the clipboard, the server re-validates that the node kind is installed and permitted in the target tenant. Kinds scoped to another tenant (or not installed) cause the paste to fail before any mutation.
3. `source_flow_id` is retained only for audit/telemetry — never for authorisation.

This closes the IP-leak path: a blob copied from tenant A can be pasted into tenant B only if tenant B already has access to the kinds it contains.

### `include_children` vs clipboard's `children_included` — clipboard is authoritative

The clipboard records `children_included` at copy time. The paste body sends `include_children`. When they disagree, **the clipboard is authoritative** — `include_children` on paste can only *filter down*, never conjure data that wasn't captured.

| clipboard | paste body | result |
|---|---|---|
| `children_included: true` | `include_children: true` | paste container + children (default) |
| `children_included: true` | `include_children: false` | paste container shell only; children filtered out at paste time |
| `children_included: false` | `include_children: true` | **server rejects with `400 missing_children`** — the requested children aren't in the blob, and silently pasting a shell would be worse than failing |
| `children_included: false` | `include_children: false` | paste container shell only |

The rejection case is explicit so users re-copy with children, rather than getting a mysterious "shell without children" paste they didn't ask for.

### HTTP surface (additions)

```
POST   /flows/{id}/duplicate                  → body: { node_ids, include_children, include_links }
POST   /flows/{id}/paste                      → body: { clipboard, target_position, rewire, include_children }
```

Both return `{ head_revision_id, summary, warnings }` (see "Dropped-link feedback is mandatory" above). No new endpoint for "copy" — that's pure client-side serialisation of already-fetched data.

**`target_position` anchor semantics.** `target_position` is the canvas coordinate where the **top-left corner of the pasted selection's bounding box** is placed. If that position collides within a small epsilon (±8 px) with an existing node or with another paste submitted concurrently by a second user, the server applies a small deterministic jitter (fixed offset per paste, derived from the new head revision id) so concurrent pastes from two tabs don't stack exactly. Duplicate's "small canvas delta" follows the same rule: bounding-box top-left, offset by a fixed delta, with the same jitter rule on collision.

### Undo interaction

Duplicate and paste each produce **one** revision with `op = "duplicate"` or `op = "paste"` and a summary like "pasted 4 nodes, 3 links." Ctrl+Z reverts the whole thing. No partial undo of a paste — that's a footgun and nobody expects it.

### Node settings are not duplicated as revisions

When a node is duplicated, its current settings are copied into the new node as its **initial** settings (first `node_setting_revisions` entry with `op = "create"`). The source node's settings history is **not** carried over — the clone starts fresh. Dragging history along would confuse audit ("who edited this?" → "someone, on a different node, last year").

**Orphan handling on undo-of-duplicate.** If a duplicate is undone (or the flow revision that created it is pruned away), the cloned node is logically deleted but its `node_setting_revisions` rows persist — the two tables have independent pruning. This is fine for live read paths (no node, nobody queries the rows) but the rows are dead weight. The **nightly maintenance task** (already responsible for pruning after flow/node deletions) explicitly sweeps `node_setting_revisions` where the `node_id` has no live node **and** no flow revision in the retained window references it. Calling this out so implementers don't discover it after shipping.

---

## Storage cost sanity check

Edge, typical flow: 50 nodes, 80 links, ~15 kB serialised.

- 200 revisions, avg patch 200 bytes, snapshot every 20 → 200·200 + 10·15_000 ≈ **190 kB per flow**.
- 20 flows on a gateway → ~4 MB. Comfortably within the 350 MB budget.

Node settings, typical: 1 kB payload, avg patch 80 bytes, snapshot every 20.

- 100 revisions · 80 + 5 · 1_000 ≈ **13 kB per node**.
- 500 nodes across a gateway → ~6 MB.

Cloud is unbounded by comparison and not a concern.

Numbers are **uncompressed JSON**. With zstd (see open questions) expect 3–5× reduction; estimates here are deliberately conservative.

---

## Historical settings materialisation (v1 contract)

Extension authors ship outside our release cycle. A node kind on v2 of its schema may have revisions in the table written against v1. `GET /nodes/{id}/settings/revisions/{revId}` must always return *something usable*, not crash or return corrupted data.

**Contract**, effective v1:

1. Every `node_setting_revisions` row carries `schema_version` (the kind's schema version at write time).
2. On materialisation, the server consults the kind registry for a migration chain from `schema_version` → current.
3. **Happy path:** migrations exist, they run, the UI gets current-shape data.
4. **Degraded path:** no migration available (kind is v3, revision is v1, author shipped no v1→v2 migration). The endpoint returns:
   ```json
   {
     "revision_id": "…",
     "schema_version": "1.0",
     "current_schema_version": "3.0",
     "payload": { /* raw, unmigrated */ },
     "migration_status": "unavailable"
   }
   ```
   The UI renders a read-only diff/preview with a "this revision predates the current kind schema; revert is disabled" banner. **Revert is blocked** when `migration_status != "ok"` — reverting to uninterpretable data would corrupt the live state.
5. **Missing kind** (extension uninstalled): same shape with `migration_status = "kind-missing"`. Same restriction.

The registry hook is a single method on the kind provider: `migrate_settings(from: SchemaVersion, to: SchemaVersion, payload: Value) -> Result<Value, MigrationError>`. Kinds that never break their schema implement a trivial identity. This is additive to [VERSIONING.md](../design/VERSIONING.md)'s existing schema-version discipline; the work is wiring, not new policy.

---

## Open questions

1. **Cross-device session continuity.** The redo chain is already reconstructable from the log on any device/tab, so "resume redo after reload" works for free. Still open: whether the UI *exposes* a redo action when the user returns to a flow after a long gap. Lean: yes, but greyed out after 24 h with an explicit "redo old action?" confirm.
2. **"Who edited what" surfacing in UI.** Data is there (`author`, `summary`); whether we expose a full activity feed is a product call, not blocked by this design.
3. **Compression.** Snapshots are JSON; worth zstd-compressing on edge? Probably yes once the feature is shipped and we see real sizes — not v1.
4. **Settings-undo toast fatigue.** The cross-scope toast described above may get noisy for power users. May need a "don't show again" or a quieter indicator.

---

## Implementation sketch

Three phases, each independently shippable.

**Phase 1 — flow revisions (backend + API).** Table, endpoints, optimistic concurrency, snapshot cadence, pruning. No UI work; wire a thin "undo" button in the existing canvas that calls the endpoint.

**Phase 2 — node settings revisions.** Same pattern for the `node_setting_revisions` table. Property panel gains its own undo/redo scope.

**Phase 3 — history UI.** Revision timeline, diff view, revert button. Nice-to-have; not required for the core undo/redo UX to work.

Client-side command stack (the fast, in-memory one) is **complementary, not a substitute**: we still want it layered on top for sub-second UX so every keystroke doesn't round-trip. It flushes into a single DB revision on debounce / blur / explicit save. That's a UX detail, not part of this design.
