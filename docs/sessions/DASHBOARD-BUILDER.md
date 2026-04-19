# Dashboard Builder — Scope

A visual surface for authoring `ui.page` nodes without hand-writing JSON. Power users get a Monaco-based JSON/YAML editor with live preview. Non-devs *may* get a template picker + form generator — contingent on a specific Stage 0 test result (see below). Plugin-authored `KindManifest.views` editing is deferred to the last stage, intentionally.

**Not a canvas.** A free-form pan-zoom builder is explicitly out of v1 (Stage 4). Nobody on the team has committed to shipping it. Read the canvas stage as "if we decide it was needed, here's the starting design — otherwise v1 is the final shape." Stop pretending "we'll add it later" without a date. If you need a canvas, don't use this project in 2026.

**Prove the idea is rock-solid before adding more of anything.** S6 shipped 24 IR components; S7 shipped optimistic actions, capability handshake, DoS limits, and three falsification tests. The question is not "do we have enough widgets" — it's "does the end-to-end authoring loop actually hold up under real use." Stage 0 is a proof pass, not a ship pass. New widgets only land when a concrete authored page hits a wall we can't route around.

Authoritative references:

- [SDUI.md](../design/SDUI.md) — IR vocabulary, binding grammar, render/resolve semantics
- [DASHBOARD.md](../design/DASHBOARD.md) — node kinds, binding sources, subscription plan
- [UNDO-REDO.md](../design/UNDO-REDO.md) — revision-log machinery (Stage 3 consumes this)
- [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md) — `ui.*` kinds live in the graph like anything else
- [docs/testing/DASHBOARD.md](../testing/DASHBOARD.md) — the CLI-based authoring path, which the builder sits alongside

---

## Named user

**Stage 1 target user: the power user on this team.** Someone who understands the IR, uses the CLI daily, and wants to author or tweak a layout without cycling through `agent ui resolve --dry-run` in a terminal. The single concrete gap Stage 1 closes is "you can't see whether a hand-authored tree has broken bindings without running the full render." That gap is real, the fix is small, and the user exists today.

**Stage 2 target user: unnamed.** "Non-devs" is a demographic, not a user. Stage 2 ships only if a Stage 0.3 LLM-authoring test proves that Stage 1 + an LLM sidebar *doesn't* handle the same cases with less code. See Stage 2's "existence gate" for the specific pass/fail criterion.

**Stage 3–5 target user: the Stage 1 user, plus undo/redo and optional canvas affordances.** Same person; expanded surface.

**Stage 6 target user: plugin authors.** Deferred indefinitely — the existing YAML-manifest path works, and the three other authoring modes (CLI/LLM, plugin YAML, hand-written JSON) cover the plugin-view case without new UX.

If this project ever acquires an external customer whose need reshapes the above, revisit. Until then, honest about what's being built and for whom.

---

## Authoring modes the builder coexists with

SDUI supports three authoring modes today. The builder is a fourth — for the power user named above. It is not a replacement for any of them.

| Mode | Artifact | Who | When |
|---|---|---|---|
| CLI / AI | `agent nodes create … ui.page` + `agent slots write … layout '<json>'` | LLM sessions, scripts, power users | Shipped (S1+) — see [testing/DASHBOARD.md](../testing/DASHBOARD.md) |
| YAML in plugin manifests | `views:` entry on a `KindManifest` | Plugin authors shipping "every instance of kind X has a default view" | Shipped (S5) |
| Hand-written JSON | direct `agent slots write …/layout` | humans in any editor | Shipped |
| **Builder (this doc)** | same `ui.page.layout` slot | power users | v1 (Stages 1–4) |

The builder produces artifacts the other three modes already understand. No new storage, no new format, no migration.

---

## Client API coverage (TS client — [`clients/ts/src/domain/`](../../clients/ts/src/domain/))

The builder lives in the frontend and talks to the agent exclusively through the TS client. Anything this scope needs must land in `clients/ts/src/domain/*.ts` + `clients/ts/src/schemas/*.ts` with tests, same discipline as every other endpoint (see [NEW-API.md](../design/NEW-API.md) § five-touchpoint rule).

**Already shipped in TS client (Stage 1 can use today):**

