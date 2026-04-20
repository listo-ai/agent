# Node Mutation — the four-primitive rule

Status: design, pre-implementation. Keep this doc pinned before adding any new node-level API.

## Motivation

Every time a UI surface needs to "edit a node", the easy path is to carve a new REST endpoint: `/node/rename`, `/node/move`, `/node/retitle`, `/node/tag`. Do that a few times and the surface is unmaintainable — N endpoints for N kinds of mutation, each with its own DTO, its own handler, its own client method, its own tests.

This document fixes the set of things a node can be mutated by, and pins the layers that implement those mutations.

## The rule — four primitives

A node is mutable in exactly four ways. Anything else is a composition of these.

| Primitive | HTTP | Purpose | Scope |
|---|---|---|---|
| **Create** | `POST /api/v1/nodes` | Add a new node under a parent. | Existed. |
| **Delete** | `DELETE /api/v1/node` | Remove a node (with cascade policy). | Existed. |
| **Write slot** | `POST /api/v1/slots` | Change any content/config/runtime value. Tags, presentation, layout, settings, anything schematic. | Existed. |
| **Patch structure** | `PATCH /api/v1/node` | Change the node's identity: `name`, `parent`. One endpoint, sparse body. | **New.** |

No other node-level endpoints. If someone is about to add a fifth, it's either a slot write in disguise, a new kind, or a query — not a new mutation surface.

### Why structure is separate from slots

A slot write changes a JSON value the kind author has declared. A structure change rewrites the node's path, every descendant's path, every link's `SlotRef.node`, and the SSE subscription keys. That's not a value — it's a topology operation. It deserves its own primitive, not a hidden side effect behind a slot write.

### Why one patch endpoint instead of rename + move

