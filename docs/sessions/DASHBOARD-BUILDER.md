# Dashboard Builder — Scope

A visual surface for authoring `ui.page` nodes without hand-writing JSON. Power users get a Monaco-based JSON/YAML editor with live preview. Non-devs *may* get a template picker + form generator — contingent on a specific Stage 0 test result (see below). Block-authored `KindManifest.views` editing is deferred to the last stage, intentionally.

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

**Stage 6 target user: block authors.** Deferred indefinitely — the existing YAML-manifest path works, and the three other authoring modes (CLI/LLM, block YAML, hand-written JSON) cover the block-view case without new UX.

If this project ever acquires an external customer whose need reshapes the above, revisit. Until then, honest about what's being built and for whom.

---

## Authoring modes the builder coexists with

SDUI supports three authoring modes today. The builder is a fourth — for the power user named above. It is not a replacement for any of them.

| Mode | Artifact | Who | When |
|---|---|---|---|
| CLI / AI | `agent nodes create … ui.page` + `agent slots write … layout '<json>'` | LLM sessions, scripts, power users | Shipped (S1+) — see [testing/DASHBOARD.md](../testing/DASHBOARD.md) |
| YAML in block manifests | `views:` entry on a `KindManifest` | Block authors shipping "every instance of kind X has a default view" | Shipped (S5) |
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

**Stage 4 + Stage 5:** no new client methods. Canvas uses Stage 1's methods. Block-view editing is deferred.

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
- **A template authoring UI.** Any templates that ship (Stage 2) are maintained by this team as static JSON in-repo. Block-author-contributed templates are a v2 question.
- **Block-view editing in v1.** Stage 5 exists in the doc as a record; no work planned until a concrete need emerges.
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

**This stage is the whole v1 bet for power users.** Everything after Stage 1 is either UX polish (Stage 2, provisional), history plumbing (Stage 3), speculative (Stage 4 canvas), or deferred (Stage 5 block views). If Stage 1 doesn't feel right, nothing after it matters.

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

**Resolution: Stage 3 rides Phase 2 of UNDO-REDO.** Extend the pending `NodeSettingsService` to cover arbitrary versioned slots (generalise the domain type — `NodeSlotRevisionService` or similar; the table name stays). Dashboard history becomes the first concrete user; block node-settings history ships alongside.

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

Block authors edit kind-default views today via YAML manifest + agent reload. That works. The three existing authoring modes (CLI/LLM, block YAML, hand-written JSON) cover block-view authoring without any new UX. Until a concrete need emerges — a specific block author stuck on a specific workflow that can't be solved by improving YAML affordances — do not build this.

If a need *does* emerge, these are the knobs:

- **A. Runtime-only edits, with an explicit "Export view YAML" button** that dumps to clipboard + file. Block author commits manually. Agent restart reverts to YAML. Uses a non-dismissable "edits are runtime-only — export before closing" banner to make the limitation visible.
- **B. Refuse runtime edits.** Builder opens block-shipped views read-only. Consistent with how `settings_schema` works.
- **C. File-system round-trip.** Builder writes back to the original YAML path. Requires agent write access; breaks containerised deploys.

Option A is the only one that doesn't make the builder structurally incapable of touching the platform's primary authoring artifact, provided the "unsaved — export" banner is non-dismissable (same shape as the OCC conflict banner in Stage 1). Option B is a failed product for anything but consumption; don't default to it "because it's simpler."

**None of this is scheduled.** Leave it in the doc as a placeholder.

---

## Data model (v1, no schema migration)

Dashboards are `ui.page` nodes with a `layout` slot. The builder edits through the existing write paths:

- **Stage 1:** direct `POST /api/v1/slots/write` with `expected_generation`.
- **Stage 3 onwards:** through `NodeSlotRevisionService` (Phase 2 of UNDO-REDO), which owns the generation counter and appends to `node_setting_revisions` on every write.

No new tables.

