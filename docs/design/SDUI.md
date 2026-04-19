# Scope — Server-Driven UI (SDUI)

A typed component IR emitted by the backend and rendered by a tiny React runtime. SDUI is the **default / easy path** for plugin screens: a plugin can ship zero client code and still get working CRUD, device pages, settings forms, discovery, alarms, scope boards, PR review cards. Plugins that need specialist UX keep shipping Module Federation bundles as they do today; SDUI trees can embed those components via `widget` / `custom` variants. The React app is a dumb projector for the long tail; MF is the escape hatch for everything bespoke.

## Current status

**S1–S7 complete.**

- **S1**: `crates/ui-ir` (16 component variants), JSON Schema, versioning scaffolding, `agent ui vocabulary`.
- **S2**: `POST /api/v1/ui/action` + HandlerRegistry with `toast` / `navigate` / `none` / `full_render` / `form_errors` / `download` / `stream` response variants.
- **S3**: `GET /api/v1/ui/table` paginated endpoint + `custom` IR variant + client/TS/Rust client parity.
- **S4**: SDUI renderer at `frontend/src/sdui/` — `SduiProvider`, `Renderer`, `useActionResponse`, 16 component implementations. Route `/ui/:pageRef` renders any authored page. Dashboard page (`/dashboard`) replaced with a live `ui.page` node browser — shows a card per page, clicking navigates to `/ui/<id>`.
- **S5**: `GET /api/v1/ui/render?target=<id>[&view=<id>]` endpoint. `spi::KindManifest` gained `views: Vec<KindView>` (each view is `{id, title, template: JsonValue, priority}`). DashboardState threaded with an `Arc<KindRegistry>` so the endpoint can look up a target's kind, pick the matching view, substitute `{{$target.*}}` bindings in the template, and return the same `ResolveResponse` shape `/ui/resolve` emits. `sys.logic.heartbeat` now declares an inline `overview` view (heading + badges bound to `current_state` / `current_count`). Full NEW-API parity landed in the same PR: Rust client (`client.ui().render(target, view)`), CLI (`agent ui render --target <id> [--view <id>]`) + CommandMeta, TS client (`client.ui.render(...)`), fixtures (`ui-render/{ok,not-found}.json`) + `fixture_gate` tests, plus a new React route `/render/:targetId` backed by `SduiRenderPage.tsx`. SSE subscription consumer wired via `frontend/src/sdui/useSubscriptions.ts` — opens `client.events.subscribe()`, matches incoming `slot_changed` events against the resolve/render response's `subscriptions` subjects, and invalidates the matching React Query cache entry (no polling).

### Implementation notes / divergences from design

- **Package location**: the renderer ships as `frontend/src/sdui/` (co-located with the app), not as a separate `@acme/sdui-react` package. Extraction is a later step.
- **Resolver fast-path** (`crates/dashboard-transport/src/resolve.rs`): `POST /api/v1/ui/resolve` checks whether the `ui.page` node's `layout` slot is non-null *before* running the legacy widget resolver. If present, it parses the slot directly as a `ComponentTree` and returns it, bypassing the M4-era `ui.widget` child-node resolver entirely. This enables hand-authored (or AI-authored) SDUI trees written via `agent slots write … layout '<json>'` to render without any widget nodes.
- **`UiComponent` type narrowing**: the TypeScript `UiComponent` interface uses an index signature (`{ type: string; [key: string]: unknown }`), which means `Extract<UiComponent, { type: "page" }>` resolves to `never`. Work-around: `types.ts` in `frontend/src/sdui/` defines 16 explicit node shapes (`PageNode`, `TableNode`, …) and the renderer casts with `node as unknown as PageNode`.
- **In-memory graph**: all nodes (including authored `ui.page` nodes) live only in the running server process. Every server restart wipes them. Re-create the demo page after each restart with `agent nodes create` + `agent slots write`.

### Demo page — heartbeat monitor

A concrete example page is provided at `/dashboards/heartbeat-demo` (must be re-created after each restart):

```bash
# Create page
agent nodes create /dashboards ui.page heartbeat-demo   # if /dashboards doesn't exist: create / sys.core.folder dashboards first

# Write layout
agent slots write /dashboards/heartbeat-demo layout '{
  "type":"page","ir_version":1,"title":"Heartbeat Monitor","id":"root",
  "children":[
    {"type":"row","id":"r1","children":[
      {"type":"heading","id":"h","content":"flow-1 / heartbeat","level":2},
      {"type":"badge","id":"b","label":"live","intent":"ok"}
    ]},
    {"type":"table","id":"t","source":{"query":"path==\"/flow-1/heartbeat\"","subscribe":false},
     "columns":[
       {"title":"Node","field":"path"},
       {"title":"current_count","field":"slots.current_count"},
       {"title":"current_state","field":"slots.current_state"}
     ],"page_size":10}
  ]
}'

# Verify resolver returns the tree (not empty children)
agent ui resolve --page <id> -o json
```

### S5 divergences / notes

- Binding evaluation in `/ui/render` is a pragmatic walker (`crates/dashboard-transport/src/render.rs` — `substitute_bindings`) that handles `$target.id`, `$target.path`, `$target.name`, `$target.kind`, and `$target.<slot>` — enough for the POC spine. The full `/` child-traversal grammar from DASHBOARD.md still routes through the dashboard-runtime `Binding::parse`/`EvalContext` path; unifying the two is tractable work but not required for S5's acceptance (heartbeat view renders end-to-end without it). When we need `$target/outdoor-temp.value` we'll swap the walker for a recursive resolver over `EvalContext`.
- Subscription plan on `/render` is coarse: every `{{$target.<slot>}}` reference in the template contributes one subject scoped to the target. The React renderer's `useSubscriptions` hook then invalidates on any matching `slot_changed` event. Fine-grained per-widget subscription plans (one plan per component id) are an S6 refinement.
- "Persist graph to disk" from the milestones table is orthogonal to the render spine and remains open — authored pages + plugin views still vanish across server restart.

### S6 summary

