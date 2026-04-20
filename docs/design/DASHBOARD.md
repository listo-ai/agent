# Scope — Generic Dashboard Backend

A framework-level backend for user-authored dashboards. Domain-agnostic: ships zero concepts about buildings, devices, tickets, CRMs, or anything else. Users and blocks bring the domain; the framework provides structure, reuse, and wiring.

## Current status

M1–M5 are shipped and tested. The resolver, context stack, binding engine, cache-key, ACL redaction, widget-type registry, audit events, and subscription-plan emission are live across three crates (`dashboard-nodes`, `dashboard-runtime`, `dashboard-transport`) and mirrored in the Rust client, CLI, and TS client per [NEW-API.md](NEW-API.md). See the [Milestones](#milestones) table for per-milestone status.

## Goal

Let users compose navigable, reusable, context-driven dashboards out of nodes — authored by AI, by drag/drop, or by hand — with zero per-dashboard backend code.

## Non-goals

- Rendering. This is backend only. json-render + Studio own rendering.
- Widget implementations. Widget *types* are contributed by blocks, not the framework.
- Domain vocabulary. No assumptions about what a user's nodes mean.
- Time-series storage. Telemetry lives in the TSDB; widgets query it through existing telemetry APIs.
- A second query language. Reuse `/crates/query/` and the existing template resolver.
- Live collaboration / multi-cursor editing. v1 is single-author-at-a-time.
- Theming. `ui.theme` deferred to v2 — see "Deferred" below.

## The model — four node kinds (v1)

All UI artifacts are nodes in the unified graph. They inherit RBAC, audit, versioning, query, subscriptions, import/export from the node model for free.

| Kind | Role |
|---|---|
| `ui.nav` | A navigation entry; optionally contributes a frame to the context stack when entered |
| `ui.template` | A layout with typed parameter holes (a page blueprint) |
| `ui.page` | Either a versioned `templateRef` + bound args, or a standalone layout |
| `ui.widget` | A placed widget instance with bindings into stack / self / user / page state |

Frames are generic `{ nodeRef, alias }` pairs; aliases are author-chosen strings the framework never interprets.

`ui.nav`, `ui.template`, and `ui.page` carry the `isContainer` facet so the graph's containment validator accepts them under `sys.core.station` and any `isContainer` parent. This is framework wiring, not a domain concept — it leverages the existing facet the node model already uses for placement grouping.

## The context stack

- Nav walk from root → target `ui.nav` produces an ordered list of frames.
- Each `ui.nav` may contribute one frame: `{ alias, nodeRef }`.
- Stack is addressed by alias (`$stack.<alias>`) or index (`$stack[N]`, `$stack[-1]`).
- Serialized stack == URL. Deep linking is trivial; no separate router config.

### Alias collision rule

Aliases are author-chosen and can collide along a nav walk. Rule:

- **Alias lookup (`$stack.<alias>`)** resolves to the **deepest (innermost) frame** carrying that alias — standard lexical shadowing.
- **Index lookup (`$stack[N]`)** is unambiguous and always available.
- Save-time validation **warns** on alias shadowing in a nav subtree but does not error; shadowing is sometimes intentional (e.g. nested "current" concepts).
- Widgets that need an outer-shadowed frame must use index addressing.

## Bindings

Exactly four binding sources. Nothing domain-specific, nothing route-specific.

| Source | Example | Meaning | Authority |
|---|---|---|---|
| `$stack.<alias>` / `$stack[i]` | `$stack.target.id` | Nav frame lookup | Derived from nav walk at resolve time |
| `$self.<slot>` | `$self.name` | Slot on the current widget/page node | Server (node graph) |
| `$user.<claim>` | `$user.orgId` | Verified auth claim | Server (`AuthContext`) |
| `$page.<name>` | `$page.selectedRow` | Page-local state | **Client-owned**, sent as input to `/ui/resolve` |

Bindings resolve to primitive values or `NodeRef`s. Ref navigation (`ref(x).slot`) is delegated to the existing query engine's ref walker — whatever ref slots a user's nodes define.

### `$page` state lifecycle

- **Owner:** the client (Studio). The server stores no per-session page state.
- **Shape:** an opaque JSON object, capped at **64 KB**. Schema declared by the page (`pageState` JSON Schema field) and validated on each resolve call.
- **Transport:** sent in every `/ui/resolve` request body.
- **Persistence:** never. If a page wants sticky state (e.g. user's last-picked filter), the client opts in by writing to user preferences via the generic node API — not through `$page`.
- **Navigation:** `$page` is scoped to a single mounted page instance. Navigating away discards it. Back-button restoration is the client's concern (history state).

Keeping `$page` as a resolve-time input preserves server statelessness and keeps the resolver referentially transparent given its explicit inputs.

## The resolver

**Referentially transparent given its inputs.** Not "pure" in a loose sense — it reads mutable state (nodes, ACLs, widget registry), so the cache key is:

```
cache_key = hash(
  pageRef,
  page_node_version,
  template_node_version,    // if page has templateRef
  widget_node_versions[],   // all ui.widget children
  bound_node_versions[],    // every node any binding resolves to
  auth_subject,
  auth_role_epoch,          // RBAC changes bust the cache
  stack,
  pageState_hash,
  widget_registry_version,
)
```

Caches are advisory; a miss costs a resolve, not correctness. Invalidation is driven by node-version bumps (already in the node model) and the auth role epoch (already in `/crates/auth`).

## ACL policy

Single, explicit rule for mixed pages:

- **Per-widget redaction**, not page-level 403.
- If any binding in a widget resolves to a node the caller cannot read, the widget is replaced in the render tree with a typed placeholder: `{ "kind": "ui.widget.forbidden", "reason": "acl" }`. Renderer shows a neutral stub.
- If the `ui.page` node itself is unreadable, resolve returns 403.
- Forbidden-widget counts are returned in `ResolveResponse.meta` so clients can surface "5 widgets hidden by permissions."
- Audit: each redaction emits one audit event with `(widgetId, boundNodeId, authSubject)`.

## Subscriptions ↔ resolve reconciliation

The resolver emits a **static snapshot plus a subscription plan**. Both come from the same pass over bindings.

```
ResolveResponse = {
  render: <resolved JSON tree>,
  subscriptions: [
    { widgetId, subjects: ["node.<id>.slot.<name>", ...], debounceMs: 250 }
  ],
  meta: { cacheKey, forbiddenCount, pageStateSchema, ... }
}
```

Rules:

- Each widget's subscription subjects are **derived mechanically from its bindings** — the same ref walk that produced concrete node ids produces concrete subscribe subjects. No per-widget hand-written subscribe logic.
- ACL is applied to subjects before they leave the backend; forbidden-node subjects are dropped.
- **Mid-session mutation handling:**
  - Referenced node *slot changes* → NATS event → client applies diff to the widget without re-resolving.
  - Referenced node *moved* (path changes) or *retyped* → server emits a `ui.invalidate` event on a page-scoped subject the client is always subscribed to; client re-resolves.
  - Referenced node *deleted* → server emits `ui.invalidate`; on re-resolve the widget becomes `ui.widget.dangling` in the render tree (typed placeholder, like the forbidden stub).
- All of this runs on the existing messaging crate's subjects and outbox; no dashboard-specific broker.

## Template versioning & migration

Templates evolve. Pages must not silently break.

- **`ui.page.templateRef` is pinned by version**, not by id alone: `{ id, version }`. Templates use the node model's existing version field.
- **Save-time parameter-contract validation** runs against the pinned template version only.
- **Template author edits** produce a new version; existing pages keep rendering against their pinned version.
- **Migration protocol:** when a template author publishes a new version, the backend runs parameter-contract diff between old and new. If non-breaking (additive optional holes, widened types), pages can auto-upgrade on next save. If breaking, pages stay on the old version and the UI surfaces "template update available — X pages need migration."
- **Lazy re-validation:** `/ui/resolve` validates contract only if the pinned version no longer exists (which means the template was deleted — rare, treated as `ui.page.dangling`). No re-validation on every resolve.
- **Breaking-change definition** is documented in `ui-nodes/` next to the parameter-contract validator and covered by tests.

## Backend responsibilities (in scope)

1. **Schemas** — JSON Schema + Rust structs for the four node kinds; registered with the node model.
2. **Parameter-contract validation** — `ui.page` binding a versioned `ui.template` must satisfy the template's `requires` shape; enforced at save time and on template-version changes.
3. **Context resolver** — given a `ui.nav` path (or explicit stack), produce a validated context stack with alias-collision handling.
4. **Binding resolver** — given a `ui.page` + context stack + pageState, produce a resolved render tree **and** a subscription plan. Cache-key-indexed.
5. **Query templating integration** — feeds `$stack.*`, `$self.*`, `$user.*`, `$page.*` into the existing template engine used by RSQL.
6. **Widget-type registry** — blocks register widget types via `contributions.widgets[]`. Backend validates that a `ui.widget.widgetType` references a known type.
7. **ACL redaction** — per-widget forbidden placeholders with audit events.
8. **Transport** — REST endpoints (gRPC deferred; the Rust/TS clients + CLI all speak REST):
   - `GET /api/v1/ui/nav?root=<id>` → nav tree slice rooted at the given `ui.nav` id
   - `POST /api/v1/ui/resolve` — body `{ page_ref, stack, page_state, dry_run?, auth_subject?, user_claims? }`. With `dry_run: true`, the same endpoint performs validation only and returns structured errors (unbound holes, unknown widget types, missing frames) without producing a render tree. Single endpoint, no drift.
   - Standard node CRUD (already generic) handles authoring.
   - Paths are versioned under `/api/v1/` per [VERSIONING.md](VERSIONING.md); bumping requires a 12-month deprecation window.
9. **Subscription plan emission** — mechanical derivation from bindings; ACL-filtered.
10. **Invalidation signal** — `ui.invalidate` events on page-scoped subjects when bound nodes move / retype / delete.
11. **Import/export** — a dashboard = a subtree of UI nodes; export uses the existing node subtree exporter.
12. **Size & DoS limits** — enforced at resolve time:
    - Max widgets per page: **200**
    - Max nav depth: **16**
    - Max resolved render tree: **2 MB**
    - Max binding depth (ref walks): **8**
    - Max subscription subjects per page: **500**
    - `pageState` body cap: **64 KB**
    - Exceeding any cap returns 413 with a precise error.

## Deferred (v2)

- **`ui.theme`** — needs a proper token-resolution order spec (`theme × page × widget`, with widget-declared token slots) before it's worth shipping. Placeholder in v1 is worse than nothing.
- Per-user page overrides / favorites (`ui.overlay` kind).
- Scheduled snapshots / PDF export.
- iframe embedding.
- Client-side drag/drop undo beyond normal node versioning.

## Crate layout

Following `CODE-LAYOUT.md`. Prefixed `dashboard-` to avoid collision with any future frontend `ui-*` crates:

```
/crates
  /dashboard-nodes         # Domain — the four node kinds, schemas, contract validation, version-diff rules
  /dashboard-runtime       # Domain — context stack, binding resolver, subscription-plan emitter, cache-key
  /dashboard-transport     # Transport — REST + gRPC handlers for /ui/nav and /ui/resolve
```

No `dashboard-data` crate — persistence goes through the existing node repositories. If specialised indexes prove necessary (e.g. widget → bound-node reverse lookup for invalidation), add migrations to `/crates/data-*`.

## Dependencies on existing work

- `/crates/spi` — add proto messages for `ResolveRequest`, `ResolveResponse`, `NavSlice`, `SubscriptionPlan`, `UiInvalidate`.
- `/crates/query` — reused as-is for template resolution and ref walking.
- `/crates/auth` — `AuthContext` is the source of `$user.*`; role epoch drives cache invalidation.
- `/crates/messaging` — subscription subjects and `ui.invalidate` events.
- Node model — the four kinds register as node types; versioning and ACLs inherited.

## Acceptance criteria

- A `ui.template` with two parameter holes can be instantiated by N `ui.page` nodes, each pinning different node refs, with no copy of the layout.
- A single `ui.page` renders differently depending on the nav path that leads to it, driven solely by stack frames.
- Alias shadowing along a nav walk resolves to the innermost frame for `$stack.<alias>` and emits a save-time warning.
- `POST /ui/resolve` with `dryRun: true` returns precise errors for unbound holes, unknown widget types, missing frames, oversized `pageState`, and type mismatches.
- A caller without read access on a bound node sees a `ui.widget.forbidden` placeholder for that widget; other widgets on the page render normally; one audit event is emitted per redaction.
- Deleting a bound node mid-session emits `ui.invalidate`; the next resolve returns `ui.widget.dangling`.
- Editing a template to a non-breaking new version auto-upgrades dependent pages on next save; a breaking edit leaves pages pinned to the old version and surfaces a migration signal.
- Subscription subjects emitted by the resolver match exactly the set of bound node slots the caller has read access to.
- Adding a new widget type in an block requires zero backend changes.
- Resolve of a well-formed 200-widget page completes within agreed latency budget (TBD; measured under M3).
- Zero occurrences of domain-specific strings in `/crates/dashboard-*`.

## Milestones

| M | Status | Deliverable |
|---|---|---|
| M1 | ✅ shipped | Four node kinds + YAML manifests in [`crates/dashboard-nodes`](../../crates/dashboard-nodes); CRUD via generic node API; query engine finds them; parameter-contract validator (`ui.page.bound_args` ↔ `ui.template.requires`) with `ref`/`string`/`number`/`bool` types. |
| M2 | ✅ shipped | Context stack (innermost-alias-shadowing rule) + binding parser/evaluator (`$stack`/`$self`/`$user`/`$page` + multi-hop ref walks) + cache-key derivation in [`crates/dashboard-runtime`](../../crates/dashboard-runtime); 27 unit tests against `InMemoryReader` fixtures. |
| M3 | ✅ shipped | `GET /api/v1/ui/nav` + `POST /api/v1/ui/resolve` (with `dry_run`) in [`crates/dashboard-transport`](../../crates/dashboard-transport); all six size/DoS limits enforced (`MAX_WIDGETS_PER_PAGE` etc.); fixture gate covers ok / not-found / payload-too-large / dry-run scenarios. (OpenAPI generation deferred to Stage 9 per repo convention.) |
| M4 | ✅ shipped | `WidgetRegistry` with version-bumped cache invalidation; `AclCheck` trait seam (`AllowAll` default, `DenyNodes` for tests); per-widget `RecordingReader` ACL check; `RenderedWidget` is a `#[serde(tag = "kind")]` enum with `ui.widget` / `ui.widget.forbidden` / `ui.widget.dangling` variants; `AuditSink` trait (`TracingAudit` default) emits one event per redaction. |
| M5 | ✅ shipped (emission); 🟡 bridge pending | `SubscriptionPlan { widget_id, subjects, debounce_ms }` emitted in `ResolveResponse::Ok`; subjects (`node.<id>.slot.<name>`) derived mechanically from per-widget slot-access log, deduplicated, ACL-filtered; `InvalidateSink` trait seam with `TracingInvalidate`/`RecordingInvalidate` impls. The graph-event bridge that actually emits `ui.invalidate` on node move/retype/delete lands when the `messaging` crate ships real NATS subjects. |
| M6 | ⏳ pending | Template version-diff migration protocol end-to-end (auto-upgrade path + breaking signal). |
| M7 | ⏳ pending | Subtree import/export; full acceptance suite green; latency budget measured and recorded. |

Client-parity per [NEW-API.md](NEW-API.md) is complete for every shipped endpoint: Rust client (`agent-client::ui`), CLI (`agent ui nav`, `agent ui resolve`), TS client (`AgentClient.ui`), CLI `CommandMeta` for `--help-json` / `agent schema`, and pinned fixtures under `clients/contracts/fixtures/cli-output/ui-nav/` + `ui-resolve/`.

## One-line summary

**A small, domain-agnostic backend that adds four UI node kinds, a context stack with explicit alias-collision and ACL rules, and a resolver that emits both a render tree and a mechanically-derived subscription plan — everything else (RBAC, audit, versioning, query, pub/sub, import/export) is inherited from the node graph, so the framework ships with zero dashboard-specific infrastructure beyond resolution, validation, and invalidation signalling.**