Pages created by the builder and pages hand-authored by the CLI are indistinguishable — same slot, same shape. Pages defined in block YAML are read-only in the builder; any "fork to edit" affordance is deferred along with Stage 5.

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
- **Three pre-Stage-1 fixes are blocking**: extend `/ui/resolve --dry-run` to emit binding errors; add `expected_generation` to slot writes; ship `GET /api/v1/ui/vocabulary` + client method + CLI command (referenced across design docs but never actually implemented).
- **Separation of concerns is a structural rule.** `model/` is pure TS, no React; `store/` is headless; `panels/` and `canvas/` never import from each other; `preview/` doesn't import from `persistence/`. ESLint enforces; every layer has its own test shape.
- **No builder code calls `fetch` directly.** Every network call goes through the TS client per NEW-API.md's five-touchpoint rule.
- **OCC invariant is a hard rule**: the "someone else edited this" banner is non-dismissable. Reload or export, nothing else.
- **Stage 1 is the whole v1 bet.** Monaco + live preview + the extended `dry_run`. For the power user named above.
- **Stage 2 is probationary.** Ships only if the Stage 0.3 existence gate proves LLM+Monaco can't handle the seeds. Even if it ships, seed templates are chosen from the failures — not all five by default.
- **Stage 3 rides Phase 2 of UNDO-REDO.** `NodeSlotRevisionService` generalises over `node_setting_revisions`. All writes route through it, CLI included.
- **Stage 4 canvas is speculative.** No ship commitment. If the bet is wrong, v1 is the final shape — no silent deferral.
- **Stage 5 block-view editing is deferred indefinitely.** Left in the doc as a placeholder.

Prove the idea first. Fix the two substrate gaps. Ship Stage 1 for the power user. Let Stage 0.3 decide whether Stage 2 happens. The rest falls out.

---

## Implementation status (snapshot: 2026-04-20)

Work since this doc was written. Numbers refer to commit subjects; look them up in `git log` for the full diff.

### Shipped — substrate (backend)

- **`GET /api/v1/ui/vocabulary`** — returns `{ir_version, schema}` where schema is `schemars::schema_for!(ui_ir::Component)`. Full five-touchpoint: [crates/dashboard-transport/src/vocabulary.rs](../../crates/dashboard-transport/src/vocabulary.rs), `clients/rs` `UiVocabulary` DTO + `client.ui().vocabulary()`, `agent ui vocabulary` CLI, `CommandMeta`, TS client `vocabulary()`, fixture + `fixture_gate::ui_vocabulary_ok`.
- **`expected_generation` OCC on slot writes** — `POST /api/v1/slots` accepts optional `expected_generation`; mismatch returns 409 with `{code: "generation_mismatch", current_generation}`. `clients/rs` ships `ClientError::GenerationMismatch` + `Slots::write_with_generation`; TS client ships `GenerationMismatchError` + `writeSlot({..., expectedGeneration})`. `GraphStore::write_slot_expected` enforces the check inside the write lock so the guarantee is atomic. Fixture: `slots_write_generation_mismatch`.
- **`/ui/resolve --dry-run` binding validation** — walks the layout, parses every `{{...}}` with `dashboard_runtime::Binding::parse`, evaluates against `EvalContext`, collects failures as `ResolveIssue` entries. Also flags `$page.*` keys not declared in the page's `page_state_schema.properties`. Shared tree-walking helper at [crates/dashboard-transport/src/binding_walk.rs](../../crates/dashboard-transport/src/binding_walk.rs) (reused by `render.rs`'s `scan_bindings`). Fixture: `ui_resolve_dry_run_binding_errors`.
- **Inline `layout` override on `/ui/resolve`** — when the request body carries `layout: <candidate>`, the resolver validates/renders that instead of the node's persisted slot. Honoured on both `dry_run` and live paths. Lets the builder preview an unsaved buffer with the real server-derived subscription plan.
- **Query parser strips matching surrounding quotes** — `kind=="ui.page"` now matches the stored value `ui.page`. Was a real user-blocking bug; `id==<uuid>` and every quoted-value filter path were silently returning zero rows. [crates/query/src/parser.rs](../../crates/query/src/parser.rs) `strip_matching_quotes`.

### Shipped — frontend (page builder)