Shipped the remaining IR vocabulary — `chart`, `sparkline`, `tree`,
`timeline`, `markdown`, `ref_picker`, `wizard`, `drawer`. Chart zoom
writes `{from, to}` into `$page[page_state_key]` (default
`"chart_range"`) and double-click clears it; the render endpoint
re-reads `page_state` on the next round-trip. `timeline` + `markdown`
support `subscribe` with `mode: "append" | "replace"` for streaming
content. `sparkline` subscribes to one subject; `chart` derives its
subject from `source: { node_id, slot }`. Subscription-plan
derivation recognises all three (charts, sparklines, tables) and
emits per-widget plans keyed by the authored component id, so the
client invalidates only the affected widget — no broad sweeps.

### S7 summary

- **Optimistic actions** — `Action.optimistic: { target_component_id, fields }` applied via React-Query `setQueryData` before the round-trip fires; authoritative `Patch` / `FullRender` responses replace through the same `applyPatch.ts` helpers. Rollback on round-trip error.
- **Capability handshake** — [`frontend/src/sdui/capability.ts`](../../frontend/src/sdui/capability.ts) surfaces `SUPPORTED_IR_VERSION`; `SduiPage` and `SduiRenderPage` refuse to project a tree whose `ir_version` exceeds it and show a clean mismatch banner.
- **DoS limits** — `crates/dashboard-transport/tests/limits.rs` asserts each of `page_state_bytes`, `render_tree_bytes`, `tree_nodes`, `tree_depth`, `component_types` returns 413 with its stable `what` tag. The legacy `widgets_per_page` and `binding_ref_depth` limits were removed with the widget resolver; new tree-shape limits (`tree_nodes` 2k, `tree_depth` 32, `component_types` 60) match SDUI.md § "Size & DoS limits".
- **Falsification tests** — `crates/dashboard-transport/tests/falsification.rs`. UC1 BACnet discovery, UC2 PR review card, UC3 scope-plan board each ship as a fixture `layout` + minimal handler stubs; resolve + action round-trips pass with zero domain-specific keywords in `ui-ir` or the React renderer.

## Build discipline — POC-first, full-core, hard-delete

Two principles govern how SDUI is built and how it interacts with the code already shipped:

1. **POC-first with the full core architecture, a minimal feature set.** S1 is not an MVP of one component — it is the complete architecture (IR crate + resolver + action dispatcher + React renderer + capability handshake) wired end-to-end with ~10 components. The bar is: "a plugin author can ship a working screen through the whole pipeline." Every remaining component is additive — the architecture is not incrementally grown, the vocabulary is. The long-term vision (~32 components, custom escape hatch, streaming, optimistic actions) drives the architecture decisions even in S1; missing components are added later, missing architecture is not.

2. **Hard-delete anything not needed. No back-compat during pre-production.** The platform has not shipped. Existing kinds, endpoints, crates, or client methods that don't fit SDUI's shape get deleted outright — not deprecated, not wrapped, not migrated through a shim. Specific candidates: any `ui.*` resolver output shape that narrows to ComponentTree gets its old shape removed in the same PR; unused widget-registry abstractions from M4 that don't map onto the IR `widget` variant get replaced, not extended. The only constraint is "`cargo test --workspace` and the TS tests stay green". Everything else is fair game to delete.

Both principles apply through S1–S7.

## Plain-English overview

Before the formal scope below, a one-page version for plugin authors:

**The one sentence.** The server sends JSON describing the screen; the React app turns that JSON into pixels.

**The picture.**

```
 ┌──────────┐   component tree (JSON)   ┌──────────────┐
 │ Backend  │ ─────────────────────────▶ │ React (dumb) │
 │  (Rust)  │                             │              │
 │          │ ◀───────────────────────── │              │
 └──────────┘   action (button click)    └──────────────┘
```

**What the server sends.** A typed tree where every node is one of ~32 known component kinds (`row`, `col`, `table`, `form`, `button`, `diff`, `rich_text`, …) plus two escape hatches (`widget`, `custom`) that point at plugin-registered React components.

**What the client does.** Switch on `node.type` and render the matching React component, recurse into children, wire actions back to `/api/v1/ui/action`. That's the whole client. ~800 lines.

