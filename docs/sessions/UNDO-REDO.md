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
| `op` | TEXT | `create`, `edit`, `undo`, `redo`, `revert`, `import` — for auditing, not logic |
| `summary` | TEXT | short human label, e.g. "added 2 nodes, 1 link" |
| `patch` | BLOB | JSON-Patch (RFC 6902) against `parent_id`'s materialised document |
| `snapshot` | BLOB? | full document; written every N revisions (default 20) or on first revision |
| `created_at` | TIMESTAMP | |

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

This is the standard optimistic-concurrency pattern. `seq` + `parent_id` exist to make conflicts and history linearisation verifiable.

### Undo

Undo is **not** "delete the last revision." It's "append a new revision whose content equals revision N-1."

- Client calls `POST /flows/{id}/undo`.
- Server looks up `head.parent_id`, materialises that state, writes a new revision with `op = "undo"` pointing at it, and moves `head` forward.
- This preserves a clean audit trail (you can see a user hit undo) and makes redo trivial.

### Redo

Server tracks a per-session **redo cursor**: when the last op on this flow/node by this user was an `undo`, the next `redo` re-applies the next-forward revision along the undo chain. Any *non-undo/redo* edit **truncates the redo cursor** — standard editor semantics. The cursor lives in Redis/in-memory keyed by `(user, flow_id)` and expires after the edit session; it does **not** need to survive reload.

### Revert to revision K

Same mechanism as undo but targeted: write a new revision whose content equals revision K. `op = "revert"`. The revisions between K and current are not deleted — they remain in history.

### Node-settings vs flow-scope undo

A user editing node settings in the property panel is in **node scope**: Ctrl+Z undoes the last setting change for that node only. Switching to the canvas (no node selected) puts them in **flow scope**: Ctrl+Z undoes the last flow-level change (add/move/delete/link).

Internally these hit different endpoints (`/nodes/{id}/settings/undo` vs `/flows/{id}/undo`) against different revision tables. The UI decides scope based on focus.

**Interaction with flow revisions**: settings edits do *not* write a flow revision. The flow document references node settings by node id; node settings are versioned independently. This is the whole reason there are two tables — otherwise every slider drag would rewrite the whole flow.

---

## Retention & pruning

Per-flow cap: **200 revisions** default, configurable. Per-node settings cap: **100 revisions**. Prune runs:

- Opportunistically on write when over cap, in the same transaction.
- Nightly via a maintenance task for cleanup after flow/node deletions.

Pruning must **never** orphan a snapshot chain — if we delete revision K and K held a snapshot, the next surviving revision inherits a fresh snapshot. Concretely: pruning rewrites the oldest surviving revision to carry a full snapshot, then deletes anything older.

---

## HTTP surface (additions)

```
GET    /flows/{id}/revisions                  → list, paged, newest first
GET    /flows/{id}/revisions/{revId}          → materialised document at that revision
POST   /flows/{id}/undo                       → body: { expected_head }
POST   /flows/{id}/redo
POST   /flows/{id}/revert/{revId}

GET    /nodes/{id}/settings/revisions
GET    /nodes/{id}/settings/revisions/{revId}
POST   /nodes/{id}/settings/undo
POST   /nodes/{id}/settings/redo
POST   /nodes/{id}/settings/revert/{revId}
```

All mutating endpoints take `expected_head` for optimistic concurrency and return the new head id.

---

## Storage cost sanity check

Edge, typical flow: 50 nodes, 80 links, ~15 kB serialised.

- 200 revisions, avg patch 200 bytes, snapshot every 20 → 200·200 + 10·15_000 ≈ **190 kB per flow**.
- 20 flows on a gateway → ~4 MB. Comfortably within the 350 MB budget.

Node settings, typical: 1 kB payload, avg patch 80 bytes, snapshot every 20.

- 100 revisions · 80 + 5 · 1_000 ≈ **13 kB per node**.
- 500 nodes across a gateway → ~6 MB.

Cloud is unbounded by comparison and not a concern.

---

## Open questions

1. **Cross-device session continuity.** Should the redo cursor be persisted server-side (so closing the tab and reopening still lets you redo)? Lean: **no** — matches user expectation that redo is session-local. Revisit if users complain.
2. **"Who edited what" surfacing in UI.** The data is there (`author`, `summary`); whether we expose a full activity feed is a product call, not blocked by this design.
3. **Ext-authored settings migrations.** When a node kind bumps major and its old settings schema needs migration (see [VERSIONING.md](../design/VERSIONING.md)), historical revisions hold old-shape payloads. Materialising an old revision must run the kind's migration chain before handing it to the UI. Needs a hook on the kind registry.
4. **Compression.** Snapshots are JSON; worth zstd-compressing on edge? Probably yes once the feature is shipped and we see real sizes — not v1.

---

## Implementation sketch

Three phases, each independently shippable.

**Phase 1 — flow revisions (backend + API).** Table, endpoints, optimistic concurrency, snapshot cadence, pruning. No UI work; wire a thin "undo" button in the existing canvas that calls the endpoint.

**Phase 2 — node settings revisions.** Same pattern for the `node_setting_revisions` table. Property panel gains its own undo/redo scope.

**Phase 3 — history UI.** Revision timeline, diff view, revert button. Nice-to-have; not required for the core undo/redo UX to work.

Client-side command stack (the fast, in-memory one) is **complementary, not a substitute**: we still want it layered on top for sub-second UX so every keystroke doesn't round-trip. It flushes into a single DB revision on debounce / blur / explicit save. That's a UX detail, not part of this design.