- Feature dir: [frontend/src/features/page-builder/](../../frontend/src/features/page-builder/).
  - `model/` — pure TS. `types.ts` (`DraftPage`, `ValidationIssue`), `validate-layout.ts` (JSON parse + shape checks, line:col pinning).
  - `store/` — zustand. `builder-store.ts` holds draft, issues, `saveState`, `conflict`.
  - `persistence/` — side effects. `use-validator.ts` (local + server dry-run; layout override passed through), `use-autosave.ts` (debounced `writeSlot` with OCC; maps `GenerationMismatchError` into the store's `conflict` state).
  - `preview/LivePreview.tsx` — posts the current editor buffer as the inline layout to `/ui/resolve`, renders the returned tree through the existing `SduiProvider`/`Renderer`, mounts `useSubscriptions` against the returned plan.
  - `panels/` — `EditorPane.tsx` (Monaco JSON editor, markers driven by `store.issues`), `ValidationList.tsx`, `SaveStatus.tsx`, `ConflictBanner.tsx` (non-dismissable; reload + clipboard-copy).
- Route `/pages/:id/edit` → `PageBuilderPage`.
- `/dashboard` route and `DashboardPage` deleted — pages are `ui.page` nodes and the listing is at `/pages` via the renamed `PagesListPage`. Sidebar entry updated. The "Pages" list has a **New page** button that creates a `ui.page` at root, seeds a minimal layout via `writeSlot`, and navigates straight into the builder. Empty-state is a prominent CTA with explanatory copy.
- Chart component now actually fetches data (it shipped with empty `series` previously and no fetch path). Client-side fetch for `source.{node_id, slot}`: splits node-id→path into its own long-staleTime query, probes telemetry + history in parallel on first fetch, pins to whichever store has rows, extracts `.payload` from flow-engine envelopes (`{_msgid, _ts, payload}`).
- `useSubscriptions` patches caches in place on `slot_changed` events via `setQueriesData` instead of invalidating — tables merge the new slot value into the matching row, charts append `[ts, value]` to the series. Falls back to invalidation if the predicate matches no cached entry. **Patch path is believed incomplete — see Known issues §1.**

### Not yet shipped — Stage 1 polish

- **Vocabulary palette** (`panels/VocabularyPalette.tsx`). Driven by the already-shipped `/ui/vocabulary` endpoint. Empty today; `EditorPane` does not wire a JSON Schema reference into Monaco, so autocomplete is JSON syntax only, not semantic IR awareness.
- **YAML mode toggle.** Scope says it parses to the same model and saves as JSON.
- **Page-state editor** (`panels/PageStateEditor.tsx`). Would edit the page's `page_state_schema` slot inline.
- **ESLint `no-restricted-imports` layer rule.** The layer split is currently enforced by convention, not by lint.
- **Headless `vitest` suites for `model/` and `store/`.** Neither layer has tests yet; the `validate-layout.ts` line/col logic especially deserves coverage.

### Not yet shipped — Stage 2/3/4/5

All deferred as originally specified. Stage 0.3 existence gate has not been run; Stage 2 remains probationary.

---

## Known issues

Open issue tracker. Each entry: **ID**, **symptom**, **what's been tried**, **where to look next**.

### #1 — Live cell updates not visually propagating after SSE patch (STILL BROKEN — two root causes fixed, symptom persists)

**Update 2026-04-20 (first round):** found a likely root cause that none of the original A–E hypotheses cover. `Chart.id` and `Table.id` are `Option<String>` in [crates/ui-ir/src/component.rs:128,166](../../crates/ui-ir/src/component.rs#L128). The plan emitters in [crates/dashboard-transport/src/render.rs](../../crates/dashboard-transport/src/render.rs) bail when `id` is empty (`unwrap_or("") + !id.is_empty()` guard), so any layout authored without explicit ids on subscribed widgets produced **zero** subscription plan entries — the SSE handler at `useSubscriptions.ts:64` would silently `continue` on every event because `subjectToWidget.get(subject)` returned `undefined`. The original debug recipe logged from inside the patch functions, so a missing plan looked identical to "events aren't firing."

**First-round fix.** New `assign_synthetic_ids` pass on the resolve handler walks the layout JSON and injects deterministic ids (`auto:chart:0`, `auto:table:0`, …) on chart/table/sparkline/timeline that omitted `id`. Same pass runs for the `/ui/render` `ui.page` fast-path. Both the rendered tree the client receives and the subscription plan now carry the same id, so the patch path's `setQueriesData({queryKey: ["sdui-table", widget]})` actually matches. Tests in `render.rs::synthetic_id_tests`.

**Second-round fix (same session, different root cause, same symptom).** Even with synthetic ids in place, live updates still dropped on the floor for users who authored explicit `id`s. The culprit was a **NodeId string-format split** across the dashboard stack:

- `NodeId` is `NodeId(pub Uuid)` with `#[serde(transparent)]`. Default serde therefore emits the hyphenated form (Uuid's own Display).
- `NodeId::Display` however used `self.0.simple()` → **un-hyphenated** (32-char hex).
- The subscription-plan emitters in `render.rs::emit_query_subjects`, `table.rs`, and `reader.rs` all stringified ids via `snap.id.0.to_string()` (i.e. through the Uuid's default Display) → hyphenated subjects like `node.3b661138-40c1-....slot.count`.
- SSE events were emitted with NodeId::Display (un-hyphenated) → `node.3b66113840c14d4dac568d86cbaea56c.slot.count`.
- Chart/kpi plans derive subjects from the `source.node_id` string the author wrote in the layout (un-hyphenated, since that's what `/api/v1/nodes` returns). They matched.
- Table plans went through the `emit_query_subjects` path with `.0.to_string()`. They did not.

Result: table updates were silently dropped every tick; chart/kpi updates worked whenever the author remembered to use the un-hyphenated form. Users with mixed layouts got partial live behaviour, which is maximally confusing.

**Canonical form: un-hyphenated, always.** `NodeId` and `LinkId` now have custom `Serialize`/`Deserialize` impls that always emit `simple()` and accept both forms on input. `Display` already did the right thing. Every `.0.to_string()` call site in dashboard-transport + transport-rest + dashboard-runtime + tests has been swept to `.to_string()` so there is exactly one string shape in the wire and the logs. Fixture-gate's UUID-normaliser recognises both 32-char and 36-char shapes for backward compatibility with any pinned fixtures.

**Status: still not ticking live per user report.** Both fixes shipped and pass unit/fixture tests. The user reloaded after the NodeId normalisation change and reports the widgets still do not update in real time. Something else is still wrong — the two causes above are real bugs that needed fixing independently, but neither is the complete story. Open hypotheses:

- **Hyp F — SSE stream isn't actually being consumed in the builder preview.** `useSubscriptions` is mounted inside `LivePreview`, which has a `queryKey` that includes `debouncedText + pageState`. Every edit (and the compose-panel apply) blows away the effect, cancels the SSE iterator, and re-subscribes. Between cancel and re-subscribe the reader loses position. In read-only `/ui/:id` the queryKey is stable — which is why live ticks work there and not in the builder (Hypothesis D from first round, still unverified).
- **Hyp G — `applyToTables`'s `setQueriesData({queryKey: [...]})` is matching the query but React Query v5 doesn't renotify observers when the functional updater returns structurally-equal-but-new-reference data.** Try a forced `invalidateQueries({queryKey, exact: false})` after the `setQueriesData` to see if the observer fires then.
- **Hyp H — the event stream never arrives.** Open devtools Network → filter `events` → confirm the `/api/v1/events` SSE connection is open and lines are flowing. If lines stop, the bug is earlier (SSE client), not in the subscription router.
- **Hyp I — flow-engine envelope shape.** `slot_changed` events carry `value: {_msgid, _ts, payload}`. The chart's `applyToCharts` extracts `.payload` before appending, but the table's `applyToTables` sets `slots[slot] = event.value` directly (the envelope). Column field `slots.count.payload` would read through — but authors using `slots.count` see the envelope object and get `[object Object]`. Not a missed-tick bug, but cosmetic confusion on top.

**Next-session first step:** open the page, set `localStorage.setItem("sdui_debug", "1")`, reload, watch devtools console. Three outcomes:

1. No `[sdui] subscriptions mounted` log ever → Hyp F (builder instability tearing the subscriber down).
2. Mounts fire but `[sdui] event matched` never does → Hyp H (SSE isn't delivering) or a subject-format mismatch we haven't found yet (curl `/api/v1/events` to confirm event shape).
3. `event matched` fires on every tick AND `touched=true` → Hyp G (React Query observer notification).

Leave the NodeId normalisation + synthetic-id fixes in place — both were real bugs regardless of this third one, and reverting them would mask the real cause.

**Diagnostic also added.** `useSubscriptions.ts` now logs subscription mounts and per-event match/miss when `localStorage.sdui_debug === "1"`. Set that and reload to see exactly which subjects the plan covers and whether incoming events match.

**Verification step for next session.** Reload the heartbeat page, set `localStorage.setItem("sdui_debug", "1")`, watch the devtools console: every tick should log `[sdui] event matched node.<uuid>.slot.count → auto:chart:0` (or whatever id the author set). If matched events still don't visually update, the remaining hypotheses below are still in play (B/D for the chart's path-query race, E for table cell memoisation).



**Symptom.** A `ui.page` with a subscribed table (`{type: "table", source: {query: "path==/flow-1/heartbeat", subscribe: true}, columns: [{field: "slots.count.payload"}]}`) mounts fine and the initial row shows the current payload. Heartbeat continues ticking and SSE delivers `slot_changed` events — but the cell value does not visibly update. Same story for the chart: initial render has points, but new points don't appear.

**What's been tried.**
1. First cut invalidated `["sdui-table", widget]` / `["sdui-chart", widget]` on every event. User confirmed this worked — but per-tick network traffic was 3 requests (table refetch + chart telemetry + chart history; chart also did a node-id lookup each tick, so technically 4 calls for a 1-table-1-chart page).
2. Cached node-id→path lookup (long `staleTime`). Pinned the chart to whichever store had rows after the first probe. Dropped to 2 requests per tick.
3. Switched from invalidate to `setQueriesData` with a custom `predicate` function so events mutate cache in place. **User reported widgets stopped updating.**
4. Replaced the custom predicate with TanStack Query v5's native `{queryKey: ["sdui-table", widget]}` prefix filter, and added a fallback: if no query matches the patch, invalidate the prefix so something refreshes. **Still not updating visibly per user.**

Current code: [frontend/src/sdui/useSubscriptions.ts](../../frontend/src/sdui/useSubscriptions.ts).

**Debugging hypotheses for the next session.**

- **Hypothesis A — the predicate truly isn't matching.** Either the queryKey prefix comparison treats array elements non-structurally, or the TableComp's actual queryKey shape doesn't start with `["sdui-table", <widget_id>]`. Verify by console-logging `q.queryClient.getQueryCache().getAll().map(q => q.queryKey)` immediately inside the subscriber and confirming the live widget's cached key. `TableComp`'s key is `["sdui-table", node.id, node.source.query, page, pageSize]` ([TableComp.tsx:39](../../frontend/src/sdui/components/TableComp.tsx)). If `node.id` differs from `widget_id` in the plan, the predicate silently misses.
- **Hypothesis B — the updater returns a new object reference but React Query treats it as unchanged for notification purposes.** Check with a log: `console.log("table patch applied", touched)` from inside the updater. If `touched` is `true` and no observer fires, the issue is in how `setQueriesData` propagates; try `invalidateQueries` as a forced re-observation after the update.
- **Hypothesis C — the builder's preview re-resolves on every keystroke (queryKey includes `debouncedText`), so the `useSubscriptions` instance is tearing down and rebuilding its SSE reader too fast to keep up with the tick rate.** Verify by opening the builder, pausing all editor keystrokes, and watching the Network tab: if patches start landing only after a pause, this is it. Fix: factor the subscription setup out of the resolve query lifecycle, or feed the subscription plan through a stable ref so the SSE reader doesn't thrash.
- **Hypothesis D — `setQueriesData` on a component whose owning query is stale returns the old data to renders.** Under v5, a stale query that is still referenced by an observer should still notify; but if the builder keeps the query alive only transiently (see C), the patch may not reach an observer that matters. Test by mounting the same page at `/ui/:id` (production render, stable subscriptions) and watching the same events — if it ticks there, the bug is builder-shell-specific.
- **Hypothesis E — `TableComp` reads `row.slots[col.field]` through a dotted-path walker that does **not** re-evaluate after a shallow slots-map swap.** Check [TableComp.tsx:23 `getPath`](../../frontend/src/sdui/components/TableComp.tsx). Our patch returns `{...row, slots: {...row.slots, [slot]: value}}` — a new slots reference — but if React memoizes the cell body by row identity alone, the cell won't re-render. Quick fix: bust per-row memo by always returning `{...row}` when patched.

**Recommended first debug step for next session.** Start a dev build, open the browser devtools, and add three `console.debug` lines:

```ts
// useSubscriptions.ts, inside the for-await loop:
console.debug("[sdui] event", event.slot, event.id, "widget:", widget);
// inside applyToTables, after the map:
console.debug("[sdui] table patch touched=", touched, "rowId=", event.id);
// inside applyToCharts, after the if-not-empty:
console.debug("[sdui] chart patch applied", head?.label, v);
```

The shape of the output tells you which hypothesis is live. Report back and we can fix it in one more PR.

### #2 — `id==<uuid>` filter relied on a quote-strip fix; verify dependent paths

The query parser fix (`strip_matching_quotes`) unblocked both the Pages list and the builder's draft loader. Several other places filter by uuid: chart's path lookup at [Chart.tsx useChartFetch](../../frontend/src/sdui/components/Chart.tsx). None should regress, but when the next session adds new `filter=id==...` callers the pattern to use is unquoted (`` `id==${uuid}` ``) — the quote is optional but the parser now tolerates both.

### #3 — Chart data still costs one round-trip on first render

Even with the patch path working, the *first* render of a chart queries history/telemetry to seed the series. The doc-comment on [Chart.tsx](../../frontend/src/sdui/components/Chart.tsx) says "the server fills `series`" — that was aspirational; no server-side population exists. Cleanest long-term fix: populate `series` in `resolve.rs` for every authored `chart` component whose `source.{node_id, slot}` resolves, by reading history/telemetry in the resolver and emitting the points in the render tree. Once done, the chart becomes a pure renderer, `sdui-chart` query disappears, and SSE patches are the only update path.

### #4 — Pre-existing workspace test failures (not from this session)

`cargo test --workspace` is currently red on two fronts, confirmed pre-existing before this session by a `git stash` round-trip:

- `crates/graph/tests/seed_snapshot.rs::math_add_manifest_is_pinned` — pinned manifest is missing `last_a/last_b/last_sum` status slots that the actual manifest now emits. Fix: regenerate the pinned snapshot.
- `crates/observability/tests/no_println.rs::library_code_does_not_use_println_or_eprintln` — four existing offenders (`transport-cli/src/commands/capabilities.rs:16`, `transport-cli/src/commands/ui.rs:206` / `:320`, `transport-cli/src/output.rs:188`) have their `NO_PRINTLN_LINT:allow` marker on the line *after* the `eprintln!(` call, but the lint requires same-line. Fix: move the marker onto the call line.

Neither blocks Stage 1.

### #5 — Frontend `tsc --noEmit` has one pre-existing error

`rsbuild.config.ts:57` — "An object literal cannot have multiple properties with the same name." Unrelated to the builder; flagged here so the next session doesn't blame the wrong change.

---

## Handoff notes for the next session

- **Read these first** — they were updated alongside this session's substrate work and are now internally consistent:
  - [`docs/design/DASHBOARD.md`](../design/DASHBOARD.md) — framework-level backend spec. Now lists the vocabulary endpoint, the `layout` override on resolve, the OCC param on slot writes, and the expanded dry-run error set.
  - [`docs/testing/CLI-DASHBOARD.md`](../testing/CLI-DASHBOARD.md) — authoring recipes via the CLI. Covers the four Studio routes (including `/pages/:id/edit`), the envelope caveat (`{_msgid,_ts,payload}`), `--expected-generation`, and the new dry-run binding errors in the failure table.
  - [`docs/design/SDUI.md`](../design/SDUI.md) — IR vocabulary and binding grammar. Unchanged this session but load-bearing for interpreting dry-run errors.
- **Top priority:** resolve issue #1. The drop to 2 calls/tick is correct; the value-doesn't-update problem is the only blocker for declaring Stage 1's live-preview bet proven. Work through the hypotheses in order (A → E).
- **After #1:** decide whether to ship the server-side chart population (#3) now or defer. If deferring, add a comment on `Chart.tsx` to that effect so the stale "server fills series" docstring stops misleading readers.
- **Do not start Stage 2 work.** The Stage 0.3 existence gate has not been run. Stage 2's five seed templates may or may not ship based on that gate.
- **Do not add new endpoints without the five-touchpoint rule.** [NEW-API.md](../design/NEW-API.md) still applies. The `layout` override on `/ui/resolve` was added as a param on an existing endpoint; that's fine. A *new* endpoint would need Rust handler + Rust client + CLI + CommandMeta + TS client + fixtures.
- **Dev server recipe.** Agent on port 8081: `./target/debug/agent run --http 127.0.0.1:8081 --db /tmp/agent.db`. Frontend against that agent: `PUBLIC_AGENT_URL=http://localhost:8081 pnpm --filter @sys/studio dev`. To build the TS client after any `clients/ts/src/` change: `pnpm --filter @sys/agent-client build` (frontend imports from `dist/`, not `src/`).
- **Fixture gate.** Every new endpoint or altered error contract gets a fixture under `clients/contracts/fixtures/cli-output/` and a test in `crates/transport-cli/tests/fixture_gate.rs`. Do not skip this; it is the only guard against silent contract drift across the CLI and the TS client.

---

## Next-up feature menu (post-issue-#1)

This is the long-term backlog — not "everything we'll ever build" but the specific primitives that separate a widget renderer from a dashboard framework. Ordered by leverage, not by effort. Stage 2's existence gate still applies on top of this list: every item is optional until a real authored page hits a wall without it.

Context for the tiers: the friend's reference system (their YAML linked from session notes) showed a polished CRUD-over-collections shape — filters, bulk actions, date presets, dashboard composition. Most of that maps cleanly onto our existing IR; what's missing are a handful of input components and two or three substrate tweaks.

### Tier 1 — essential primitives, each small, each unblocks a page class

1. **Query templating in `source.query`** — `{{$page.*}}` substitution at resolve time. The keystone: without this, every filter UI is cosmetic. Substrate change in [crates/dashboard-transport/src/table.rs](../../crates/dashboard-transport/src/table.rs) and the `table`-plan collector in [render.rs](../../crates/dashboard-transport/src/render.rs).
2. **`select` / `toggle` / `field` input components** writing to `$page.<key>`. Already named as Stage 2 prereqs earlier in this doc. Each mirrors the existing S6 component boilerplate (`ui_ir::Component` variant + ZOD schema + renderer component).
3. **Date-range picker with presets** (`15m / 1h / 6h / 24h / 7d / all`). Writes `{from, to}` into a `$page` key; chart and table consume via binding. Strictly better than chart drag-zoom as the only time control.
4. **Table selection → bulk actions** — `select: "single" | "multi"` on the table writes ids into `$page.<selection_key>`. Bulk-action buttons read that key as their `args`. No component changes beyond the table.
5. **`confirm: "..."` field on actions** (cherry-picked from the reference YAML). Every destructive button gets a typed confirm dialog; zero per-app code. Small `ui_ir::Action` addition.
6. **KPI / stat tile component** — single big number with optional delta + intent. Concretely hit as a gap this session when the user wanted "show the count live" and had to fall back to a one-row table.
7. **`row_link` sugar on tables** — navigates to another page with `$row.id` in context. Authors write `row_link: {page_ref, with: {id: "$row.id"}}` instead of a hand-authored `row_action: navigate` + handler.

### Tier 2 — architectural, high-leverage, change what the platform is

8. **URL ↔ `page_state` sync.** Filter state, selected tab, date range all round-trip through the URL. A dashboard link sent in chat lands on the same view. Non-negotiable long-term; the most underrated feature in dashboard tooling.
9. **Schema-driven `node_form` component.** Given a node id/path, read the kind's slot schemas, render per-slot inputs, submit through `writeSlot`. Eliminates hand-authored forms for the 80% case. Uses `@rjsf` (already a frontend dep).
10. **Page-as-widget (`{type: "page_embed", page_ref}`).** Lets dashboards compose smaller pages. Directly analogous to the reference system's `layout` view type, without inventing a new concept. A `page_embed` resolves to the referenced page's tree inline at resolve time.
11. **User preferences as a node subtree.** A `sys.user.preferences` kind under the user's actor id; a `$prefs.<key>` binding source reads it. Feeds sticky state like last-viewed tab, default date range, column visibility. Node model already handles versioning/ACL/sync — no new infra.
12. **Distinct-values endpoint** (`GET /api/v1/nodes/distinct?field=slots.severity&kind=sys.alarm`). Drives filter-chip options from real data instead of hard-coding `[low, medium, high]` per view. Keeps filter UIs honest as data evolves.
13. **Paused / reconnecting indicator.** A shared SSE status badge. When the stream drops, every live widget dims and a tiny chip says "reconnecting". Tiny change; outsized trust signal.
14. **Inline cell edit.** Click a writable cell, edit, commit through `writeSlot` with OCC. Row-level forms without leaving the page.
15. **Empty-state component** — `{type: "empty", title, description, action?}`. One canonical way to say "nothing here yet — here's what to do" across the whole vocabulary.

### Tier 3 — later polish, opinionated

16. **Undo snackbar** for destructive actions. Dismiss → 5s "undo" toast → restore via `slots.at(revision)`. Rides Phase 2 of UNDO-REDO when Stage 3 lands.
17. **Tabbed shells at the page level** — per-page tabs with persistent state. Today authors can use the `tabs` component, but page-level tabs are nicer.
18. **Column visibility / reorder** — table gains a gear menu; state lives in user preferences (#11).
19. **CSV / JSON export** — table gets a dump button. Pure client-side over the current rows; zero substrate change.
20. **"Inspect binding" dev overlay** — hover a cell in the builder preview, see which slot feeds it and the raw server path. Debugging gold; also a selling point.
21. **Saved filter presets** — `/pages/alarms?preset=unack-high`. Presets are child nodes of the page, queryable like anything else.

### Cherry-picked from the reference YAML

- **`confirm:` on actions.** (Tier 1 #5.) Strictly better than authoring a separate dialog per destructive button.
- **Declarative `filters[]` shape** on tables. Not the authoring format, but the schema (`{column, title, options}`) — worth mirroring so a filter bar is a first-class field on `table`, not a separately-authored row of selects.
- **Date-range preset buttons.** (Tier 1 #3.) Universal muscle memory across every product in the space.
- **Action `target: record | selection | collection`.** Clean vocabulary for "this button applies to one row / selected rows / the whole collection." Worth stealing verbatim.

### Explicitly NOT copying

- **Views as a parallel registry.** Their views live in their own YAML alongside collections, a duplicate world. Our pages are graph nodes — they inherit versioning, ACLs, subscriptions, import/export for free. Keep ours.
- **A separate `fields:` schema list.** Kinds already carry per-slot schemas. Don't duplicate the schema definition in the view.
- **Collection/record/info/actions/layout as *view types*.** Their classifier exists so their renderer knows which shape to project. Our IR is the shape directly — `table`, `form`, `page_embed`. Strictly more flexible.

### The three bets worth naming

- **URL = complete state** (#8). The single most underrated feature in dashboard tooling. Everything else gets easier once deep links work.
- **Everything is a slot write.** Form submits, bulk actions, preferences all route through `writeSlot` + OCC + revisions. Undo/redo for free, audit for free, OCC story unified with the CLI.
- **Pages compose via page-embed** (#10). The difference between a widget library and a framework. A dashboard is a page that embeds an alarms page that embeds a severity filter — all authored separately, all independently versioned.

### Suggested first end-to-end slice

Once issue #1 is resolved, the quickest visible win is Tier 1 #1 + #3 together: **query templating** + **date-range preset picker**. That's exactly the shape of the reference system's `Readings` chart — a row of time-range buttons above a chart whose `source.range` reads `$page.range`. Implement those two; every subsequent filter feature reuses the same substrate.

---

## Chart history backfill + live stream (proposal)

**Problem.** Today a `chart` mounts with an empty series and only grows as SSE `slot_changed` events arrive. Open the page, you see nothing until the next tick. Close/reopen, the prior points are gone. Every live chart is effectively amnesiac — "live" without "history" is only half a chart.

**Shape.** Add a `history` block to the `chart` IR. Declarative, additive: `source` continues to describe the live stream (unchanged), `history` describes the initial backfill. The two compose: on mount the client fetches history, then SSE extends the same series forward.

```json
{
  "id": "hist",
  "type": "chart",
  "source": {
    "node_id": "973dab…",
    "slot": "out",
    "field": "payload.count"
  },
  "history": {
    "enabled": true,
    "range": "last_1h",
    "bucket": "10s",
    "agg": "avg",
    "user_selectable": true
  },
  "live": { "subscribe": true, "max_points": 500 }
}
```

**Fields.**

- `range` — preset (`last_5m | last_1h | last_6h | last_24h | last_7d`) **or** `{ from, to }` ISO timestamps. Also accepts a `$page.<key>` binding so one picker drives many charts (see Tier 1 #3).
- `bucket` + `agg` — server-side downsampling. `agg ∈ {avg, min, max, sum, last, count}`. Non-negotiable: without bucketing, 7 days of 1 Hz data is 600k points per chart per client.
- `user_selectable` — renders a per-chart range picker above the chart. Cheap escape hatch for one-off drill-downs without a page-level `$page.range`.
- `live.subscribe` — whether to open an SSE subscription after the backfill lands. Default `true`.
- `live.max_points` — ring-buffer cap for the merged (history + live) series. Default ~500; older points drop off the left as new ticks arrive.

**Flow.**

1. Mount. Client calls `GET /api/v1/history?node=…&slot=out&field=payload.count&from=…&to=…&bucket=10s&agg=avg`. Response is a seeded `[[ts, value], …]` array.
2. Subscribe. Client opens SSE from `to` (or `now`) forward. Appends each event to the same series; trims to `max_points`.
3. Range change (picker or `$page.range` write). Cancel SSE, refetch history, resubscribe from the new `to`. One codepath, not two.

**Why this shape.**

- `source` semantics stay unchanged — existing dashboards keep working; `history` is purely additive. Authors without a history store can simply omit the block.
- Server does the bucketing. Browser never sees raw events older than `max_points` worth of live ticks.
- The same `history` block works for `sparkline` and `timeline` with zero IR changes — they consume the same `[ts, value]` shape.
- `range` accepting a `$page` binding means one date-range picker (Tier 1 #3) drives every chart on a page. No special wiring per chart.

**Open questions before implementation.**

- **Storage.** Is there a timeseries store for `payload.*` fields today, or does history need to be derived from the revision/event log? If the latter, bucketing has to happen on top of a scan of `slot` revisions — acceptable for `last_1h` shapes, painful for `last_7d`.
- **Dedup at the seam.** If the backfill ends at `to = now` and the first SSE event fires during the history fetch, we need a monotonic cursor (`ts > last_history_ts`) to avoid a duplicate point. The merge belongs in the chart renderer, not per-caller.
- **Default when unset.** If `history` is omitted, preserve today's behavior (empty on mount, populate from SSE). Don't silently turn this on — authors who want it should opt in.
- **Where the endpoint lives.** `crates/transport-rest` is the natural home; the reader abstraction probably belongs in `crates/dashboard-transport` alongside the existing table/chart plan emitters so the CLI can exercise it through the same fixture-gate path.

**Smallest shippable slice.** `history.range` as preset-only (no bindings yet), server-side bucketing, no picker UI. A single new client call, one new endpoint, chart appends history to its series before SSE mounts. Bindings + picker fall out naturally on top of Tier 1 #3.


