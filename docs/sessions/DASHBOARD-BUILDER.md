# Dashboard Builder — Scope

A visual surface for authoring `ui.page` nodes without touching react-flow or the CLI. Users pick a template, fill in a form, preview live, save. Power users drop into a JSON/YAML editor with the same preview pane.

**Not a canvas.** A free-form pan-zoom builder is explicitly deferred (see Stage 5). We're betting that 80% of real dashboards fit a small set of curated templates, and that a template + form UX is cheaper to build, cheaper to maintain, produces consistent-looking pages, and diffs cleanly. If that bet is wrong, Stage 5 adds the canvas — informed by which layouts the template library actually missed.

Authoritative references:
- [DASHBOARD.md](../design/DASHBOARD.md) — node kinds, binding sources, transport contract
- [NEW-SESSION.md](../design/NEW-SESSION.md) — resolve / subscription plan
- [UNDO-REDO.md](../design/UNDO-REDO.md) — shipped flow-revision machinery we reuse wholesale
- [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md) — `ui.*` kinds live in the graph like anything else

---

## Goals

- **Non-devs can build a dashboard** from templates without writing JSON.
- **Power users can drop into JSON/YAML** for cases templates don't cover, with the same live preview.
- **Logic and visual are separable.** Model, store, and validation are pure and testable without a DOM; canvas/panel components are pure views over that store.
- **Reuse the existing SDUI renderer** for preview — the builder's output is the same `ui.page` shape the runtime already resolves.
- **Reuse the existing undo/redo backend** — no new revision machinery. Saves are flow edits; undo is `POST /flows/{id}/undo`.
- **Binding safety.** Invalid bindings (`$page.missing`, wrong type, unknown template hole) fail at save time with a clear error, not at render.

## Non-goals

- **Canvas / pan-zoom / free positioning.** Stage 5, not v1. No Moveable.js, no react-grid-layout in Stages 1–4.
- **Drag-from-palette onto a canvas.** Widgets are added by filling template slots, not by dropping.
- **Real-time collaborative editing.** Same story as flows — one editor at a time; `expected_head` collisions surface as "someone else edited this."
- **Runtime state authoring** (widget inbox contents, cached values). We only version authored state.
- **A template authoring UI.** Templates are shipped as YAML in `crates/dashboard-nodes/manifests/` and registered like other kinds. Building templates is a dev task in v1.
- **Bindings-as-a-graph overlay.** Nice-to-have, deferred to Stage 5 alongside the canvas.

---

## Shape of the work

Four stages. Each one produces a thing the user can touch. Stage 5 is the opt-in canvas, explicitly not v1.

### Stage 1 — Template registry + schema endpoint

**Goal:** the backend can describe a template well enough that a generic form renders it.

- [ ] `ui.template` manifest format finalised in `crates/dashboard-nodes/manifests/` — each template declares: id, version, display name, icon, category, **parameter schema** (JSON Schema), **preview thumbnail** path, and the layout tree it expands to.
- [ ] `GET /api/v1/ui/templates` — list installed templates (id, version, display name, category, thumbnail URL). Tenant-scoped via the same ACL as the graph.
- [ ] `GET /api/v1/ui/templates/{id}` — full manifest including the parameter JSON Schema.
- [ ] `POST /api/v1/ui/validate` — body: `{ page_ref | page_document }` → `{ ok, errors[] }`. Errors carry a structured `{ path, code, message }` so the frontend can pin them to the right form field. Shared validator — not a re-implementation — used by save and by the editor on every keystroke.
- [ ] **Seed templates** (ship 5; this is the v1 library):
  - `SingleStat` — one metric, one title, optional threshold colour
  - `KpiGrid` — N-up tiles over a list binding
  - `TableWithFilters` — table + filter row bound to `$page.*`
  - `ChartWithDropdown` — time-series chart + dropdown writing to `$page.range`
  - `MasterDetail` — list on the left, detail panel on the right, selected row in `$page.selected`
- [ ] Template resolver honours version pinning (per DASHBOARD.md) — upgrading a template does not silently rewrite existing pages.

**Proves:** the backend can describe the template library to the frontend with enough fidelity to drive a form generator. No UI yet.

### Stage 2 — Frontend skeleton + template picker + form generator

**Goal:** user clicks **New dashboard**, picks a template, fills a form, saves. Page renders via the existing SDUI renderer.

Layout (logic/visual split, hard rule: `canvas/` and `panels/` never import from each other; both read `store/`, `store/` reads `model/`):

```
frontend/src/features/dashboard-builder/
├── model/                    ← pure, no React, unit-tested headless
│   ├── types.ts              ← DraftPage, DraftWidget, BindingRef
│   ├── bindings.ts           ← $stack/$self/$user/$page parser + typecheck
│   └── template-args.ts      ← args ↔ JSON Schema validation
├── store/                    ← zustand, no DOM
│   ├── builder-store.ts      ← draft, dirty, lastSavedRev, validationErrors
│   └── persistence.ts        ← debounced save via flows edit endpoint
├── preview/
│   └── LivePreview.tsx       ← wraps existing sdui/Renderer.tsx
├── panels/
│   ├── TemplatePicker.tsx    ← grid of thumbnails, categories, search
│   ├── ArgsForm.tsx          ← JSON-Schema-driven form; field components per type
│   ├── BindingField.tsx      ← typed binding autocomplete ($stack.* etc.)
│   ├── PageStateEditor.tsx   ← declares $page.* names + initial values
│   └── ValidationList.tsx    ← errors pinned to fields, click-to-jump
├── DashboardBuilderPage.tsx  ← split-pane shell only
└── __tests__/                ← model/ and store/ tests, no jsdom
```