**What a plugin author ships** to get a working device page / settings form / discovery screen:
- A kind manifest (existing concept) + one `views: [{ template: ComponentTree }]` entry declaring what to render.
- A handful of action handlers (`bacnet.scan`, `node.update_settings`) registered in the handler registry.
- Zero frontend code. (They *may* ship MF bundles for specialist widgets; they don't *have to*.)

**Where data comes from.** Three flavours — resolve-time bindings (baked in), live subscriptions (NATS streams patches), on-demand queries (`GET /ui/table?...`).

**Why this works.** The long tail of plugin screens is CRUD-shaped. One renderer + one IR covers every driver, integration, and workflow kind. Specialist UX still uses Module Federation; SDUI is the easy path, not a wall.

---

## Goal

Let a plugin ship a kind and have every client screen for that kind appear simultaneously — with no per-client code, no per-screen templates on the client, and no framework lock-in. One install, one renderer, N plugin screens.

The rule: **the React app never knows what BACnet, a PR, or a scope is.** It knows `table`, `form`, `button`, `row`, `diff`. Plugins ship component trees + action handlers; the renderer fills in the rest.

### SDUI is the default path, not a mandate

SDUI lets plugins ship **zero client code** and still get working screens — that's the point. It is **not** a restriction; plugins remain free to ship their own React via Module Federation (see [PLUGINS.md](PLUGINS.md)) whenever they need full control. Every plugin picks its own mix:

| Plugin strategy | When to use | Cost |
|---|---|---|
| **SDUI only** (default) | CRUD, device pages, settings forms, alarm authoring, discovery — the long tail of plugin screens. | Zero FE code. Ship a backend handler + a component tree per kind view. |
| **SDUI + a few custom renderers** | Plugin is 95% CRUD but needs one specialist widget (gauge, sparkline, floor-plan overlay). | Ship a small MF bundle that registers `renderer_id`s the `custom` variant references; the rest stays SDUI. |
| **Full Module Federation bundle** | Plugin's UX is specialist (custom visualisations, heavy interactions, domain-specific authoring). UC1's `com.acmefac.ui.dashboard` goes here — gauges, trends, floor plans. | Ship React like any normal app. SDUI isn't involved. |

**Two integration points connect the worlds:**

1. The `custom` IR variant embeds a plugin-registered React component *inside* an SDUI tree. A page can be 90% SDUI with one floor-plan pane that's MF-provided.
2. Any screen not described by an SDUI tree is just a regular MF route. The Studio shell doesn't care whether a plugin's screen is SDUI-rendered or MF-bundled.

### The framework's own authoring tools

A small, enumerated set of *framework-provided* screens ship as bespoke React (not SDUI) because they are authoring tools for the IR itself and would chase their own tails if SDUI-rendered:

- **Dashboard builder** — drag-drop authoring of `ui.page` / `ui.template` trees (Studio only).
- **Flow canvas** — node-and-wire DAG editor for `sys.core.flow` documents (Studio only).
- **Scope / form schema editor** — if ever built visually; likely text-based for v1.

These are ~3 screens total. They ship once, with the framework, and stop expanding — unlike plugin screens, which grow with every extension installed.

## Authoring modes

A `ui.page` can be authored three ways. All three produce the same artifact — a `ui.page` node whose `layout` slot holds a validated `ComponentTree`. The renderer doesn't know or care who wrote it.

| Mode | Who authors | When shipped | Notes |
|---|---|---|---|
| **AI via CLI** | An LLM session (Claude Code / OpenCode / similar) the user already runs locally, shelling out to `agent <cmd>` | **First demo — S1** | Uses only the existing CLI + `agent schema` + `--help-json`. Zero new endpoints. See "AI-driven authoring" below. |
| **Drag-drop** | User in Studio, Grafana-style canvas | Post-S4 | Needs the IR stable + renderer mature. This is the "dashboard builder" framework tool — bespoke React, carved out above. |
| **Hand-written** | Power user editing JSON or YAML directly | Free at any time | Just `agent slots write /path/to/page layout '<json>'`. |

### AI-driven authoring — the 2026 starting point

The AI session is the user's own Claude Code / OpenCode instance (per UC2 — the user pays the AI bill, not the platform). It authors dashboards by running the CLI:

1. **Discovery.** `agent schema --all -o json` dumps input/output schemas for every subcommand. `agent schema ui resolve` gives the IR tree shape. The AI knows what's available without hard-coding anything.
2. **Vocabulary.** `agent ui vocabulary -o json` (new in S1 — see milestones) dumps the IR component-union JSON Schema — the authoritative list of component types the AI can emit.
3. **Authoring.** The AI creates the page node and writes the layout:
   ```
   agent nodes create /dashboards ui.page my-dashboard
   agent slots write /dashboards/my-dashboard layout '<component-tree JSON>'
   ```
4. **Preview.** `agent ui resolve --page <id> --dry-run -o json` validates the tree without producing a render, surfaces structured errors the AI can repair.
5. **Commit.** `agent ui resolve --page <id>` confirms the page renders. If any binding fails, the AI iterates.

MCP is deferred. UC2's per-user MCP endpoint will expose these same operations as tools later; until then, shelling out is the contract. Every structural thing an MCP server would need (`--help-json`, `agent schema`, deterministic JSON output, stable exit codes) is already in the CLI.

Sucrase / JSX-over-wire is also deferred — see "Deferred" below. The typed IR is expressive enough that an LLM emitting JSON can cover the 2026 use cases; runtime JSX eval stays in reserve.

## Non-goals

- Offline-first. SDUI is online-only in v1. A persistent offline mode is a later concern; the cache layer that makes it possible is in scope, full offline is not.
- Client-side business logic. Zero. Every interaction round-trips. Optimistic-patch hints exist so the UX doesn't feel laggy — but the authoritative response is always the server's.
- A full layout engine. IR components map 1:1 to existing React layout primitives (flex/grid). No bespoke layout algorithm.
- A theme system. Theming rides on the React app's styling layer. IR carries semantic hints (`intent: "danger"`, `size: "lg"`) not CSS.
- Forcing plugins to use SDUI. Plugins may ship full Module Federation bundles when they need full UX control (see "SDUI is the default path, not a mandate" above). SDUI covers the long tail of CRUD/viewing screens so plugin authors don't *have to* write React, not so they *can't*.
- A pre-approved list of plugin React components the IR magically knows about. If a plugin wants a React component rendered inside an SDUI tree, it registers a `renderer_id` and uses the `custom` variant. If it wants entire pages outside SDUI, it ships a normal MF route.
- Authoring tools themselves. The dashboard builder, flow canvas, and any future visual IR editor ship as framework-provided bespoke React. SDUI is a rendering protocol, not a visual authoring protocol.
- Flutter / Swift / other platforms. Deferred — the IR is language-agnostic by construction (JSON + JSON Schema) but only React is in v1 scope.

## Principle

> **The server emits a typed component tree; the client renders it.**

Not a DSL. Not a template language. A discriminated union of ~30 component kinds, each with a serde-stable schema, each versioned, each mapped to one React component. JSON in, pixels out. Interactions go back over HTTP.

## The component IR

Six categories, ~32 components total, plus a `custom` escape hatch. Every component is a Rust enum variant with `#[derive(Serialize, Deserialize, JsonSchema)]` in a new `crates/ui-ir` crate. The union discriminator is a stable `"type"` field on the wire.

| Category | Components | Purpose |
|---|---|---|
| **Layout** | `row` · `col` · `grid` · `tabs` · `stack` · `split` · `scroll` · `spacer` | Flex/grid mapping. No new layout algorithm — delegates to browser. |
| **Display** | `text` · `heading` · `badge` · `kpi` · `icon` · `image` · `markdown` · `code` · `sparkline` · `chart` · `diff` | Pure read. `chart`/`sparkline` take a subscription subject, not a snapshot. `diff` renders unified or side-by-side diffs with read-only annotations supplied as props; inline commenting composes via per-line `action`s that open a `dialog` with a `form` (see "Diff interactions" below). |
| **Data** | `table` · `tree` · `list` · `detail` · `timeline` | `table` is a query + columns + row action — paging/sort/filter are server-side. |
| **Input** | `field` · `select` · `ref_picker` · `date` · `slider` · `toggle` · `search` · `rich_text` | Validated via JSON Schema. `ref_picker` queries the node graph. `rich_text` is a markdown-aware editor — required by UC3 scope authoring and anywhere a user writes long-form content. |
| **Interactive** | `button` · `link` · `menu` · `dialog` · `drawer` · `toast` | Trigger actions. Actions are server round-trips. |
| **Composite** | `form` · `card` · `header` · `kpi_grid` · `wizard` | Typed wrappers over the above so clients can optimise rendering (e.g. `form` owns validation UX). |
| **Escape** | `custom` | `{ "type": "custom", "renderer_id": "acme.floorplan", "props": {...}, "subscribe": [...] }`. The React app looks up `renderer_id` in a local registry plugins populate. Ships in v1 (not deferred) — covers floor-plan overlays (UC1), schedule grids (UC1), state-machine diagrams (UC3), streaming AI output (UC2), and anywhere a domain needs specialist UX. |

Every component tree has a root of kind `page` (with `title`, `children`, optional `header`/`drawer`). Embedding at any depth is allowed.

### Versioning

IR carries an `ir_version: u32` at the root. The client advertises the versions it supports in the capability handshake (`/api/v1/capabilities` already exists); the server clamps emission to the highest mutually-supported version. Adding a component variant is a minor bump; removing or re-shaping is a major bump with a 12-month deprecation window. Same discipline as REST API versioning.

### Embedding plugin React inside an SDUI tree

Plugins that ship Module Federation bundles can register their components so SDUI trees can embed them. Two leaf variants defer rendering to a plugin-provided React component:

- **`widget`** — for small, slot-bound visualisations. Plugin registers a `widget_type` (e.g. `acme.gauge`) in the existing widget registry; the IR references it as `{ "type": "widget", "widget_type": "acme.gauge", "props": {...} }`.
- **`custom`** — for full-screen specialist renderers (floor plans, node-graph diagrams, schedule grids). Plugin registers a `renderer_id` in a parallel registry; the IR references it as `{ "type": "custom", "renderer_id": "acme.floorplan", "props": {...} }`.

Both are looked up in the React app's client-side registry (populated at MF-bundle load time), and both degrade to a neutral stub when the id is unknown. The server filters them against the client's advertised capabilities before emission — an unfamiliar `widget_type` never makes it to the client.

The distinction by intent: `widget` for composable pieces (drop it into a `grid`), `custom` for "hand me the whole pane". Nothing forces a plugin to use either — if a plugin owns a route entirely, it skips SDUI and ships a normal MF page.

## Data bindings

Reuse the existing grammar from DASHBOARD.md — `$target.*`, `$stack.*`, `$self.*`, `$user.*`, `$page.*` — with one additive extension: the `/` child-by-name traversal. Every IR component's value slots accept either a literal or a binding expression wrapped in `{{ }}`. The resolver fills bindings server-side before emission; the client never sees expressions.

### Binding grammar

```
binding := source ( ( "." ident )     # slot read on the current node
                  | ( "/" ident )     # child node by name
                  )*
```

- `.` is **data access** — read a slot on whatever node the cursor is pointing at.
- `/` is **graph traversal** — move the cursor to a named child node. The cursor is still a node; follow with more `/` or a `.slot` read.

Both compose freely. `$target/outdoor-temp.value` means "navigate to the child named `outdoor-temp` under `$target`, then read its `value` slot."

### Worked example — one reusable page, N buildings

Graph:

```
/buildings/
├── building-1    (sys.site, name: "Building 1")
│   ├── outdoor-temp  (sys.point, slots: { value: 68.4, units: "°F" })
│   └── kwh           (sys.point, slots: { value: 142.7 })
├── building-2    (sys.site, name: "Building 2")
│   ├── outdoor-temp  (slots: { value: 72.1, units: "°F" })
│   └── kwh           (slots: { value: 98.3 })
└── building-3 ...

/pages/pageOverviewCommon   (ui.page — authored once, reused by every building)
```

Authored `ui.page.layout` (written once, never duplicated):

```json
{ "type": "page",
  "title": "{{$target.name}} Overview",
  "children": [
    { "type": "kpi",
      "id": "kpi-outdoor",
      "label": "Outdoor Temp",
      "value": "{{$target/outdoor-temp.value}}",
      "unit":  "{{$target/outdoor-temp.units}}" },
    { "type": "kpi",
      "id": "kpi-energy",
      "label": "Energy (kWh)",
      "value": "{{$target/kwh.value}}" },
    { "type": "table",
      "id": "alarms",
      "source": {
        "query": "parent_path=prefix={{$target.path}}/alarms AND kind==alarm.active",
        "subscribe": true },
      "columns": [
        { "title": "Time",     "field": "slots.ts.value" },
        { "title": "Severity", "field": "slots.severity.value" } ] }
  ] }
```

Resolving against `building-1`:

```http
POST /api/v1/ui/resolve
{ "page_ref": "/pages/pageOverviewCommon", "target_ref": "/buildings/building-1" }
```

Yields:

```json
{ "type": "page",
  "title": "Building 1 Overview",
  "children": [
    { "type": "kpi", "id": "kpi-outdoor",
      "label": "Outdoor Temp", "value": 68.4, "unit": "°F" },
    { "type": "kpi", "id": "kpi-energy",
      "label": "Energy (kWh)", "value": 142.7 },
    { "type": "table", "id": "alarms",
      "source": {
        "query": "parent_path=prefix=/buildings/building-1/alarms AND kind==alarm.active",
        "subscribe": true },
      "columns": [...] } ] }
```

Subscription plan scopes every live subject to `building-1`:

```json
[ { "widget_id": "kpi-outdoor",
    "subjects": ["node./buildings/building-1/outdoor-temp.slot.value",
                 "node./buildings/building-1/outdoor-temp.slot.units"] },
  { "widget_id": "kpi-energy",
    "subjects": ["node./buildings/building-1/kwh.slot.value"] },
  { "widget_id": "alarms",
    "subjects": ["query.parent_path==/buildings/building-1/alarms.kind==alarm.active"] } ]
```

Resolving the same page against `building-2` flips every path to `building-2`; subscriptions isolate automatically. **One page node, N buildings, zero duplication.**

### What happens if a child is missing

`$target/outdoor-temp.value` against a building that has no `outdoor-temp` child surfaces `BindingError::RefNodeMissing`. The affected component degrades to a `{ "type": "dangling", "id": "..." }` stub; the rest of the page renders. When someone later creates the missing child, `ui.invalidate` fires → client re-resolves → the stub becomes live. Same rules as DASHBOARD.md § "ACL policy".

### Other common patterns

```
{{$target.name}}                     # this node's name
{{$target/alarms/active.count}}      # two-level child walk, then slot read
{{$stack.site/meter-1/kwh.value}}    # addressed via nav frame instead of target
{{$user.org_id}}                     # auth claim
{{$page.selected_row}}               # client-owned page state
```

The handler that emits IR runs the same binding engine the existing `/ui/resolve` endpoint uses; the only new thing from S1 is that the output shape is the IR tree and the grammar gains the `/` child traversal.

## Nodes vs IR components — what's what

Before going further, the distinction between "a node in the graph" and "a component in an IR tree" matters enough to be explicit. They are different kinds of things.

### The full graph for the building example

```
/                                      ← root station (sys.core.station, built-in)
├── buildings/                         sys.core.folder   (built-in)   container
│   ├── building-1                     sys.site          (plugin)     DATA — the real building
│   │   ├── outdoor-temp               sys.point         (plugin)     DATA — a sensor
│   │   └── kwh                        sys.point         (plugin)     DATA — a meter
│   └── building-2 …
│
├── templates/                         sys.core.folder   (built-in)
│   └── overviewTpl                    ui.template       (SDUI)       reusable layout blueprint (optional)
│
├── pages/                             sys.core.folder   (built-in)
│   └── pageOverviewCommon             ui.page           (SDUI)       the authored dashboard (layout slot = ComponentTree)
│
└── nav/                               sys.core.folder   (built-in)
    └── home                           ui.nav            (SDUI)       sidebar root
        ├── buildings                  ui.nav            (SDUI)       "Buildings" section header
        │   ├── b1                     ui.nav            (SDUI)       sidebar row — frame_ref → /buildings/building-1
        │   ├── b2                     ui.nav            (SDUI)       sidebar row — frame_ref → /buildings/building-2
        │   └── b3                     ui.nav            (SDUI)
        └── alarms                     ui.nav            (SDUI)
```

### Kinds used above — quick reference

| Kind | Shipped by | Stores | Role |
|---|---|---|---|
| `sys.core.station` | Built-in | Nothing special | One per graph; the `/` node |
| `sys.core.folder` | Built-in | Children | Pure grouping |
| `sys.site`, `sys.point` | **Plugin** (e.g. `com.acmefac.bms`) | Domain slots (`name`, `value`, `units`, …) | The real-world entities |
| `ui.page` | Built-in (SDUI) | Slot `layout: ComponentTree` (+ optional `template_ref`, `bound_args`) | An authored dashboard |
| `ui.template` | Built-in (SDUI) | Slot `layout: ComponentTree` + `requires` (hole schema) | Reusable blueprint pages pin via `template_ref` |
| `ui.nav` | Built-in (SDUI) | Slots: `title`, `path`, `frame_alias`, `frame_ref`, `order` | One row in the sidebar; pushes a frame onto the context stack when clicked |

### What is *not* a graph node

The individual `kpi` / `table` / `row` / `button` / `form` / `diff` / `widget` entries inside a `ui.page.layout` are **IR components**, not nodes. They are JSON inside a single slot on a single node. They have no id, no path, no ACL of their own — they inherit the containing page's identity and permissions.

| | Graph node | IR component |
|---|---|---|
| Persistent identity | ✅ uuid + path | ❌ just a `type` tag + optional `id` string for targeting patches |
| ACL, audit, versioning | ✅ inherited from the node model | ❌ inherits the page/template node's |
| Live subscription subject | ✅ `node.<id>.slot.<name>` | ❌ — its data subscriptions come from its bindings, which resolve to node slots |
| Examples | `ui.page`, `ui.nav`, `sys.point` | `kpi`, `table`, `button`, `widget` (reference to a plugin-registered React component) |

**The `widget` variant is a reference, not a node.** When the IR carries `{ "type": "widget", "widget_type": "acme.gauge", "props": {...} }`, `acme.gauge` is an id looked up in the client's React-component registry (populated by MF bundles). The old M1–M5 `ui.widget` graph kind stays for backward shape but is rarely authored directly — the common case is inline IR `widget` variants inside a page's `layout`.

### How the building example wires together

**Sidebar click → target:**

```
ui.nav "b1".frame_ref   = { id: "/buildings/building-1" }
ui.nav "b1".frame_alias = "site"
```

Clicking `b1` pushes `{ alias: "site", node_ref: /buildings/building-1 }` onto the context stack. `$target` and `$stack.site` both resolve to building-1.

**Page → target (bindings):**

`/pages/pageOverviewCommon.layout` contains `{{$target/outdoor-temp.value}}` etc. The resolver walks from `$target` (building-1), descends to the `outdoor-temp` child, reads its `value` slot. Same page renders differently per building because `$target` changes.

**Page → template (optional):**

```
ui.page "pageOverviewCommon".template_ref = { id: "/templates/overviewTpl", version: 3 }
ui.page "pageOverviewCommon".bound_args   = { /* fills template's `requires` holes */ }
```

The resolver loads the template's layout, substitutes `bound_args`, then fills bindings. One template can back many pages. Most simple dashboards skip templates — `ui.page.layout` carries its own inline `ComponentTree`.

### Decision guide

| You want to… | Use… | Stored as |
|---|---|---|
| Represent a real building | `sys.site` node | Graph node (plugin kind) |
| Represent a sensor reading | `sys.point` node | Graph node (plugin kind) |
| Author a dashboard | `ui.page` node | Graph node; its `layout` slot holds a `ComponentTree` |
| Share layout across dashboards | `ui.template` node + `template_ref` on pages | Graph nodes |
| Put a KPI tile / table / button on a dashboard | IR component | JSON inside the page's `ComponentTree` |
| Embed a plugin-provided React gauge on a dashboard | IR `widget` variant | JSON inside the ComponentTree; `widget_type` looks up the React component in the client's MF-registry |
| Build the left sidebar | Tree of `ui.nav` nodes | Graph nodes; `frame_ref` slot points at what becomes `$target` on click |

## Actions — the interaction protocol

Every `button`/`link`/`menu` item carries an `action: Action`. One new endpoint:

```
POST /api/v1/ui/action
body: { handler: string, args: JsonValue, context: ActionContext }
```

`context` carries the current `target`, `stack`, `page_state`, `auth_subject` — the same tuple the resolver already takes.

Response is a discriminated union:

```json
{"type": "patch",        "target_component_id": "table-3", "tree": { ... }}
{"type": "navigate",     "to": { "target_ref": "..." }}
{"type": "full_render",  "tree": { ... }}
{"type": "toast",        "intent": "ok" | "warn" | "danger", "message": "..."}
{"type": "form_errors",  "errors": { "field": "message" }}
{"type": "download",     "url": "..."}
{"type": "none"}
```

Handlers are registered by name in a `HandlerRegistry` — same pattern as the widget registry. Plugins ship handlers; the framework validates and dispatches. Auth context flows in; RBAC enforced at the handler level via the existing `auth` crate.

**Optimistic hints**: a button may carry `optimistic: { patch: {...} }`. The client applies it immediately; the server response either confirms or replaces. This hides round-trip latency for the common "toggle a setting" case without client-side logic.

## Interaction state: chart zoom, virtualised scroll, etc.

Rich client-side gestures (chart pan/zoom, table virtualised scroll, tree expand/collapse) are handled by the React component implementation — *not* the IR. Two rules:

1. **View state that the server must observe** (e.g. "the user zoomed to this time range; fetch denser data") round-trips via `$page` state. Chart components carry `page_state_key: "chart_range"` hints; the React component writes `{ "chart_range": {from, to} }` into `$page` and re-issues `/ui/resolve` when it changes.
2. **View state that is purely local** (scroll position, hover state, expand/collapse rows) stays in the React component. Never round-trips.

This keeps the IR stateless and referentially transparent while letting specialist components (charts, large tables, trees) do their job. Table virtualised rendering for 10k+ rows is a client-runtime concern — `@acme/sdui-react`'s `table` component uses virtualised scroll internally; the IR stays unchanged.

## Diff interactions

`diff` is a display component. Inline commenting works by composition, not by embedding input affordances in the diff itself:

- The `diff` component takes `annotations: [{ line, text, author, created_at }]` in its props — read-only, server-supplied.
- It takes an optional `line_action: Action` (e.g. `{handler: "github.comment_pr_line", args: {pr: "...", line: "$line"}}`). Clicking a line fires the action; the `$line` placeholder is substituted from the click context before round-tripping.
- Handlers that open a comment form return `{type: "navigate", to: {...}}` or a patch that shows a `dialog` containing a `form`.

Rule: `diff` is pure display + per-line action callback. Everything beyond that composes with existing IR (`dialog`, `form`, `button`) — no bespoke input affordances inside the diff component.

## Streaming content

Some components display data that arrives incrementally — token-by-token AI output (UC2), live flow progress logs (UC3), tailing audit events. Two mechanisms, already built:

1. **Subscription-driven live update.** `text` / `markdown` / `code` / `timeline` components accept `subscribe: "<subject>"` in place of a literal value. Each NATS message on that subject appends to (or replaces, depending on `mode: "append" | "replace"`) the component's content. No new IR primitive needed — the subscription plan already emits the subject.
2. **Streaming action responses.** A long-running handler may return `{ "type": "stream", "channel": "<id>" }` and then push incremental patches to that channel via the same subscription mechanism. `@acme/sdui-react` holds a channel → component-id map and applies patches.

### Stream lifecycle — termination

Clients need an unambiguous end-of-stream signal. Rule:

- The server emits a **sentinel** final message on the channel: `{ "type": "stream_end", "channel": "<id>", "reason": "done" | "error" | "timeout" }`. On receipt, the client stops showing streaming indicators and treats the accumulated content as final.
- A server-side timeout (default **60 seconds** of inactivity on the channel) emits `stream_end` with `reason: "timeout"` and closes the channel. Handlers may override the timeout per-channel; clients never infer it.
- On client disconnect, the channel is garbage-collected server-side within the same window. Re-subscribing to a GC'd channel returns `stream_end` immediately with `reason: "gone"`.

These cover streaming without the IR growing a dedicated `stream_text` variant.

## Tables are queries, not row lists

```json
{
  "type": "table",
  "source": {
    "query": "parent_path=prefix=/floor1 AND kind==sys.driver.point",
    "subscribe": true
  },
  "columns": [
    { "title": "Name",  "field": "path",                     "sortable": true },
    { "title": "Value", "field": "slots.present_value.value" },
    { "title": "Units", "field": "slots.units.value" }
  ],
  "row_action": { "type": "navigate", "to": { "target_ref": "$row.id" } },
  "page_size": 50
}
```

Tables do not ship rows on render. The client renders empty → issues a `GET /api/v1/ui/table?source_id=<id>&page=1&sort=...` → gets a page of rows. Subsequent changes stream via the subscription plan the existing resolver already emits. **One component covers ~60% of every business-app screen**: device lists, alarms, tickets, audit logs, flow runs.

## Forms are JSON Schema + bindings

```json
{
  "type": "form",
  "schema_ref": "$target.settings_schema",
  "bindings": "$target.settings",
  "submit": {
    "type": "action",
    "handler": "node.update_settings",
    "args": { "target": "$target.id" }
  }
}
```

The client renders fields from the schema via a fixed mapping:

| JSON Schema construct | IR component |
|---|---|
| `"type": "string"` | `field` |
| `"type": "string", "format": "date-time"` | `date` |
| `"type": "string", "format": "markdown"` | `rich_text` |
| `"type": "string", "format": "node-ref"` (or `$ref: node`) | `ref_picker` |
| `"type": "number"` / `"integer"` | `field` (numeric) |
| `"type": "boolean"` | `toggle` |
| `"enum": [...]` | `select` |
| `"oneOf": [...]` | `select` (variant picker) + conditional `form` for the **server-selected** variant. The server resolves which variant is active and emits only that sub-form; changing the variant fires an action that round-trips and replaces the sub-form. Consistent with "no client logic". — UC1 Modbus RTU-vs-TCP, UC2 AI-runner picker |
| `"type": "array", "items": { ... }` (scalar items) | `field` list with add/remove |
| `"type": "array", "items": { "type": "object" }` | repeatable sub-`form` — UC3 scope stages, UC1 schedule entries |
| `"type": "object"` (nested) | nested `form` |

The full mapping lives in the React renderer and is estimated at ~150 lines including the array-of-objects and `oneOf` branches. Validation runs server-side on submit; `form_errors` response shows per-field messages. No client-side schema library required.

## Relationship to DASHBOARD.md

SDUI **is** the render target the existing resolver emits. The four dashboard kinds (`ui.nav`, `ui.template`, `ui.page`, `ui.widget`) stay — they're *authorable* component trees, stored as graph state. What changes:

- `ui.template.layout` and `ui.page.layout` were "opaque JSON"; they become typed ComponentTrees validated against the IR schema at save time.
- `ui.widget` stays as the extension-provided custom-component primitive. In IR terms it's one variant among ~30.
- `/api/v1/ui/resolve` keeps its contract but its `render` field narrows to a ComponentTree shape.
- Kinds can declare default views (proposed in the earlier brainstorm) — those views are IR trees stored on the `KindManifest` instead of as separate `ui.page` nodes.

In short: SDUI is the runtime representation; DASHBOARD kinds are the authoring/persistence layer. Same content, two views.

## Transport

- `POST /api/v1/ui/resolve` — existing; returns `render: ComponentTree` instead of `widgets: []`. Response otherwise unchanged (subscriptions, meta, dry_run).
- `POST /api/v1/ui/action` — new; action dispatcher.
- `GET /api/v1/ui/table?source_id=<id>&page=&size=&sort=&filter=` — new; table pagination.
- `GET /api/v1/ui/render?target=<node-id>[&view=<view-id>]` — new; convenience wrapper for "render this node using its default view" (requires the `KindManifest.views` extension).
- All endpoints versioned under `/api/v1/`. Capability handshake advertises `ir_version`.

Client parity per [NEW-API.md](NEW-API.md) applies: Rust client + CLI + TS client ship in the same PR as each endpoint. The existing CLI `agent ui` surface gains `resolve` output pretty-printing for the IR and an `agent ui action <handler> --args <json>` invoker for scripting.

## React renderer contract

One React package, `@acme/sdui-react`, with a single exported component:

```tsx
<Renderer tree={componentTree} actionClient={client} />
```

Internals:

1. A `componentRegistry: Record<ComponentKind, ReactComponent>` with one entry per IR variant.
2. A switch on `tree.type` dispatches to the registered component; children recurse.
3. An `ActionClient` handle issues `POST /api/v1/ui/action` and applies response-union variants to local state (patch, navigate, toast).
4. A `SubscriptionClient` wires to the existing SSE/NATS event stream and applies slot-change events to tables/charts/text nodes that bound to them.
5. Zero business logic. Components are pure mappings from IR to React. Extensions register custom renderers via `registerComponent("acme.gauge", GaugeComponent)` — exactly parallel to how backend plugins register widget types.

Size targets (excluding tests):

- Core renderer: under **800 lines** of TSX.
- Built-in component implementations: **~3000 lines** target, **4000 lines** red line.
- `diff` and `rich_text` are expected to delegate to established libraries (e.g. monaco-diff, tiptap/milkdown). Only the IR-adapter wrappers count toward the budget; the libraries themselves are allowed to be large.

If the built-in total exceeds the red line with third-party libs already delegated to, we've over-engineered.

## Size & DoS limits

- Max IR tree nodes per resolve: **2000**
- Max tree depth: **32**
- Max serialized IR tree: **2 MB** (reuse the existing render-tree cap)
- Max action handler timeout: **5 s** server-side
- Max rows per table page: **500**
- Max distinct component types per page: **60**

## Deferred

- **MCP tools for authoring.** UC2 already defines the per-user MCP endpoint. Once AI-via-CLI is proven in S1, add MCP tools that wrap the same operations (`sdui.create_page`, `sdui.validate_tree`, `sdui.describe_vocabulary`) so AI sessions don't have to shell out. Same semantics, different transport.
- **Sucrase / JSX-over-wire.** An escape hatch where the server (or AI) emits JSX source that the client transpiles + evaluates at runtime — unbounded expressivity at the cost of client-side eval, weaker validation, and a harder security story. Typed IR is the v1 contract; revisit sucrase + a `jsx_fragment` variant only if a concrete authoring scenario proves the ~32-component vocabulary + `custom` escape hatch insufficient.
- **Offline cache**. v2. Requires IR to carry TTL hints and actions to carry optimistic specs richer than a single patch.
- **Partial / subtree re-resolve.** `$page`-state changes (e.g. chart zoom) currently re-issue `/ui/resolve` for the whole page. For pages with 200 widgets this is wasteful even with caching. v2 extends the resolver with a `subtree_root` parameter or lets the server emit a `patch` response from a re-resolve, replacing only the changed subtree. Not urgent for S1's small pages; revisit if real-world trees grow large.
- **IR → RFW translator** (Flutter). Out of scope. The IR is designed to be portable, but v1 ships only React.
- **Visual IR authoring** (drag-drop tree editor). The dashboard builder and flow canvas ship as framework-provided bespoke React (see the "zero client code" carve-out); a fully visual IR editor for arbitrary trees is Studio Stage 4+ work.
- **Server-side A/B tests, feature flags, analytics hooks** on IR emission. Tractable later; not v1.
- **First-class `schedule_grid`, `floorplan`, `node_graph` components.** v1 serves these via the `custom` escape hatch. Promotion to first-class is a later call if the same domain-specific renderer shows up in enough extensions to justify framework ownership.

## Crate layout

Following [CODE-LAYOUT.md](CODE-LAYOUT.md):

```
/crates
  /ui-ir                   # Domain — the component IR + JSON Schema + versioning
  /dashboard-transport     # (existing) — gains /action + /table + /render endpoints
  /dashboard-runtime       # (existing) — unchanged
```

No `ui-ir-runtime` needed — the existing binding engine already does the work. The IR crate holds only types, schema, and a pure-function builder API.

```
/clients/react
  /sdui-react              # New package — the Renderer + ActionClient + component registry
```

## Dependencies on existing work

- `/crates/query` — reused for `table` source queries and for `ref_picker` lookups.
- `/crates/auth` — handler registry enforces scopes via `AuthContext`.
- `/crates/messaging` — subscription plan feeds table/chart live updates.
- `/crates/dashboard-runtime` — binding engine, cache-key, context stack all reused as-is.
- `/clients/rs` + `/clients/ts` — grow typed IR mirrors per [NEW-API.md](NEW-API.md).

## Acceptance criteria

Grounded in the three use cases in [USE-CASES.md](../usecase/USE-CASES.md):

- **S1 AI-authoring test:** a locally-running AI CLI session (Claude Code or similar), given only the output of `agent --help-json` + `agent ui vocabulary -o json`, produces a working `ui.page` node by shelling out to `agent nodes create` + `agent slots write`, with the resulting page passing `agent ui resolve --dry-run` on first or second attempt. No MCP, no special endpoints, no custom AI plumbing.
- **UC1 falsification test:** a React app with zero BACnet-specific code renders a working BACnet discovery page — list devices, click scan, add a discovered device to the graph, see it live-update — driven entirely by IR the backend emits.
- **UC2 falsification test:** the same bundle, zero GitHub-specific code, renders a per-user PR review card for UC2's nightly flow — with a `diff` component showing the PR changes, inline `button`s for approve / request-changes / reject, and the action round-trips to the GitHub extension's handlers.
- **UC3 falsification test:** the same bundle, zero scope-specific code, renders UC3's scope-plan daily board — rows of scopes with state badges, per-row approve/reject buttons, live updates as the flow advances stages via subscriptions.
- A plugin author ships a new kind with `views: [{template: ComponentTree, title: "Overview"}]` in their manifest; clicking an instance in Studio shows the view. Zero framework code changes.
- `POST /api/v1/ui/action` with an unregistered handler returns 404 and the correct CLI exit code.
- Table component renders 10k-row result with server-side paging, sort, and filter — each interaction is one HTTP round-trip; client virtualises row rendering internally; no full re-renders.
- Form component derives every field type from a JSON Schema via the mapping table above, including `oneOf` (variant picker) and array-of-objects (repeatable sub-form). Multi-variant settings (UC1 Modbus RTU-vs-TCP, UC2 AI-runner picker) render without custom code.
- `rich_text` input round-trips markdown through a scope's `description` slot; server persists verbatim; re-render produces identical content.
- `diff` component renders a unified diff from a handler response; inline-comment actions round-trip to the handler.
- `custom` escape hatch: a plugin registering `renderer_id: "acme.floorplan"` gets its component rendered; unknown `renderer_id` degrades to a neutral stub without crashing the tree.
- IR version handshake refuses to render a v2 tree against a v1 client and surfaces a clean capability-mismatch error.
- Optimistic action hints produce a visible UI patch under ~16 ms of the button click; the authoritative response confirms or overrides within the round-trip.
- Streaming: a handler returning `{type: "stream", channel}` + subsequent NATS patches renders incremental content on a `text` or `markdown` component without the server re-emitting the full tree.
- `$page` chart-range round-trip: zooming a `chart` writes `$page.chart_range`; the next `/ui/resolve` returns denser data for the zoomed window.
- Zero occurrences of domain-specific strings (`site`, `device`, `point`, `ticket`, `pr`, `scope`) in `/crates/ui-ir` or `/clients/react/sdui-react`.

## Milestones

| M | Deliverable |
|---|---|
| ~~S1~~ ✅ | `crates/ui-ir` lands with ~15 components (layout + display + `button` + `form` + `table` + `diff` + `rich_text`); JSON Schema emitted; versioning scaffolding. New CLI `agent ui vocabulary` dumps the IR schema for AI authoring. First demo: a locally-running Claude Code / OpenCode session authors a working dashboard end-to-end via `agent nodes create` + `agent slots write` + `agent ui resolve --dry-run`, shelling out only — zero new endpoints. |
| ~~S2~~ ✅ | `/api/v1/ui/action` + handler registry + `auth`-gated dispatch. `toast` / `navigate` / `full_render` / `form_errors` / `download` / `stream` / `none` response variants. |
| ~~S3~~ ✅ | `/api/v1/ui/table` paginated endpoint backed by the query engine. **`custom` escape hatch ships here** — client-side renderer registry + the `{type:"custom"}` IR variant + fallback stub. TypeScript + Rust client parity. |
| ~~S4~~ ✅ | `frontend/src/sdui/` — `SduiProvider`, `Renderer`, `useActionResponse`, 16 component implementations incl. `custom` registry. Dashboard page replaced with live `ui.page` browser. Route `/ui/:pageRef` renders any authored page. Resolver fast-path in `crates/dashboard-transport` so `layout`-slot trees bypass the M4 widget resolver. |
| **S5** | `/api/v1/ui/render?target=<id>` + `KindManifest.views` extension; clicking any node in Studio shows its default view. Zero authored pages for the 90% case. Also: **persist graph to disk** so authored pages survive server restart (currently all nodes are in-memory only). |
| S6 | Remaining components (chart, sparkline, wizard, tree, timeline, drawer, ref_picker, rich-text editor via tiptap, markdown streaming). Streaming-subscription wiring on `text` / `markdown` / `code` / `timeline`. `$page`-state round-trip for chart zoom/range. |
| S7 | Optimistic action hints end-to-end. Capability-handshake enforcement (`ir_version`). Size/DoS limits tested. Full acceptance suite green across all three use cases. |

## One-line summary

**A versioned component IR emitted by the resolver and rendered by a 500-line React package; every screen in every client comes from the backend, every plugin ships UI with its data model, and the React app never learns what the domain is.**
