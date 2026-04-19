# Reactive UI — Implementation

Wire every Studio surface that reads the graph — the **flows sidebar**, the **flow canvas**, the **property panel** — to the same live cache so they stay 100% in sync across clients. Lazy-load on demand, CRUD from any surface (CLI, another Studio tab, a plugin, a fleet command, the engine itself) appears everywhere within one round-trip, and the local cache reconciles deterministically after network interruptions.

> **Status: complete.** All six stages (1a–1f) are shipped. This doc reflects the as-built design.

This covers two rendering surfaces, one mechanism:

- **Sidebar tree** ([`frontend/src/components/layout/Sidebar.tsx`](../../frontend/src/components/layout/Sidebar.tsx)) — hierarchical: `flows → flow → node → …`, lazily expanded to any depth.
- **Flow canvas** ([`frontend/src/pages/flows/FlowCanvas.tsx`](../../frontend/src/pages/flows/FlowCanvas.tsx)) — one flow's subtree rendered as the node/link graph; property panel edits round-trip back through the same events.

Both read from the same path-keyed client store and subscribe to the same SSE stream. Same subscribe-before-fetch contract, same event handlers, same reconnect semantics. A slot edit in the property panel, a fleet-command-driven node creation, and a CLI `nodes create` all surface identically in every open client.

This doc is the *shape*: what to land, what's explicitly out, and the two small backend additions that remove a class of state-drift bugs before they're written.

Authoritative references: [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md) for why the tree *is* the graph, [NEW-API.md](../design/NEW-API.md) for the five-touchpoint PR rule, [FLEET-TRANSPORT.md § "Studio's transport abstraction"](../design/FLEET-TRANSPORT.md) for the cross-agent Studio story this composes into, [FLOW-UI.md](FLOW-UI.md) for flow-specific UI concerns this doc stays out of, [UNDO-REDO.md](UNDO-REDO.md) for the history model that sits on top of this reactive layer.

---

## Goals

- **One source of truth across both surfaces.** Sidebar tree *and* flow canvas derive from the same path-keyed node cache + link cache. No parallel state in ten components, no "flow view's nodes" vs "sidebar view's nodes".
- **Lazy everywhere.** The sidebar fetches a branch only on expand. The canvas fetches only the current flow's subtree. No full-graph snapshot on page load.
- **Live.** Any CRUD mutation — property panel here, CLI there, fleet command, engine behaviour, another Studio tab — appears in every open view within one round-trip.
- **Reconnect-correct.** After SSE drop, every view catches up deterministically, not "mostly". No stale nodes on the canvas after a coffee-break network hiccup.
- **Multi-client safe.** Two users editing different flows share the same SSE stream; one user's navigation doesn't drop the other's updates.
- **Backend-agnostic.** Same bindings work against a local standalone agent and a cloud-mediated remote edge — the event shape and cache mechanics don't change.

## Non-goals

- **CRDT / operational transform collaborative editing.** Property-panel edits are last-writer-wins gated by slot `generation`. One editor at a time per node; concurrent edits surface as "someone changed this, refresh."
- **Server-side session state.** The server is stateless per connection. Which branches a user has expanded, which flow they've open, which node they've selected — all frontend-only.
- **Undo/redo.** Separate feature — see [UNDO-REDO.md](UNDO-REDO.md). This doc provides the live-state layer that undo/redo sits on top of; it doesn't own the history model.
- **Drag-and-drop reparenting in this landing.** Reparent API (`POST /api/v1/node/move`) is a follow-up; v1 renders + watches, doesn't restructure.
- **Flow canvas authoring semantics.** Which nodes a flow draws, how connections are styled, which slots surface as input/output ports — all owned by [FLOW-UI.md](FLOW-UI.md). This doc owns the data plumbing only.
- **Custom per-user tree views.** Everyone sees the same graph shape; filters and searches are view-only overlays computed client-side.
- **Virtualised rendering for huge trees.** Scope says "the graph gets big enough that typical branches are tens of nodes, not thousands". If a single parent has 1000+ children we scrollify that list, don't add windowing.