| Method | Endpoint | File | Notes |
|---|---|---|---|
| `client.ui.resolve(req)` | `POST /api/v1/ui/resolve` | [`domain/ui.ts:27`](../../clients/ts/src/domain/ui.ts) | Includes `dry_run` parameter; the validator path |
| `client.ui.render(target, view?)` | `GET /api/v1/ui/render` | [`domain/ui.ts:51`](../../clients/ts/src/domain/ui.ts) | For kind-view preview |
| `client.ui.action(req)` | `POST /api/v1/ui/action` | [`domain/ui.ts:34`](../../clients/ts/src/domain/ui.ts) | For buttons / forms in the preview |
| `client.ui.table(params)` | `GET /api/v1/ui/table` | [`domain/ui.ts:41`](../../clients/ts/src/domain/ui.ts) | For `table` components in the preview |
| `client.ui.nav(rootId)` | `GET /api/v1/ui/nav` | [`domain/ui.ts:21`](../../clients/ts/src/domain/ui.ts) | For sidebar context |
| `client.slots.writeSlot(path, slot, value)` | `POST /api/v1/slots/write` | [`domain/slots.ts:18`](../../clients/ts/src/domain/slots.ts) | **Missing `expected_generation`** — see Pre-Stage-1 gaps |
| `client.kinds.list()` | `GET /api/v1/kinds` | [`domain/kinds.ts:6`](../../clients/ts/src/domain/kinds.ts) | Slot-name lookup for binding validation |
| `client.events.subscribe()` | SSE | [`domain/events.ts`](../../clients/ts/src/domain/events.ts) | Preview live ticks, per [`useSubscriptions.ts`](../../frontend/src/sdui/useSubscriptions.ts) |
| `client.nodes.*` | `GET/POST /api/v1/nodes/*` | [`domain/nodes.ts`](../../clients/ts/src/domain/nodes.ts) | For "create a new `ui.page`" during `/dashboards/new` |

**Pre-Stage-1 gaps (blocking):**

| Method | Endpoint | Why needed | Work required |
|---|---|---|---|
| `client.ui.vocabulary()` | `GET /api/v1/ui/vocabulary` | Monaco pulls the component-union JSON Schema from here; vocabulary palette does too. **Endpoint does not exist on the backend yet**, despite being referenced across SDUI.md, CLI.md, and [testing/DASHBOARD.md](../testing/DASHBOARD.md). This is a third substrate gap on par with the `dry_run` and OCC gaps | Backend: route + handler emitting `ui_ir::Component`'s `schemars` schema. Client: method + schema test. CLI: `agent ui vocabulary` subcommand + CommandMeta entry + fixtures — the CLI references it in docs but the command isn't implemented |
| `client.slots.writeSlot` with `expected_generation` | `POST /api/v1/slots/write` with new `expected_generation` param | OCC guard (§OCC invariant). Mismatch returns 409; builder's conflict banner triggers off that status | Backend: add param + 409 path. Client: optional param on `writeSlot`. Fixture gate test |

**Stage 2 additions (only if the existence gate says Stage 2 ships):**

| Method | Endpoint | Work required |
|---|---|---|
| `client.ui.templates.list()` | `GET /api/v1/ui/templates` | Stage-2 backend endpoint returning the `templates/*.json` library |
| `client.ui.templates.get(id)` | `GET /api/v1/ui/templates/:id` | Per-template full record |

**Stage 3 additions (ride Phase 2 of UNDO-REDO):**

| Method | Endpoint | Notes |
|---|---|---|
| `client.nodeSlots.undo(id, slot, {expected_head})` | `POST /api/v1/nodes/:id/slots/:slot/undo` | Generalised from `NodeSettingsService`; table in [UNDO-REDO.md](../design/UNDO-REDO.md) Phase 2 |
| `client.nodeSlots.redo(id, slot, {expected_head})` | `POST /api/v1/nodes/:id/slots/:slot/redo` | " |
| `client.nodeSlots.revert(id, slot, {to, expected_head})` | `POST /api/v1/nodes/:id/slots/:slot/revert` | " |
| `client.nodeSlots.revisions(id, slot, params)` | `GET /api/v1/nodes/:id/slots/:slot/revisions` | History panel |
| `client.nodeSlots.at(id, slot, revId)` | `GET /api/v1/nodes/:id/slots/:slot/at/:revId` | Materialised value at a revision |

**Stage 4 + Stage 5:** no new client methods. Canvas uses Stage 1's methods. Plugin-view editing is deferred.

**Rule:** no builder code hits `fetch` directly. Every network call goes through a typed method on `AgentClient`. If the method doesn't exist, add it to the client first, per NEW-API.md's five-touchpoint rule (Rust handler + Rust client + CLI command + CommandMeta + TS client + fixtures). No shortcuts — especially not in the builder, which is the first consumer that will make "just this one" exceptions tempting.

---

## Goals

