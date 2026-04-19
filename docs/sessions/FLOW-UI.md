# Flow UI — Implementation Scope

Scope for the Studio flow editor — the canvas where users wire nodes, configure them, and watch messages travel. First-party, *not* a plugin (see [PLUGINS.md](../design/PLUGINS.md) rationale: the canvas is too central to hide behind a plugin contract). Plugins contribute **node kinds, property panels, and dashboard widgets** into the canvas the editor already hosts.

Authoritative references: [UI.md](../design/UI.md), [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md), [NODE-AUTHORING.md](../design/NODE-AUTHORING.md), [RUNTIME.md](../design/RUNTIME.md). This doc is the concrete shipping plan.

## Goal

Replace [FlowsPage.tsx](../../frontend/src/pages/flows/FlowsPage.tsx) (today's read-only node list) with a working canvas that:

- Renders the agent's graph as nodes + links.
- Lets users drag kinds from a palette, wire them, edit settings in a property panel, save.
- Streams live slot values onto nodes — counters tick, points light up.
- Stays usable on a 13" laptop at 1× zoom and on a 4k desktop at 2× zoom.

**Non-goal for Stage 1:** the full "live wiring in the tree" vs "flow document container" distinction from [EVERYTHING-AS-NODE.md § "Flow documents are nodes too"](../design/EVERYTHING-AS-NODE.md). We pick one — flow-document containers — and ship that. Live wiring at arbitrary graph paths is Stage 2.

## Scope rails

**In:**

| In |
|---|
| React Flow v12 canvas inside Studio ([frontend/src/pages/flows/](../../frontend/src/pages/flows/)) |
| Palette reads registered kinds from the agent's `GET /api/v1/kinds` (already wired — [crates/transport-rest/src/kinds.rs](../../crates/transport-rest/src/kinds.rs)) |
| One "flow" = one `sys.core.flow` container node; its children are the compute nodes on the canvas |
| CRUD: create node, move node (persisted as `position` slot on the node), delete, connect/disconnect links |
| Property panel (right side) rendered from the selected node's `settings_schema` via `@rjsf/core` |
| Live slot display — SSE (`GET /api/v1/events`) updates node badges as values change |
| Undo/redo in-memory for the session (no server-side history this stage) |
| Keyboard: delete, duplicate, multi-select with shift, zoom, pan, focus-node |
| Auto-layout button (`dagre` or `elkjs`) for imported flows that arrive without positions |
| Plugin-contributed custom node renderers via `contributes_to: node:<kind_id>` in `plugin.yaml` |

**Out (explicitly):**

| Out | Why |
|---|---|
| Live-wiring mode (nodes outside a flow container, wired directly) | Stage 2 — adds cognitive surface we don't need yet; most users want document-style |
| Subflow authoring (flows-as-services) | Needs versioned flow deployment (Stage 3) |
| Collaborative editing / presence | Needs CRDT / OT — large; defer past Stage 3 |
| Server-side undo history | Out of scope; session-local history is enough for MVP |
| Mobile / touch gestures | React Flow ships basic support; polish later |
| Read-only "public share" mode | Deferred; needs permission model |
| Function-node inline editor (QuickJS) | Plugin-delivered when we ship the QuickJS runtime |
| Safe-state policy editor | [RUNTIME.md § "Safe-state handling"](../design/RUNTIME.md) — a property-panel widget, not canvas work; schedule after MVP |
| Flow-level validation (cycles, type-compat) beyond what the graph enforces on save | Server does the hard checks; canvas just surfaces errors |

## Architecture

Three components, clean seams:

```
┌────────────────────────── Studio FlowsPage ──────────────────────────┐
│                                                                      │
│  ┌──────────────┐  ┌────────────────────────┐  ┌──────────────────┐ │
│  │   Palette    │  │     Canvas             │  │  Property Panel  │ │
│  │   (kinds)    │→ │  (React Flow)          │← │  (@rjsf/core)    │ │
│  └──────────────┘  │                        │  └──────────────────┘ │
│         ↑          │   nodes ← /api/v1/     │          ↑            │
│         │          │   links ← /api/v1/     │          │            │
│         │          │   live  ← SSE events   │          │            │
│         │          └────────────────────────┘          │            │
│         │                     ↕                        │            │
│         │               useFlowStore (zustand)         │            │
│         │   selection · dirty · undo stack · viewport  │            │
│         │                     ↕                        │            │
│         └──────────── AgentClient ────────────────────┘            │
│                 (nodes, slots, links, events)                        │
└──────────────────────────────────────────────────────────────────────┘
```

- **Canvas** owns React Flow. Every on-canvas node maps to exactly one graph node (child of the flow container). Every React Flow edge maps to one `Link`.
- **Palette** lists every `KindManifest` the host has registered (first-party + plugin-contributed, no distinction in UI — we don't promote trust tiers in the palette).
- **Property panel** reads the selected node's kind manifest, renders `@rjsf/core` form, writes back through `POST /api/v1/config`.
- **Store** (zustand) is UI-only: selection, clipboard, viewport, dirty flag, undo stack. Server state goes through TanStack Query + SSE invalidations.

## Data model — what lives on the wire

A flow is a graph subtree. No new REST surface.

```
/flows/                          (folder — isContainer)
  my-flow/                       (sys.core.flow — isFlow, isContainer)
    ├─ counter-1/                (sys.compute.count)
    ├─ switch-1/                 (sys.logic.switch)
    └─ webhook-1/                (sys.io.webhook)

links: [counter-1.out → switch-1.in,  switch-1.out_0 → webhook-1.send]
```

**Per-node slots the canvas adds to every kind:**

| Slot | Role | Who writes | Why |
|---|---|---|---|
| `position` | `config` | canvas (on drag-end) | `{x: number, y: number}` — persisted so the layout survives restarts |
| `notes` | `config` | canvas (inline edit) | optional node-level annotation for operators |

No "flow document JSON" alongside the graph — the graph *is* the flow document. Export/import at Stage 2 serialises the subtree.

## Feature breakdown

### Canvas (React Flow)

| Feature | Notes |
|---|---|
| Node renderer | Default: icon + title + lifecycle badge + live-value badges (top 2 status slots). Plugins can replace via `contributes_to: node:<kind_id>` (MF-exposed `./Node`). |
| Edge renderer | Default: smoothstep with arrow; colour by source slot's type (number = blue, Msg = grey, error wires = red). |
| Selection | Single-click, shift-click for multi, rubber-band. |
| Keyboard | `Delete` / `Backspace` removes selection; `⌘/Ctrl-D` duplicates; `⌘/Ctrl-Z` / `⌘/Ctrl-⇧-Z` undo/redo; `⌘/Ctrl-A` select all; `F` focus selection. |
| Drag from palette | HTML5 DnD into the canvas viewport; drop position becomes the node's `position` config slot. |
| Auto-layout | Button in toolbar invokes `dagre` on current node+edge set; writes new positions. |
| Minimap + zoom controls | React Flow built-ins; minimap shows lifecycle colours. |
| Undo/redo | Session-local stack over a `FlowCommand` enum (`addNode`, `moveNode`, `addLink`, `deleteNode`, `patchConfig`). Each command has a `do/undo` pair that translates to agent API calls. |

### Palette (left side)

- Searchable list of `KindManifest`s from `GET /api/v1/kinds`.
- Grouped by first facet (Driver, Compute, Logic, I/O, System).
- Plugin-contributed kinds show a small badge (plugin id in tooltip) — factual, not a warning.
- Drag starts a `{type: "kind", id: "sys.compute.count"}` payload.

### Property panel (right side)

- Binds to selection. Empty state when nothing selected.
- Top: path, kind, lifecycle, inline rename.
- Middle: `@rjsf/core` form generated from `kind.settings_schema`. Multi-variant schemas show the selection dropdown first, then the variant form.
- Bottom: status slots (read-only, live-updating); links to/from this node.
- "Save" button is autosave-with-debounce (500ms) — commits via `POST /api/v1/config`. Last-write-wins per [EVERYTHING-AS-NODE.md § "Validation — three layers"](../design/EVERYTHING-AS-NODE.md).
- Plugin property panels contributed via `contributes_to: property-panel:<kind_id>` override the `@rjsf/core` default.

### Live data

- One SSE connection to `/api/v1/events` at page mount; close on unmount.
- `SlotChanged` events fan into the relevant node's renderer → badge flashes + re-renders with new value.
- No diffing — React Flow's `nodes` reference is kept stable across slot updates; the renderer component reads live slot values from a separate context so the canvas doesn't re-layout on every tick.
- **Deadband**: if more than 10 updates/sec arrive on a node, the badge coalesces to "…updating…" and shows the latest every ~250ms. Prevents 60fps canvas thrash on telemetry nodes.

### Toolbar

- New flow · Save · Auto-layout · Fit to view · Zoom in / out · Toggle minimap · Toggle grid · Run (Stage 2, disabled button today with tooltip).

### Persistence

- Every canvas mutation is an agent REST call:

| Canvas action | REST call |
|---|---|
| drop a palette entry | `POST /api/v1/nodes` `{parent, kind, name}` then `POST /api/v1/slots` to set `position` |
| drag a node | debounced `POST /api/v1/slots` on `position` |
| connect two ports | `POST /api/v1/links` |
| delete selection | `DELETE /api/v1/node?path=…` per node (server cascades links) |
| edit config in panel | debounced `POST /api/v1/config` |

No "dirty" state that needs a save button — the canvas is always consistent with the server. The toolbar's "Save" is a no-op alias (kept for muscle memory) until we introduce the edit/commit model in Stage 2.

## Staged landing

| Stage | What | Bail-out signal |
|---|---|---|
| **1a** — canvas foundations | React Flow mounted; renders current graph read-only; pan/zoom; minimap. Ties into `useNodes` + `useLinks`. | Canvas can't render 500 nodes at 60fps → pick a different canvas lib |
| **1b** — CRUD loop | Palette, drag-drop, auto-save position, property panel with `@rjsf/core`, delete, link create/delete. Undo for these commands only. | `@rjsf/core` can't render a real Modbus variant schema → replace with `react-hook-form` |
| **1c** — live data | SSE pump; node badges flash on `SlotChanged`; deadband coalescer. | CPU spikes past ~40% on realistic telemetry → back off to polling |
| **1d** — plugin slots | `contributes_to: node:<kind_id>` and `property-panel:<kind_id>` loaders wired; plugin-hello ships a custom Panel that takes over its own kind's property panel. | MF remote refuses to load into the canvas context → restrict to sidebar/dashboard only |
| **Stage 2** (later) | Export/import flow subtrees, subflow authoring, live-wiring mode, `Run` semantics (engine start/stop per flow) | — |

1a–1d target a single shipping increment (~1 week each at steady pace). Each 1x ships behind a flag if needed, but the goal is all four land as one page swap.

## Dependencies to add

Frontend:

| Package | Why |
|---|---|
| `@xyflow/react` (v12) | The canvas. Already listed as target in [UI.md](../design/UI.md). |
| `dagre` (or `elkjs`) | Auto-layout. Start with `dagre` — smaller, fine for ≤ ~200 nodes. |
| `@rjsf/core` + `@rjsf/validator-ajv8` | Schema-driven forms. Already in UI.md's stack table. |

No backend changes required for 1a–1c. Stage 1d only needs the existing `GET /plugins/:id/*` MF wire.

## Invariants this scope commits to

1. **The canvas never invents state.** Everything visible is derived from the graph; mutations round-trip through the agent. If the agent goes down mid-edit, the canvas shows the error banner, never silently buffers.
2. **Position is a slot, not a side file.** `position` lives on the node as a `config` slot. Restarts preserve layout; RBAC on `config` writes governs who can rearrange flows.
3. **Plugin UIs are opt-in overrides, never the default.** Every kind has a sensible default renderer + `@rjsf/core` panel the moment it's registered. Plugins enhance; they don't gate functionality.
4. **SSE is the only live-update path.** No polling fallbacks, no WebSocket shim. If SSE breaks, that's the signal to fix the event bus, not paper over it in the frontend.
5. **Undo is client-side only for now.** Server history is a separate feature (audit log exists but isn't a replayable command stream yet). The client stack is best-effort — closing the tab loses it.

## Open questions

- **Flow container creation UX.** Do users pick "New flow" from a menu, or do they drop a node at `/flows/` and it auto-wraps? Lean towards explicit "New flow" — avoids surprise containment rules.
- **Multi-flow UI.** One canvas per flow (React Router param), or a tabbed view? Start with one — adds a breadcrumb header; tabs defer to user feedback.
- **Handle naming on custom kinds.** React Flow needs `handleId`s matching slot names. Confirm the `KindManifest.slots` field already exposes names — yes, per [NODE-AUTHORING.md § "Anatomy of a node kind"](../design/NODE-AUTHORING.md).
- **How to surface server-side placement errors** when a drop violates containment. Toast + revert is the baseline; consider pre-validating via kind manifest lookups client-side for palette UX.
- **Keyboard focus when property panel is open.** React Flow wants keyboard for selection ops; `@rjsf/core` wants it for form fields. Use standard DOM focus-within semantics; delete-key listener checks `document.activeElement`.

## One-line summary

**Replace the read-only node list with a React Flow canvas that drives the agent's graph directly — palette → drag-drop → property panel → live data, all first-party, with plugin slots for custom renderers and property panels — landing in four tight stages (canvas, CRUD, live data, plugin slots) without introducing any new server primitive.**