---

## Two surfaces, one store

Both views read from the same `GraphStore` (client-side), subscribe to the same SSE connection, react to the same events. They differ only in *which slice of the cache they select* and *which user actions they originate*.

| | Sidebar | Flow canvas |
|---|---|---|
| File | [`Sidebar.tsx`](../../frontend/src/components/layout/Sidebar.tsx) | [`FlowCanvas.tsx`](../../frontend/src/pages/flows/FlowCanvas.tsx) + [`FlowPropertyPanel.tsx`](../../frontend/src/pages/flows/FlowPropertyPanel.tsx) |
| Selection | children of each expanded path, recursively to any depth | nodes with `parent_path` inside active flow + links with both endpoints in that subtree |
| Load trigger | User clicks a chevron (or sidebar mounts, which auto-expands `/`) | User opens a flow (route param change) |
| Writes back | None in v1 (display only) | Property-panel slot edits, node add, node delete, link add/remove |
| Expected cache hit rate on reload | Low — tree is mostly collapsed | High — a flow's subtree usually fits; canvas reopens are free |

**Shared primitives:**
- Same `NodeSnapshot` cache keyed by path. The canvas's nodes are a subset of the sidebar's nodes.
- Same `GraphEvent` handlers mutate the cache. A property-panel slot edit fires `SlotChanged`; the sidebar's row highlighting (if we ever add it) reacts for free.
- Same subscribe-before-fetch startup, same reconnect semantics, same `lastSeq` cursor.

**What the flow canvas adds on top** (but still inside the same store):
- A **link cache** keyed by link id — link events (`LinkAdded` / `LinkRemoved` / `LinkBroken`) mutate it idempotently, same pattern.
- A **selection state** — which node is open in the property panel. Purely frontend, shared between canvas and panel via the store so deep-links (e.g. `?flow=/demo&node=/demo/counter`) work.
- **Optimistic slot writes.** The panel fires `POST /api/v1/slots` and records `{ expected_gen, value }` in the store's `pending` map. Either the `SlotChanged` SSE event or the HTTP response resolves the pending entry — **whichever arrives first**. Both carry the same `generation`, so late arrivals are no-ops by the standard stale-check. A newer-than-expected generation means a concurrent remote write won → revert. See § "Optimistic slot writes" for the full table.

These all live inside the `GraphStore` so the canvas doesn't grow its own parallel state that could drift from the sidebar.

## Scope rails

**In:**

| In |
|---|
| `has_children: bool` on `NodeDto` — computed server-side; lets chevrons render without a second round-trip per node |
| Event sequence numbers — every `GraphEvent` gains `seq: u64`, monotonic per agent, serialised on the SSE stream |
| `GET /api/v1/events?since=<seq>` — resumable SSE; client reconnects, server replays from the given cursor |
| Server-side event ring buffer — N most-recent events retained per agent (default 1024) so reconnect replay is bounded |
| `last_seq` header on the SSE connection so the client knows the current cursor on first connect |
| Frontend node cache (path-keyed) + link cache (id-keyed) + expanded-paths set + canvas-selection state — one store, separated cleanly |
| Subscribe-before-fetch protocol: SSE opens first, events queue, then lazy fetches merge, queue drains |
| Per-event cache mutations covering every `GraphEvent` variant — create, remove, rename (with subtree path rewrite), slot change (with generation guard), lifecycle transition, link add/remove/broken |
| Sidebar binding: `useNodeChildren(path)` reads cached `parent_path == path` entries, triggers lazy fetch on first expand |
| Flow canvas binding: `useFlowSubtree(flowPath)` reads cached `parent_path starts-with flowPath` + links with both endpoints inside, triggers lazy fetch of the whole subtree on flow open |
| Property panel: optimistic slot write with pending-generation tracking; reconciles against the returned `SlotChanged` event; reverts on conflict |
| Reconnect flow: if `lastSeq` is still in the ring → replay catches up; if not → mark every expanded branch + every open flow stale and refetch |
| One shared SSE connection per `AgentClient`, fanned out to every subscriber in the Studio process (sidebar, canvas, panel, anything else later) |
| Fixture tests for the new wire fields per [NEW-API.md](../design/NEW-API.md) rules |