- **Power users edit JSON/YAML** with Monaco + schema intellisense and live preview. The JSON editor is the foundation.
- **Logic and visual separable.** Model, store, and validation are pure and testable without a DOM; canvas/panel components are pure views over that store.
- **Reuse the existing SDUI renderer** for preview — the builder's output is the same `ui.page.layout` the runtime resolves.
- **Binding safety.** Invalid bindings (`$page.missing`, wrong type, unknown slot) fail at save time with a clear error, not at render. The validator is `POST /ui/resolve` with `dry_run: true`. **Known gap:** as of S7, `dry_run` only runs `serde_json::from_value::<ComponentTree>` — it catches structural errors but does not resolve bindings or check slot existence ([`crates/dashboard-transport/src/resolve.rs`](../../crates/dashboard-transport/src/resolve.rs) `handler`). Extending `dry_run` to emit position-bearing binding/slot errors is a **pre-Stage-1 fix**, not a builder concern.
- **Preview ticks live** via the subscription plan the resolver already emits — the same `useSubscriptions.ts` the runtime uses.

## Non-goals

- **Canvas / pan-zoom / free positioning.** Stage 4, with no ship commitment.
- **Drag-from-palette.** Widgets are added via the JSON editor or template slots, not by dropping.
- **Real-time collaborative editing.** One editor at a time.
- **Silent concurrent-write resolution.** If a CLI write lands while the builder is open, the conflict banner is **non-dismissable** and forces a reload with the current tree before the user can keep editing. See "OCC invariant" below.
- **Runtime state authoring** (widget inbox contents, cached values). We version authored state only.
- **A template authoring UI.** Any templates that ship (Stage 2) are maintained by this team as static JSON in-repo. Plugin-author-contributed templates are a v2 question.
- **Plugin-view editing in v1.** Stage 5 exists in the doc as a record; no work planned until a concrete need emerges.
- **Replacing the CLI authoring path.** `agent ui` stays the AI-first surface. Builder is a parallel affordance.

---

## Separation of concerns (hard rule)

Logic and visual code are separated from the first line of Stage 1 onwards. This is a **structural** rule, not a preference — breaking it is the single most common way builder UIs turn into unmaintainable balls of state-plus-JSX, and once it's broken the code never comes back clean.

**Layers, top to bottom:**

1. **`model/`** — pure TypeScript. No React, no DOM, no browser globals. Parses and types the draft; walks trees; validates bindings; normalises JSON ↔ YAML. Runnable in Node with `vitest` — no jsdom. Every function is a pure input → output transformation.
2. **`store/`** — zustand (or similar) state container. No React-Query, no fetching, no side effects beyond the store's own bookkeeping. Depends on `model/`. Testable headless by driving actions and asserting state shape.
3. **`persistence/`** — side-effectful glue: `fetch`, debouncing, OCC-guard dispatch, conflict-banner dispatch. Depends on `store/` and the agent client. Testable with a mocked `AgentClient`. No UI.
4. **`preview/`** — React components that read the store + agent event stream and project. Depends on `sdui/Renderer.tsx`. No business logic, no validation, no save logic. If a `preview/` component imports from `persistence/`, that's a bug.
5. **`panels/`** — React UI for one concern each (editor pane, vocabulary palette, validation list, page-state editor, history panel). Depends on `store/` (read) + action dispatchers exposed from `store/` or `persistence/`. **Never** imports from `canvas/`.
6. **`canvas/`** — (Stage 4 only) React UI for free-form layout. Same rules as `panels/`. **Never** imports from `panels/`.

**Enforcement:**

- ESLint rule `no-restricted-imports` in the builder package: `model/` cannot import React; `store/` cannot import from `panels/` or `canvas/`; `panels/` and `canvas/` cannot import from each other; `preview/` cannot import from `persistence/`.
- Build script rejects any file in `model/` that `grep -l "react\|jsx\|dom"` matches.
- Every `store/` and `model/` module ships with unit tests that run under `vitest run` without jsdom. If a test needs jsdom, it doesn't belong in those layers.
- Every `panels/` and `preview/` component ships with at most a smoke render test — the real logic is already covered in `store/` and `model/`. Component tests exist to catch wiring, not correctness.

**Why this matters concretely for this doc:** three stages (1, 2, 3) and the deferred Stage 5 all mutate the same `DraftPage` in the store. If the form generator (Stage 2) and the Monaco editor (Stage 1) both have their own copy of state, switching between them loses work. If history (Stage 3) adds its own revision-tracking code inside `panels/HistoryPanel.tsx`, the CLI round-trip test can't exercise it. The only way this stays coherent as the surface grows is strict layering.