- [ ] Builder page route at `/dashboards/new` and `/dashboards/:nodeId/edit`.
- [ ] Template picker: grid of thumbnails from `GET /ui/templates`, filter by category, search by name.
- [ ] Form generator: walks the template's JSON Schema and emits field components (string, number, enum → select, ref → node-picker, binding → `BindingField`). Unknown types fall through to a monospace JSON textarea with validation.
- [ ] Binding field: autocomplete for `$stack.*`, `$page.*`, `$self.*`, `$user.*`. Validates against the template's declared slot types so the user sees type errors inline, not at save.
- [ ] Debounced autosave: every change writes a flow edit (`POST /flows/{id}/edit`) with the current head as `expected_head`. `409` surfaces as "someone else edited this — reload."
- [ ] Preview pane: calls `POST /ui/resolve` against the current draft and renders via the existing `sdui/Renderer.tsx`. No code changes to the renderer.

**Proves:** a non-dev can make a working dashboard from a template end-to-end, with live preview and persistence.

### Stage 3 — JSON/YAML editor + validation

**Goal:** power users can edit the raw page document and see errors inline. Same preview pane.

- [ ] Monaco editor with JSON Schema wired from the template manifest (autocomplete + inline red squigglies).
- [ ] YAML mode toggle — parses to the same document model; save normalises to JSON on the wire.
- [ ] `/ui/validate` called on every debounce; errors rendered in the gutter and in the shared `ValidationList` panel.
- [ ] Toggle between **Form** and **JSON** without losing state — both edit the same `DraftPage` in the store. Form is a view over JSON, not a separate model.
- [ ] "Revert to template defaults" button — restores args to the template's declared defaults without touching page state.

**Proves:** the form is a convenience over a real document, and the document stays the source of truth.

### Stage 4 — Undo/redo + history UI for dashboards

**Goal:** dashboards get the same history affordances flows already have. Zero new backend.

- [ ] Ctrl+Z / Ctrl+Shift+Z in the builder call `POST /flows/{page_node_id}/undo` / `/redo` with `expected_head` from the store. No client-side command stack in v1 — every edit is already a debounced server revision (see UNDO-REDO.md's final note on client-side stacks being a later UX layer, not a substitute).
- [ ] History panel: revision timeline per the Phase 3 history UI in UNDO-REDO.md, same component if it lands first.
- [ ] Revert button on any revision → `POST /flows/{id}/revert`.
- [ ] Toast on undo when the reverted revision touched a binding that points at a widget whose settings changed independently (cross-scope undo mitigation from UNDO-REDO.md §"Cross-scope undo is intentionally independent").

**Proves:** history works end-to-end on dashboards, reusing shipped machinery.

### Stage 5 — Canvas mode (deferred, explicitly out of v1 scope)

Only pursued if Stages 1–4 ship and real users hit template gaps we can't close by adding templates. If we do:

- [ ] `canvas/` directory with `PanZoomCanvas.tsx` + `GridCanvas.tsx`, selected by a `mode` field on the draft. Feature matrix declared up front; no mode `if`s sprinkled in render.
- [ ] Bindings-as-a-graph overlay on hover — reuses the subscription plan already returned by `/ui/resolve`, no new backend.
- [ ] Widget palette: click-to-add with `findNextFreeSlot`, not drag-from-palette (the rubix scene-builder pattern).

Left open deliberately. Design doc for this stage happens when — and only when — we have evidence it's needed.

---

## Data model (v1, no schema migration)

Dashboards are already `ui.page` nodes. The builder edits those nodes through the shipped flow-revision surface. No new tables, no new columns. The "draft" in the frontend store is a local buffer; the server-of-record is the flow revision.

One open question: pages made via the builder vs pages hand-authored in YAML manifests need to coexist. Resolution: the manifest path continues to work (read-only in the builder, with a "this page is defined in a manifest — fork to edit in DB" affordance). No auto-migration.

---

## Risks and chosen mitigations

| Risk | Mitigation |
|---|---|
| Template library covers <80% of real pages | Ship 5 templates; measure gap before committing to Stage 5 |
| Form generator balloons into bespoke-per-field code | Hard rule: every field type is a registered component; no template-specific forms |
| Binding validation drifts between frontend hints and backend truth | One validator, exposed via `/ui/validate`; frontend calls it, never re-implements it |
| Autosave thrash during typing | Debounce (500 ms); only edits the DB when the document parses cleanly |
| Revert of a template-referencing page when the template version has moved | Pin template version in `ui.page.templateRef.version`; surface "template has a newer version" banner with opt-in upgrade |
| Users expect a canvas because every other tool has one | README screenshots + onboarding emphasise template-first; reserve the word "builder" for what we shipped, not what competitors shipped |

---

## Open questions

1. **Who owns template authoring long-term?** v1: us, via YAML. v2: extension authors, via the existing kind-registration path? If the latter, templates need the same version/migration story as kinds (see UNDO-REDO.md §"Historical settings materialisation"). Not v1 blocking.
2. **Thumbnails.** Hand-made PNGs in v1, or server-rendered from a fixed resolve? Hand-made is fine for 5 templates; auto-render becomes attractive past ~20.
3. **Form field registry extensibility.** If a template needs a custom field (e.g. a colour picker wired to the theme), does the template manifest register the component? Or is the field set fixed and templates pick from it? Lean: fixed set in v1, extensibility is a v2 decision.
4. **Multi-page dashboards (tabs/nav).** Covered by `ui.nav`, but the builder UX for composing a multi-page dashboard is under-specified. Stage 2 addresses single pages; multi-page composition is a Stage 2.5 extension.