**Out (explicitly deferred):**

| Out | Why / when |
|---|---|
| Operational transform / CRDT merge | Concurrent-edit story stays "generation guard + refresh on conflict". Real collaborative editing is a stage-N decision, not a reactive-UI concern. |
| Partial-path subscriptions (subscribe to `/station/*` only) | SSE sends all events, filtered client-side. Per-path subject filtering becomes meaningful when we wire the fleet-transport equivalent, not before. |
| Server-side expanded-state sync between a user's devices | Per-device, per-session. Pinning to a user's profile is a Stage-N "preferences" feature. |
| Drag-and-drop reparenting | Needs `GraphStore::move_as` and careful link-rewriting. Separate landing. |
| Search indexing beyond `filter=…` | Text search on slot values / kind metadata belongs in a dedicated search surface, not the sidebar or canvas. |
| Offline queueing of mutations in the browser | Studio today assumes a live agent. Offline-mutate-then-sync is a separate SW-level feature. |

## Wire additions

Both are small and mechanical — the five NEW-API.md touchpoints apply to each.

### 1. `has_children: bool` on `NodeDto`

```rust
// crates/transport-rest/src/routes.rs
pub(crate) struct NodeDto {
    id: String,
    kind: String,
    path: String,
    parent_path: Option<String>,
    parent_id: Option<String>,
    has_children: bool,     // ← new
    lifecycle: Lifecycle,
    slots: Vec<SlotDto>,
}
```

Computed from `GraphStore` — every store knows its own child count, so this is O(1) per node. The tree renders expand chevrons without ever issuing a speculative "does this have children?" query.

### 2. Event sequence numbers + resumable SSE

`seq` and `ts` are **transport-layer fields**, not domain fields. The domain `GraphEvent` stays clean. The SSE transport wraps every event in a `SequencedEvent` before writing to the wire:

```rust
// crates/transport-rest/src/event.rs
pub struct SequencedEvent {
    pub seq: u64,   // monotonic per agent lifetime; AtomicU64 fetch-add on publish
    pub ts: u64,    // ms since Unix epoch (u64, not DateTime — avoids chrono dep)
    #[serde(flatten)]
    pub event: GraphEvent,
}

// First frame on every new SSE connection — tells the client where "now" is
// before it has consumed any events.
pub struct HelloFrame {
    pub event: &'static str, // always "hello"
    pub seq: u64,
}
```

The `AgentSink` in `transport-rest` holds an `AtomicU64` counter and an `EventRing`. On every `emit()` it assigns `seq`/`ts`, pushes into the ring, and broadcasts the `SequencedEvent` to all live SSE connections.

The domain `GraphEvent` itself has **no** `seq` or `ts` fields — that separation keeps the graph crate independent of transport concerns.

Server-side behaviour of `GET /api/v1/events?since=<n>`:
- If `n` is still in the ring → flush those events first, then stream live.
- If `n` is below the ring's lower bound → `409 Conflict + { error: "cursor_too_old", available_from: <m> }`. Client refetches everything it cares about and resubscribes from current.
- Every connection's first frame is `hello { event: "hello", seq: N }` so clients know where "now" is even if the ring is empty.
- Default ring capacity: **1024** (`DEFAULT_RING_CAPACITY` in `transport-rest/src/ring.rs`).

This is the single-biggest reliability win for the frontend. Without it, SSE reconnect silently drops events and caches drift. With it, "was I up to date?" has a deterministic answer.

## Frontend architecture