**Scope discipline:** any time a new panel needs "just a bit of state" that isn't yet in the store, the state goes in the store first. Any time a model concept ("a binding," "a draft," "a validation issue") needs describing, it goes in `model/` first. If you can't write the `model/` type or `store/` action without already knowing what the UI looks like, you haven't understood the domain yet — stop and figure it out in isolation.

---

## OCC invariant (applies to every stage that writes)

Stage 1 onwards touches `ui.page.layout`. The CLI also touches the same slot. For the builder's "someone else edited this" UX to be sound, every write must increment the generation counter the builder watches. Two failure modes to eliminate up front:

1. **A CLI write during a builder session.** Builder's `lastSavedGeneration` goes stale. Next autosave compares against stale value, sees mismatch, shows banner. **If the user dismisses the banner and continues editing, the following autosave overwrites the CLI's work.** Resolution: the banner is non-dismissable. The only actions are "reload (discard local edits)" and "export local edits to clipboard." No "keep editing."
2. **Debounced autosaves from the builder's own keystrokes.** Each write bumps the generation; builder's next debounce re-reads the new generation before its next write. This is standard OCC and is fine once the backend supports `expected_generation`.

The pre-Stage-1 fix is adding `expected_generation` to `POST /api/v1/slots/write` (or equivalent). The UI invariant (non-dismissable conflict banner, forced reload) is a builder-side rule that lands with Stage 1.

---

## Stage 0 — Prove the idea is rock-solid (no new code)

The point: before building a builder, prove the substrate we already shipped actually holds up when a human tries to use it for real. CI tests compile-correctness; CI does not test whether a non-dev can drive `agent slots write` to a working dashboard, whether the preview ticks under real SSE load, whether error shapes are useful, or whether `$page` round-trips feel right.

Stage 0 is exclusively exercises against shipped code. Zero new features. Every item either passes or surfaces a concrete bug that becomes a pre-Stage-1 fix.

### 0.1 — Authoring walkthroughs