- Same underlying work: compute (old → new) path, repath subtree, emit events.
- Sparse fields compose without multiplying routes: `{ name }`, `{ parent }`, or `{ name, parent }` all go through one handler.
- Future structural fields (e.g. `parent` accepting a subtree reparent, or a `name` that's validated against a kind-declared pattern) extend the same body, not a new URL.

## Contract — `NodePatch`

```rust
/// Sparse patch for a node's identity. All fields optional; at least one
/// must be present. An empty patch is rejected at the transport layer so
/// it never reaches the domain.
pub struct NodePatch {
    /// New last-segment name. Must be non-empty and contain no `/`.
    pub name: Option<String>,
    /// New parent path. Must exist, must accept this node's kind under
    /// the kind registry's containment rules.
    pub parent: Option<NodePath>,
}
```

Semantics:

- `name` only → rename (path stays under same parent).
- `parent` only → move (node keeps its name, lands under new parent).
- Both → rename-and-move, single atomic operation.
- Neither → rejected in the handler as `400 Bad Request`. The domain never sees an empty patch.

Validation:

1. Target node must exist.
2. `name` — non-empty, no `/`, normalized by the same rule as `create_child`.
3. `parent` — must resolve to an existing node, must accept the target's kind under containment rules (same check as `create_child`).
4. Final `(parent, name)` pair must not collide with an existing sibling.
5. Root cannot be mutated — rejected with `InvalidNodeName` (the same error `create_child` uses for `/` attempts).

Atomicity:

- The whole operation happens under one write lock.
- Repo-first: every affected `NodeRecord` is persisted with its new path before the in-memory maps are touched. A repo failure leaves memory untouched.
- Subtree is fully re-pathed; links that cross the subtree boundary stay untouched (their `SlotRef.node` is by `NodeId`, not path).

Events:

- One `NodeRenamed { id, old_path, new_path }` per affected node (self + every descendant). Emitted after memory is consistent.
- No new event kind is added. `NodeRenamed` already covers both pure renames and subtree repaths caused by moves; consumers treat it the same way.

## Implementation layout

Layers per CODE-LAYOUT.md. No cross-layer shortcuts.

### Domain (`crates/graph/`)

New module `crates/graph/src/patch.rs`. Owns all identity-mutation logic. Exposes:

```rust
// Public, sparse patch — re-exported from lib.rs.
pub struct NodePatch { pub name: Option<String>, pub parent: Option<NodePath> }

// Focused helpers — each does one thing, each ≤50 lines.
impl GraphStore {
    pub fn rename_node(&self, path: &NodePath, new_name: &str)    -> Result<NodePath, GraphError>;
    pub fn move_node  (&self, path: &NodePath, new_parent: &NodePath) -> Result<NodePath, GraphError>;
    pub fn patch_node (&self, path: &NodePath, patch: NodePatch)  -> Result<NodePath, GraphError>;
}
```

Internal helpers live in the same module as private functions:

- `plan_repath(inner, id, old_path, new_path) -> Vec<(NodeId, NodePath, NodePath)>`
- `apply_repath(inner, repo, plan) -> Result<(), GraphError>` — repo-first commit
- `validate_name(&str) -> Result<(), GraphError>`

Why this module layout:

- `store.rs` is already over the 400-line cap; this keeps new code out of it.
- The three public methods are composition-friendly — `patch_node` just dispatches to `rename_node` and/or `move_node` via the shared helpers. Transport handlers, future CLI subcommands, and integration tests can all reach for the narrowest verb that fits their call site, not the generic `patch_node` every time.
- No new error variants required — `GraphError::{NotFound, InvalidNodeName, NameCollision, PlacementRejected}` already cover every failure mode.

No panics in this module: every fallible lookup returns `GraphError::NotFound`, no `expect`/`unwrap` in library code.

### Transport (`crates/transport-rest/`)

One handler, one route, sparse DTO:

```rust
// DTO — lives in the transport crate, not leaked into domain.
#[derive(Deserialize)]
struct PatchNodeReq { name: Option<String>, parent: Option<String> }

#[derive(Serialize)]
struct PatchNodeResp { old_path: String, new_path: String }

async fn patch_node(
    State(s): State<AppState>,
    Query(q): Query<PathQuery>,
    Json(req): Json<PatchNodeReq>,
) -> Result<Json<PatchNodeResp>, ApiError> { /* 1. extract  2. validate non-empty  3. call domain  4. map */ }
```

Route:

```rust
.route("/api/v1/node", get(get_node).patch(patch_node).delete(delete_node))
```

The handler is thin — extract, call `graph.patch_node(path, patch)`, map. No business logic in the transport layer.

### Client (`@sys/agent-client`)

One method, mirrors the DTO:

```ts
interface NodesApi {
  patchNode(path: string, patch: { name?: string; parent?: string }): Promise<{ old_path: string; new_path: string }>;
}
```

The TS client does not ship separate `renameNode` / `moveNode` methods. UIs that only want to rename call `patchNode(path, { name })`; composition is the caller's job, not the client's.

### Frontend

- A `RenamePageDialog` that calls `patchNode(path, { name })`.
- A `MovePageDialog` (later) that calls `patchNode(path, { parent })`.
- No per-action API. All node structural edits converge on the same client method.

## What this does NOT cover

- **Slot content** — still `POST /api/v1/slots`. Tags, layout, presentation, settings. If you can describe it in the kind's slot schema, it's a slot write.
- **Kind changes** — a node can't change its kind. If the user "wants a different kind", that's delete + create, not a patch.
- **Bulk operations** — a future concern; out of scope for this doc.
- **Undo/redo** — handled by the revision machinery in [UNDO-REDO.md](./UNDO-REDO.md). A patch emits `NodeRenamed` events like any other mutation, which the revision log already captures.

## Decision log

1. **Patch over rename/move**: one endpoint, one handler, one client method. Scales with future structural fields without growing the surface.
2. **Sparse `NodePatch`, not a tagged command**: sparse body is the REST-native shape and matches how presentation updates already work in [TAGS-PRESENTATION-QUERY.md](../sessions/TAGS-PRESENTATION-QUERY.md).
3. **Keep three domain verbs (`rename`, `move`, `patch`)** instead of only `patch`: call sites that know they only rename shouldn't have to express that as a sparse struct. Reusable from CLI, tests, future MCP tools.
4. **One `NodeRenamed` event per affected node**, not a single "subtree moved" envelope. Consumers already handle it; no new event kind earns its place here.
5. **No new `GraphError` variants**: the existing set covers every failure. Resist the urge to add `Cannot rename root` — it falls out of `InvalidNodeName` on an empty `parent()`.

## One-line summary

**A node is mutated by exactly four primitives (create, delete, write-slot, patch-structure); `PATCH /api/v1/node` with sparse `NodePatch { name?, parent? }` is the only structural edit and composes rename and move into one surface.**