```
┌──────────────────────────────────────────────────────────┐
│                       Studio app                          │
│                                                          │
│   ┌──────────┐  ┌──────────┐  ┌──────────────┐           │
│   │ Sidebar  │  │  Canvas  │  │  Property    │           │
│   │   tree   │  │   view   │  │    panel     │  …        │
│   └─────┬────┘  └─────┬────┘  └──────┬───────┘           │
│         │             │              │                    │
│         └─────────────┼──────────────┘                    │
│                       ▼                                   │
│   ┌────────────────────────────────────────────────┐     │
│   │              GraphStore (client)               │     │
│   │                                                │     │
│   │   nodes:       Map<path, NodeSnapshot>         │     │
│   │   links:       Map<id,   Link>                 │     │
│   │   expanded:    Set<path>         // sidebar    │     │
│   │   openFlow:    path | null       // canvas     │     │
│   │   selection:   path | null       // panel      │     │
│   │   pending:     Map<path+slot, PendingWrite>     │     │
│   │   conflicts:   Map<path+slot, unknown>          │     │
│   │   lastSeq:     number                          │     │
│   │   loadingPaths: Set<path>                      │     │
│   │                                                │     │
│   │   actions:   expand(path) / collapse(path)     │     │
│   │              setOpenFlow(path) / setSelection  │     │
│   │              writeSlot(path, slot, value)      │     │
│   │              applyEvent(GraphEvent)            │     │
│   │              reconcile() on cursor_too_old     │     │
│   └──────┬─────────────────────────┬───────────────┘     │
│          │                         │                      │
│    fetch children             SSE events                  │
│          │                         │                      │
└──────────┼─────────────────────────┼──────────────────────┘
           ▼                         ▼
   ┌────────────────────────────────────────────┐
   │       AgentClient (@acme/agent-client)     │
   │  - nodes.getNodesPage({ filter: … })       │
   │  - events.subscribe({ sinceSeq, onOpen })  │
   └────────────────────────────────────────────┘
```

One store per `AgentClient` instance, created by `createGraphStore(client)` in `frontend/src/store/graph-store.ts`. The store is mounted at the app root by `GraphStoreProvider` (`frontend/src/providers/graph-store.tsx`) and exposed via `GraphStoreContext`. Components never talk to `AgentClient` directly — they subscribe to store selectors via `useStore(graphStore, selector)`.

### Provider boot sequence

`GraphStoreProvider` resolves `agentPromise` (the singleton `AgentClient`), then calls `createGraphStore(client)` and stores the result in React state. **Important:** Zustand stores are callable functions (`UseBoundStore`), so you must use `setStore(() => created)` — not `setStore(created)` — or React treats it as a functional state-update and calls the store as a reducer outside a component.

### Startup protocol (the ordering guarantee)

```
1. openSSE(sinceSeq = null)
      → server's first frame is `hello { seq: N }`
      → set lastSeq = N
2. listen & queue events (don't apply yet)
3. for each expanded path (initial is usually empty or ["/"]):
      list children where parent_path == <path>
      insert into cache
4. drain queued events, applying cache mutations
5. subsequent events apply directly as they arrive
```

Skip step 1 and you race: a create fires between your list and your subscribe → cache never sees it. Do step 3 before step 1 and the same race bites the other direction.

### Per-event cache mutation

A node is considered *visible* when any of these is true: its `parent_path` is in `expanded` (sidebar cares), or the path starts-with `openFlow` (canvas cares). Visibility drives the decision to fetch; cache parent-bookkeeping runs regardless.