Three flavours of session, not one. A scripted walkthrough proves the script is followable; it does not prove authoring is possible. Run each flavour against the three acceptance-criteria use cases from [SDUI.md](../design/SDUI.md#acceptance-criteria) (BACnet-shape discovery, PR-review-shape card, scope-plan-shape board):

- [ ] **Scripted (3×).** A reproducible `docs/testing/walkthroughs/<uc>.md` script + short recording. Pass: following the script verbatim produces a working, live-updating page in under 10 minutes.
- [ ] **Unscripted (3×).** A contributor with access to the CLI and `agent ui vocabulary -o json`, a one-sentence natural-language goal, no step-by-step. Pass: working page in under 30 minutes without slack-messaging the author. Record where they got stuck; every stuck point is a doc PR or a substrate fix.
- [ ] **Exploratory (3×).** A contributor handed a graph they didn't populate ("build me a dashboard for this flow"). They must figure out what widgets apply to the data shape without being told. Pass: working page within 45 minutes, plus a list of "I expected X to work and it didn't" moments — those are the real bug report.

The scripted runs produce documentation. The unscripted + exploratory runs produce the actual usability signal. Do not skip the latter two because they're harder to organise.

### 0.2 — Substrate stress tests

- [ ] **1000 nodes.** Heartbeat-like behaviour across 1000 nodes, a `table` over them, a `chart` reading one. Measure: resolve latency (target <100ms), sustained SSE event rate, React render time per tick. Anything hostile is a pre-Stage-1 fix.
- [ ] **Chart `$page.chart_range` round-trip.** Zoom 20 times; confirm the server emits distinct subscription plans each time.
- [ ] **Subscription-plan fidelity.** Three tables with overlapping queries; confirm per-table invalidation (S5 polish) updates only the relevant tables via React DevTools Profiler.
- [ ] **Restart survival.** `agent run --db /tmp/agent.db`, author a page, kill the agent, restart, confirm the page + heartbeat + slot values all come back. SQLite infrastructure exists; nobody has actually exercised it through a page-authoring round-trip.
- [ ] **CLI write during builder-style session.** Simulate the OCC failure mode above — start an "authoring session" (any long-lived connection), land a CLI write to the same layout, confirm the failure mode is caught before the builder exists to prevent it naturally.

### 0.3 — AI-authoring bar (existence gate for Stage 2)

The CLI is the LLM surface per [CLI.md § "LLM-friendly surface"](../design/CLI.md#llm-friendly-surface). This sub-stage is also the **existence gate for Stage 2** — see Stage 2.

- [ ] **Three runs, not one.** Different LLMs (at least two vendors), different prompts, different starting graph state. Each LLM receives only the outputs of `agent schema --all -o json`, `agent kinds list -o json`, `agent ui vocabulary -o json` plus a natural-language goal.
- [ ] Each run produces a working page in ≤ 3 `slots write` attempts with `--dry-run` as the only feedback loop. Record transcripts.
- [ ] **Stretch run.** Rerun the three Stage 2 seed-template use cases (SingleStat, KpiGrid, TableWithFilters, ChartWithDropdown, MasterDetail) through Stage 1's Monaco + a local LLM sidebar. Time each.

**Existence gate for Stage 2:** if an average user with Stage 1's Monaco + an LLM sidebar can build all five seed-template use cases in under 15 minutes each, Stage 2's form generator is code that shouldn't be written. Skip straight to Stage 3. If the runs fall over on any of the five, Stage 2 ships with the seeds that actually failed, not all five by default.

### 0.4 — Error-shape audit

Every failure the author can hit must surface a useful error:

- [ ] Unknown component `type` → `POST /ui/resolve` returns `{location, message}` with the invalid variant name. **Known to hold.**
- [ ] Missing required field → error pins to the specific path. **Known to hold.**
- [ ] **Unknown binding path** (e.g. `{{$page.missing}}`, `{{$target.not_a_slot}}`) on `dry_run` → position-bearing `{location, message}` error. **Known to fail today** — `dry_run` only parses the tree, no binding evaluation. This is the pre-Stage-1 fix: extend `dry_run` to walk the tree, substitute bindings via the existing engine, and collect each failure as a `ResolveIssue`.
- [ ] Subscribed table with zero-match query → empty subscription plan, clear (no false error).
- [ ] Binding to a non-existent slot at render → `Dangling` stub renders; page continues.
- [ ] Handler not registered → `ui action` returns 404 with stable `code: "not_found"`.
- [ ] DoS limit hit → 413 with stable `what` tag (already covered by `tests/limits.rs`; confirm CLI surfaces it cleanly).

Do not start Stage 1 until `dry_run` emits position-bearing binding errors. Monaco squigglies without this are noise.

### 0.5 — Documentation round-trip

- [ ] A second contributor (not the author) follows [docs/testing/DASHBOARD.md](../testing/DASHBOARD.md) cold, end-to-end, and ships a working page. Every confusing step is a doc PR, not a re-explain.
- [ ] Same with [docs/testing/FLOW.md](../testing/FLOW.md).

**Proves:** the substrate supports the authoring story. If any check finds worse than minor polish, fix the substrate, not the builder.

**Does not prove:** vocabulary is sufficient for every page a user will want. Only use answers that.

---

## Stage 1 — JSON/YAML editor with live preview

**Goal:** a power user can paste or author a layout, see errors inline, see the result ticking live.

**This stage is the whole v1 bet for power users.** Everything after Stage 1 is either UX polish (Stage 2, provisional), history plumbing (Stage 3), speculative (Stage 4 canvas), or deferred (Stage 5 plugin views). If Stage 1 doesn't feel right, nothing after it matters.

**Prerequisites (blocking):**

1. `POST /ui/resolve --dry-run` returns position-bearing errors for unknown bindings, missing slots, unresolved `$page` keys. Currently parse-only; see §Goals.
2. Slot writes accept `expected_generation` (or equivalent OCC guard). Currently absent. Without it, the non-dismissable conflict banner in §OCC invariant can't be correctly triggered.
3. `GET /api/v1/ui/vocabulary` exists and the TS client exposes `client.ui.vocabulary()`. Referenced across SDUI.md, CLI.md, and the testing docs but **not actually shipped** — see §Client API coverage. Monaco schema load + vocabulary palette both depend on this. Add the endpoint + CLI command + client method + fixture as one unit; don't split across stages.

All three are small changes to shipped code. Land all three before any builder UI work.

Layout (logic/visual split, hard rule: `canvas/` and `panels/` never import from each other; both read `store/`, `store/` reads `model/`):

```
frontend/src/features/dashboard-builder/
├── model/                  ← pure, no React, unit-tested headless
│   ├── types.ts            ← DraftPage
│   ├── bindings.ts         ← $stack/$self/$user/$page parser + typecheck
│   └── page-state.ts       ← page_state_schema helpers
├── store/                  ← zustand, no DOM
│   ├── builder-store.ts    ← draft, dirty, lastSavedGeneration, validationErrors
│   └── persistence.ts      ← debounced save via slots-write + generation guard
├── preview/
│   └── LivePreview.tsx     ← wraps sdui/Renderer.tsx + useSubscriptions
├── panels/
│   ├── EditorPane.tsx      ← Monaco, JSON mode + YAML mode toggle
│   ├── VocabularyPalette.tsx ← drop-down + search over `agent ui vocabulary`
│   ├── ValidationList.tsx  ← errors pinned to editor positions, click-to-jump
│   └── PageStateEditor.tsx ← declares/renders page_state_schema
├── DashboardBuilderPage.tsx ← split-pane shell
└── __tests__/              ← model/ and store/ tests, no jsdom
```

- [ ] Routes: `/dashboards/new`, `/dashboards/:nodeId/edit`.
- [ ] Monaco editor with the SDUI component-union JSON Schema pulled live from `agent ui vocabulary`. Autocomplete + inline red squigglies for unknown component types, missing required fields, wrong enum values.
- [ ] YAML mode toggle — parses to the same document model; save normalises to JSON.
- [ ] Validator is `/ui/resolve` with `dry_run: true`. Called on every debounce (500ms). Errors render in the gutter and in `ValidationList`. No new endpoint.
- [ ] Preview pane: resolves the draft, wraps in the existing `SduiProvider`, subscribes via `useSubscriptions`. No custom poll loop, no renderer changes.
- [ ] Debounced save via `POST /api/v1/slots/write` with `expected_generation` (prereq 2). Generation mismatch surfaces the non-dismissable conflict banner (§OCC invariant). The banner's only actions are "reload (discard local)" and "export local to clipboard."
- [ ] Vocabulary palette: searchable reference pane over every component + one-click "insert skeleton" that pastes a valid-minimum stub at the cursor. Pulls from `agent ui vocabulary` so it stays in sync with what the server accepts.

**Proves:** the editor infrastructure works. The AI authoring path and the human editor path produce identical artifacts.

---

## Stage 2 — Template picker + form generator (probationary)

**Existence gate:** ship only if Stage 0.3's LLM+Monaco stretch run falls over on any of the five seed use cases. If it doesn't, cancel Stage 2 and go straight to Stage 3.

Rationale: a form generator is ~2000 lines of React that codifies what an LLM can infer from the vocabulary schema in real time. In 2026, if Stage 1's Monaco + a local LLM sidebar handles the seed use cases inside a 15-minute budget, Stage 2 is form-UI engineering for its own sake. The team builds form UIs out of habit; the platform was designed from first principles to make them unnecessary. Test the hypothesis before committing to the engineering.

If the gate passes (i.e. Stage 2 is needed):

**Prerequisites (blocking):** the seeds below reference IR components that do not exist today (`kpi`, `select`, `field`, `toggle`). Ship first:

- [ ] `kpi` — big-number tile (label + value + optional intent + optional inline `sparkline`).
- [ ] `select` — standalone dropdown writing to `$page.*`.
- [ ] `field` — standalone text input writing to `$page.*`.
- [ ] `toggle` — standalone boolean writing to `$page.*`.

Four variants, same shape as the S6 batch. No speculative expansion beyond the four seeds actually need.

Stage 2 proper:

- [ ] **Template source**: curated JSON in `crates/dashboard-nodes/templates/*.json`. Each file: `{id, version, display_name, category, thumbnail, parameter_schema, expand}`. **Not** a new kind; not a graph concept; static library the backend serves.
- [ ] `GET /api/v1/ui/templates` → list; `GET /api/v1/ui/templates/:id` → full record.
- [ ] **Seeds (ship only those that failed the Stage 0.3 gate, not all five by default):**
  - `SingleStat` — one `kpi` with title, optional intent threshold.
  - `KpiGrid` — N-up `kpi` tiles over a list binding.
  - `TableWithFilters` — `select` + `field` + `toggle` filter row writing to `$page.*`, `table` bound to those `$page` keys.
  - `ChartWithDropdown` — `chart` + `select` writing to `$page.chart_range`.
  - `MasterDetail` — list on the left, detail panel on the right, selected row in `$page.selected`.
- [ ] Form generator: walks the template's parameter JSON Schema, emits field components per type. **Every field type is a registered component; no template-specific forms.**
- [ ] Form ⇔ JSON toggle edits the same `DraftPage`. Form is a view over JSON, not a separate model. Switching preserves unsaved state.
- [ ] "Revert to defaults" restores args to template defaults without touching page-state.

**Deferred until a specific need emerges:** `template_origin` field on pages (for upgrade-offer UX). The doc previously said "embed `{id, version}`" but the lifecycle is unspecified — no deprecation policy, no tombstone format, no fallback when the id is renamed or removed. Do not add the field until those are designed. When templates are reshuffled, pages forked from them simply remain as their expanded tree — no upgrade prompt, no dangling pointer.

**Proves (if shipped):** templates are a convenience over a real document. A non-dev can ship a working dashboard end-to-end.

---

## Stage 3 — History UI for dashboards

**Open problem (resolution already exists; this stage consumes it):** `ui.page` nodes are graph nodes, not flows. The shipped `flow_revisions` table is flow-specific. But `node_setting_revisions` already exists in migration v4 ([`crates/data-sqlite/src/migrations.rs:128`](../../crates/data-sqlite/src/migrations.rs)) and is the right home for `ui.page.layout` history. Phase 2 of UNDO-REDO ([`UNDO-REDO.md:66-79`](../design/UNDO-REDO.md)) is pending — `NodeSettingRevisionRepo` + `NodeSettingsService` have not been implemented yet.

**Resolution: Stage 3 rides Phase 2 of UNDO-REDO.** Extend the pending `NodeSettingsService` to cover arbitrary versioned slots (generalise the domain type — `NodeSlotRevisionService` or similar; the table name stays). Dashboard history becomes the first concrete user; plugin node-settings history ships alongside.

Dependencies:

- Phase 2 lands: repo trait + sqlite impl + service + `POST /nodes/:id/slots/:slot/{undo,redo,revert,revisions}` endpoints.
- **All writes to `ui.page.layout` go through the service**, not direct `POST /api/v1/slots/write`. This is where the OCC guard actually lives — Stage 1's OCC prereq is the transport-layer minimum, but the service is the long-term owner. The CLI's `agent slots write` gets re-wired through the service so writes from either channel increment the same generation counter and produce the same revision entries.

Once Phase 2 is landed:

- [ ] Ctrl+Z / Ctrl+Shift+Z call the undo/redo endpoint with `expected_head`.
- [ ] History panel — same component as flows history if it lands first.
- [ ] Revert to any revision. Cross-scope undo warning per [UNDO-REDO.md § "Cross-scope undo is intentionally independent"](../design/UNDO-REDO.md).

**Proves:** dashboards have the same history affordances flows do, and the CLI + builder share an OCC story.

---

## Stage 4 — Canvas mode (speculative; no ship commitment)

This stage exists for the record. Nobody on the team has committed to shipping it. If you need a canvas today, this project is not what you want.

Pursue only if Stages 0–3 ship *and* real authored pages hit layout shapes the JSON editor + (optional) templates can't express. If pursued:

- [ ] `canvas/` directory with `PanZoomCanvas.tsx` + `GridCanvas.tsx`, selected by a `mode` field on the draft.
- [ ] Bindings-as-a-graph overlay on hover — reuses the subscription plan already returned by `/ui/resolve`.
- [ ] Widget palette: click-to-add with `findNextFreeSlot`, not drag-from-palette.

Design doc happens when — and only when — we have evidence it's needed.

**If the bet is wrong** (the template-less JSON editor doesn't cover real layouts), v1 is the final shape. Don't pretend "we'll add it later."

---

## Stage 5 — `KindManifest.views` editing (deferred indefinitely)

Plugin authors edit kind-default views today via YAML manifest + agent reload. That works. The three existing authoring modes (CLI/LLM, plugin YAML, hand-written JSON) cover plugin-view authoring without any new UX. Until a concrete need emerges — a specific plugin author stuck on a specific workflow that can't be solved by improving YAML affordances — do not build this.

If a need *does* emerge, these are the knobs:

- **A. Runtime-only edits, with an explicit "Export view YAML" button** that dumps to clipboard + file. Plugin author commits manually. Agent restart reverts to YAML. Uses a non-dismissable "edits are runtime-only — export before closing" banner to make the limitation visible.
- **B. Refuse runtime edits.** Builder opens plugin-shipped views read-only. Consistent with how `settings_schema` works.
- **C. File-system round-trip.** Builder writes back to the original YAML path. Requires agent write access; breaks containerised deploys.

Option A is the only one that doesn't make the builder structurally incapable of touching the platform's primary authoring artifact, provided the "unsaved — export" banner is non-dismissable (same shape as the OCC conflict banner in Stage 1). Option B is a failed product for anything but consumption; don't default to it "because it's simpler."

**None of this is scheduled.** Leave it in the doc as a placeholder.

---

## Data model (v1, no schema migration)

Dashboards are `ui.page` nodes with a `layout` slot. The builder edits through the existing write paths:

- **Stage 1:** direct `POST /api/v1/slots/write` with `expected_generation`.
- **Stage 3 onwards:** through `NodeSlotRevisionService` (Phase 2 of UNDO-REDO), which owns the generation counter and appends to `node_setting_revisions` on every write.

No new tables.

Pages created by the builder and pages hand-authored by the CLI are indistinguishable — same slot, same shape. Pages defined in plugin YAML are read-only in the builder; any "fork to edit" affordance is deferred along with Stage 5.

---

## Risks and chosen mitigations

| Risk | Mitigation |
|---|---|
| Stage 0 surfaces a substrate bug that blocks Stage 1 | That's the point. Fix in-place |
| Stage 0.3 existence gate passes — Stage 2 is cancelled — and someone later complains the UX is "too technical" | That complaint comes with a specific user. Until then, Stage 1 is the shape |
| Vocabulary turns out to be thin once real authoring starts | Add widgets on demand as concrete pages hit walls. The S6 ship list was large because falsification tests needed it; grow from now via real use |
| No evidence for the "80% fit curated templates" claim if Stage 2 does ship | Run the Stage 0 walkthroughs first. If the three real use cases don't fit 5 templates cleanly, the template library is a bet against data we already have |
| Form generator balloons into bespoke-per-field code | Hard rule: every field type is a registered component; no template-specific forms |
| Binding validation drifts between frontend hints and backend truth | One validator: `/ui/resolve --dry-run`. Frontend calls it on every debounce; never re-implements |
| Autosave thrash during typing | Debounce 500ms; only edit the DB when the document parses cleanly |
| User dismisses "someone else edited this" banner and their next autosave overwrites concurrent work | **Banner is non-dismissable.** Only actions are "reload (discard local)" and "export local to clipboard" |
| CLI writes bypass the builder's OCC guard | Stage 1 guards transport-level with `expected_generation`; Stage 3's `NodeSlotRevisionService` becomes the single write path for both CLI and builder, eliminating the bypass |
| Saves to `ui.page.layout` bypass ACL check | Gate the save path through the existing `auth` crate's scope model. Start with a single `dashboard.write` scope until tenancy demands finer grain |
| Multi-tenant save path doesn't check tenant on the `ui.page` node | Enforce tenant membership on save, same shape as `flows` on edit / paste |
| Users expect a canvas because every other tool has one | Onboarding + README state plainly: no canvas in v1; if you need one, this project isn't for you |
| a11y regressions ship silently | Every Stage-1 panel has explicit a11y acceptance in its PR (keyboard reachability, focus order, aria-labels, intent-colour contrast). Stage 0.1 includes a keyboard-only pass |

---

## Open questions

1. **Is Stage 2 needed?** Blocking for Stage 2 only. Decided by Stage 0.3's LLM+Monaco stretch run.
2. **Who saves — a single `dashboard.write` scope, or per-node ACLs?** Non-blocking for Stage 1; default to single scope until tenancy demands finer grain.
3. **Thumbnails** (if Stage 2 ships). Hand-made PNGs for a 5-template library. Server-rendered becomes attractive past ~20 templates, which is well past v1.
4. **Multi-page dashboards (tabs/nav).** Covered at runtime by `ui.nav` + the `tabs` IR variant. Builder UX is under-specified; revisit after Stage 1 is in real use.
5. **Does the `page_state_schema` come from a template, or is it always authored inline?** Lean: inline in Stage 1 (`PageStateEditor.tsx`); if Stage 2 ships, templates carry a default schema that pre-populates the editor.

---

## Summary

- **Stage 0 is pure validation.** No new code. Scripted + unscripted + exploratory walkthroughs, stress tests, three LLM runs with an existence gate for Stage 2, error-shape audit, docs round-trip.
- **Two pre-Stage-1 fixes are blocking**: extend `/ui/resolve --dry-run` to emit binding errors; add `expected_generation` to slot writes.
- **OCC invariant is a hard rule**: the "someone else edited this" banner is non-dismissable. Reload or export, nothing else.
- **Stage 1 is the whole v1 bet.** Monaco + live preview + the extended `dry_run`. For the power user named above.
- **Stage 2 is probationary.** Ships only if the Stage 0.3 existence gate proves LLM+Monaco can't handle the seeds. Even if it ships, seed templates are chosen from the failures — not all five by default.
- **Stage 3 rides Phase 2 of UNDO-REDO.** `NodeSlotRevisionService` generalises over `node_setting_revisions`. All writes route through it, CLI included.
- **Stage 4 canvas is speculative.** No ship commitment. If the bet is wrong, v1 is the final shape — no silent deferral.
- **Stage 5 plugin-view editing is deferred indefinitely.** Left in the doc as a placeholder.

Prove the idea first. Fix the two substrate gaps. Ship Stage 1 for the power user. Let Stage 0.3 decide whether Stage 2 happens. The rest falls out.