| Event | Effect on cache |
|---|---|
| `NodeCreated{path,parent_path,kind,...}` | **Always:** if `cache[parent_path]` exists, flip its `has_children` to `true` (fixes stale chevrons on previously-leaf parents). **If visible** (parent in `expanded` *or* `path` starts-with `openFlow`): fetch the full `NodeSnapshot` for `path` and insert. Event doesn't carry slots, so the fetch is mandatory when we need the row's detail. See § "Batching NodeCreated fetches" for the debounce rule. |
| `NodeRemoved{path,parent_path}` | Delete `path` and every cached descendant (keys starting with `path + "/"`). Collapse if expanded. **If the parent had no other children left**, flip `cache[parent_path].has_children` to `false`. |
| `NodeRenamed{old,new}` | Rewrite every cached path that starts with `old` → `new`. Update `expanded`, `openFlow`, and `selection` the same way. Link endpoints carrying the old path get rewritten too. |
| `SlotChanged{path,slot,value,generation}` | If `cache[path]` exists and `generation > cached.slot.generation` → update in place; else ignore as stale. See § "Optimistic slot writes" for the ordering rule with pending local writes. |
| `LifecycleTransition{path,to}` | Update `cache[path].lifecycle`; no-op if path not cached. |
| `LinkAdded / LinkRemoved / LinkBroken` | Mutate the id-keyed link cache. Sidebar ignores (doesn't render links); flow canvas redraws affected edges. |

Every mutation is idempotent: applying an event twice yields the same state as applying it once. This is what makes the reconnect-replay safe.

### Batching `NodeCreated` fetches

A single bulk operation (engine seeds 30 nodes, plugin install creates a subtree, a `POST /seed`) fires 30 individual `NodeCreated` events, each of which — under the rule above — would trigger its own `GET /api/v1/node?path=<…>` fetch. That's a thundering herd against the same agent for data we already know we want.

**Rule:** the store coalesces pending NodeCreated fetches per event tick with a 25 ms debounce, then issues **one** `GET /api/v1/nodes?filter=id=in=<comma-separated ids>` request. Implementation:

1. NodeCreated event arrives. Push `{id, path, parent_path}` into a pending-fetch queue (keyed by id, last-write-wins on dupes).
2. If no timer is set, set a 25 ms `setTimeout` to drain the queue.
3. On drain: issue one batched list, merge results into cache, flip parent `has_children` flags in a single store transaction.

This requires the existing `id` query field to support the `in` operator ([query schema](../../crates/transport-rest/src/routes.rs) already has `Eq`/`Ne`/`Prefix`; add `In`). That's a small server-side addition landed alongside 1a.

**Upper bound on batch size:** 200 per request. Above that, split. Keeps URL length sane and reply size bounded.

**What this is not:** a general request coalescer. Other event types fire at most one fetch (rename → rewrite, remove → no fetch, slot changed → no fetch). Only `NodeCreated` needs batching, because only `NodeCreated` carries a forced fetch.

### Optimistic slot writes

The property panel writes a slot, visibly updates the cached value immediately, and expects two signals to arrive in an undefined order:

1. The `SlotChanged` SSE event carrying the new `generation`.
2. The HTTP `POST /api/v1/slots` response carrying the same new `generation`.

SSE is push, HTTP is pull, both traverse different TCP streams — **either can land first**, and the store must behave identically regardless.

**The store tracks, per `(path, slot)`, a `pending` entry:** `{ expected_gen: u64, value: T, started_at: timestamp }`. Set at write time, cleared when either signal arrives with a matching `generation`. The rule that locks the ordering ambiguity is simple:

> **First signal to arrive wins. Subsequent signals with the same `generation` are no-ops. Signals with a newer `generation` than the pending `expected_gen` mean the server moved on (conflict) — revert.**

Concretely:

| Scenario | What happens |
|---|---|
| Happy path, event first | Event applies to cache, clears pending. HTTP response arrives, generation matches, no-op. |
| Happy path, HTTP first | HTTP response applies to cache (same code path as event), clears pending. SSE event arrives with same `generation`, no-op per the standard stale-check. |
| Conflict (someone else wrote) | Whichever arrives first carries a `generation > expected_gen`. Pending reverts (cache reflects server's value, not ours), panel surfaces "replaced by remote change" and re-reads. |
| HTTP error (4xx/5xx) | Clear pending, revert cache to pre-write value, surface error. No SSE event comes for a write that didn't land. |
| Timeout with neither signal | After a bound (say 5 s), clear pending, revert, surface "couldn't confirm write — reload". Rare but must be handled to avoid permanent optimistic limbo. |

**Why this works:** both signals carry the same `generation`, which is the only source of truth. The rule "first wins, second is a no-op" means out-of-order delivery is correct by construction. The existing per-event generation guard (`ignore if generation <= cached.generation`) covers the second-arrival case without a special branch.

**What the store must *not* do:** treat the HTTP response as authoritative *while ignoring* the SSE event (duplicates), or treat the SSE event as authoritative *while ignoring* the HTTP response (misses server-side errors that never emit an event). Both channels converge on generation; neither is privileged.

### Reconnect flow

The store exposes a unified `stalePaths` concept: the union of every expanded path (sidebar's concern) and the open flow path if any (canvas's concern). Reconnect reconciliation iterates over this single set — no chance of forgetting a surface.

```
stalePaths = expanded ∪ (openFlow ? { openFlow } : ∅)

on SSE close:
   ...reconnect backoff...
   openSSE(sinceSeq = lastSeq)

if server replies `409 cursor_too_old`:
   for path in stalePaths:
       if path is in expanded:
           refetch children where parent_path == <path>
       if path == openFlow:
           refetch subtree where path=prefix=<openFlow>
           refetch links whose endpoints lie inside <openFlow>
   openSSE(sinceSeq = null)
   // treat as cold start; full reconciliation
```

Most reconnects are fast and within the ring; rare long outages fall back to refetching-every-stale-path. Both paths converge to the same steady state, and neither the sidebar nor the canvas can be silently stale after the dust settles.

### The hooks

Concrete surfaces each view uses (all in `frontend/src/store/graph-hooks.ts`):

```ts
// One store, one SSE connection per client.
// Created by createGraphStore(client) in graph-store.ts.
// Consumed via GraphStoreContext; use useGraphStoreOptional() in pages
// that render before the agent handshake resolves.

// ── Sidebar tree (Sidebar.tsx) ───────────────────────────
// childrenOf is a Map<parentPath, NodeSnapshot[]> built from the
// full node cache — so any depth is covered with one useMemo.
const nodeMap = useStore(graphStore, (s) => s.nodes);
// expand() triggers a lazy fetch of parent_path==path children
// collapse() is cosmetic; cache stays
const expanded = useStore(graphStore, (s) => s.expanded);
const loading  = useStore(graphStore, (s) => s.loadingPaths);

// ── Flows list page (FlowsListPage.tsx) ──────────────────
// Same nodeMap + linkMap from GraphStore — reactive to SSE events
// without manual invalidateQueries calls.

// ── Flow canvas (useFlowPageData.ts) ─────────────────────
const nodeMap = useStore(activeStore, (s) => s.nodes);
const linkMap = useStore(activeStore, (s) => s.links);
// setOpenFlow() triggers subtree + link fetch; wired via useEffect

// ── Property panel ───────────────────────────────────────
const { value, isPending, conflict, write } = useSlotWrite(store, path, "value");
write(42); // optimistic; resolves on matching SlotChanged, reverts on conflict
```

**Selector stability rule:** selectors must return stable references or primitives. Selectors that return `new Map(...)` or `[...values()]` inline cause Zustand to see a new reference on every render → infinite re-render loop. The pattern used throughout is:

```ts
// ✅ Select the stable Map reference; spread in useMemo
const nodeMap = useStore(store, (s) => s.nodes);          // stable ref
const nodes = useMemo(() => [...nodeMap.values()], [nodeMap]);

// ❌ Inline spread — new array every call, infinite loop
const nodes = useStore(store, (s) => [...s.nodes.values()]);
```

### State invariants the store enforces

1. **Cache monotonicity on slots.** A slot value only ever moves forward in `generation`. Out-of-order SSE frames → ignored.
2. **Subtree consistency on rename.** Either the whole subtree's keys + every reference to them (`expanded`, `openFlow`, `selection`, link endpoints) are rewritten, or none are. Wrapped in a single store transaction.
3. **Parent `has_children` matches cache reality.** After every create/remove mutation, if the parent is cached, its `has_children` equals `{ n ∈ cache | n.parent_path === parent.path }.size > 0`. Enforced by the mutation handlers, not a background reconciler.
4. **Expanded ⊆ cached-parent.** If `expanded` contains `/a/b`, then `/a/b` is in the cache. Collapse a path doesn't violate; expand without the parent cached triggers a fetch first.
5. **Open flow ⊆ cached.** If `openFlow === /x`, then `/x` is in the cache.
6. **At most one pending optimistic write per `(path, slot)`.** A second `writeSlot` on the same slot while one is pending either queues (if we add that later) or — today — rejects synchronously. Keeps generation accounting linear.
7. **`lastSeq` is monotonic.** Only updated on successful event apply, never on retry.

## Staged landing

All stages are complete.

| Stage | What | Status |
|---|---|---|
| **1a — `has_children` + `id=in=` operator** | `has_children: bool` on `NodeDto`. `In` operator added to query schema for the `id` field. Mirrored in Rust + TS clients, fixtures updated. | ✅ Done |
| **1b — Event `seq` + `ts` + ring buffer** | Transport-layer `SequencedEvent { seq, ts, #[serde(flatten)] event }` wraps every `GraphEvent` on the wire. `AgentSink` assigns seq via `AtomicU64`, pushes into `EventRing` (capacity 1024). `HelloFrame` sent as first SSE frame. `AppState` holds both the broadcast sender and the ring. | ✅ Done |
| **1c — `?since=` resumable SSE + `hello` frame** | `stream_events` handler accepts `?since=<seq>`. First frame is always `hello { seq }`. Replay from ring on valid cursor; `409 cursor_too_old` with `available_from` when cursor is below ring's lower bound. TS client updated: `subscribe({ sinceSeq, onOpen, onCursorTooOld })`. | ✅ Done |
| **1d — Frontend `GraphStore`** | `createGraphStore(client)` in `graph-store.ts`. Path-keyed node cache, id-keyed link cache, `expanded`, `openFlow`, `selection`, `pending`/`conflicts` optimistic maps, `loadingPaths`, `lastSeq`. Subscribe-before-fetch startup. Batched-NodeCreated-fetch (25 ms debounce, max 200/request). First-signal-wins optimistic writes with 5 s revert timeout. `reconcile()` for `cursor_too_old`. `destroy()` closes SSE. | ✅ Done |
| **1e — Sidebar bindings** | `Sidebar.tsx` rewritten as a recursive `TreeNode` component tree. `childrenOf` Map indexes all cached nodes by `parent_path` so any depth is covered. On mount, auto-expands `/` to load top-level flows. Chevron click calls `graphStore.expand(path)` for lazy fetch. Flows list page (`FlowsListPage.tsx`) migrated from `useNodes()`/`useLinks()` TanStack Query hooks to `useStore(graphStore, …)`. | ✅ Done |
| **1f — Flow canvas + property panel bindings** | `useFlowPageData.ts` migrated from TanStack Query to GraphStore. `activeStore = graphStore ?? emptyFlowStore` pattern for pre-connect renders. `setOpenFlow()` called via `useEffect` to trigger subtree + link fetch. Synthetic `nodesQuery`/`linksQuery` objects keep `FlowsPage` loading/error guards working without changes. | ✅ Done |

**Key implementation notes:**

- `seq`/`ts` are **not** on `GraphEvent` — they live on the transport-layer `SequencedEvent` wrapper. The domain crate stays independent of transport metadata.
- The Zustand store is a function (`UseBoundStore`). `GraphStoreProvider` uses `setStore(() => created)` — not `setStore(created)` — to prevent React from calling it as a functional state updater.
- `useStore` selectors that return `new Array` or spread a `Map` inline cause infinite re-render loops (Zustand uses `Object.is`). Always select the stable `Map`/`Set` reference and spread inside `useMemo`.
- `FlowSidebar.tsx` (the kind palette inside the flow editor) is **not** the tree sidebar. The global nav sidebar is `Sidebar.tsx` in `components/layout/`.

## Testing

| Category | What it covers |
|---|---|
| Server unit | `has_children` reflects live store state; `seq` monotonic across many writes; ring wraps correctly; `?since=<below-ring>` returns 409 |
| SSE contract | Fresh connect receives `hello { seq }`; reconnect with valid cursor sees exactly the missed events, no duplicates |
| Client store | Each `GraphEvent` variant idempotent; rename rewrites subtree keys including `expanded`; slot generation guard drops stale updates |
| Client reconnect | Happy path: replay within ring. Cold path: `cursor_too_old` → every expanded branch refetched once |
| End-to-end | Two `AgentClient` instances against one agent: one creates a node, the other's store sees it within the round-trip — sidebar row appears, canvas node materialises if the new node lies inside an open flow |
| Property-panel optimistic write | Fire slot write, assert local cache shows the new value immediately, assert incoming `SlotChanged` event with matching generation is a no-op; assert mismatched generation triggers revert |
| Fixture gate | `has_children` appears in nodes-list fixtures per NEW-API.md |

## Decisions locked

1. **Path-keyed, single-cache, single-source-of-truth.** Never derive structure from UI component state.
2. **SSE is the only live channel.** Any future richer channel (WS, fleet-bus) must implement the same subscribe-before-fetch contract and the same idempotent event shape.
3. **`seq` + ring buffer is the reconnect primitive.** No "reload the page" prompts on a transient network blip.
4. **Frontend owns all per-session state** — expanded branches, open flow, selected node. The server is stateless per session.
5. **Generation guards slot updates.** Monotonic per slot; out-of-order events are dropped, never applied.
6. **First signal wins for optimistic writes.** Whichever of the `SlotChanged` event or the HTTP response arrives first applies to the cache; the second is a no-op (same generation) or a conflict (newer generation → revert). Neither channel is privileged.
7. **Visibility = expanded ∪ openFlow.** NodeCreated fetches the new node when any open view cares about it, not only when the sidebar has the parent expanded.
8. **Parent bookkeeping on create/remove.** `has_children` flips on the parent in response to child create/remove events, without waiting for a refetch.
9. **NodeCreated fetches are batched** with a 25 ms debounce, issued as one `?filter=id=in=…` list. Scales through bulk-spawn bursts without a request storm.
10. **One SSE connection per `AgentClient`.** Shared across all subscribers. Components never open their own.
11. **Collapse doesn't evict.** Collapse is cosmetic; the cache lives until the store explicitly invalidates.
12. **`seq` + `ts` are transport-layer fields.** They live on `SequencedEvent`, not on `GraphEvent`. The graph domain crate is independent of transport metadata.
13. **Selector stability is the caller's responsibility.** `useStore` uses `Object.is` for snapshot diffing. Selectors that allocate (spread, map, filter) must not be written inline; extract the stable reference and derive in `useMemo`.
14. **Stores are functions; state setters must account for that.** When storing a Zustand store in React state, use `setState(() => store)`, not `setState(store)`, to avoid React's functional-update branch.

## Open questions

- **Ring capacity default.** 1024 events covers seconds to minutes of activity depending on edge traffic. Probably fine; revisit after we have one noisy real deployment.
- **Rename vs move.** Today rename is name-only; a true *move* (reparent) is a later landing. Events today cover rename; move will need its own event kind or an expanded `NodeRenamed { old, new }` carrying subtree count.
- **Optimistic write timeout (5 s).** Arbitrary. Worth tuning once we see real edge latency distributions; cloud-mediated remote edges may need more.
- **Sidebar virtualisation.** Currently scrollifies wide lists. If a single branch has hundreds of nodes, windowing is the follow-up (see non-goals).
- **Property-panel conflict UX.** The store reverts and records the conflict in `conflicts: Map<path+slot, unknown>`. Surfacing a "replaced by remote change" banner in the panel UI is not yet wired.

## One-line summary

**Sidebar, flow canvas, and property panel all read from one path-keyed client cache fed by a subscribe-before-fetch SSE stream with event sequence numbers and a bounded replay ring — every CRUD from any surface appears in every open view within a round-trip, reconnects catch up deterministically, optimistic slot writes reconcile against live events, and the frontend never has to say "reload to sync".**
