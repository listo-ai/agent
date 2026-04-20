# Testing dashboards via the CLI

Goal: author SDUI pages (and block-owned views) entirely from `agent <cmd>` — the frontend becomes a pure projector of whatever JSON the agent serves. This is the workflow AI assistants should use when a user prompt says *"make me a dashboard for this flow"* or *"add a panel that shows the heartbeat live."*

See also:

- [docs/design/CLI.md](../design/CLI.md) — command tree, global flags, JSON / exit-code contracts.
- [docs/design/SDUI.md](../design/SDUI.md) — IR vocabulary, binding grammar, render/resolve semantics.
- [docs/testing/FLOW.md](./FLOW.md) — sibling doc for flow + node authoring.

---

## The mental model

A dashboard is a `ui.page` graph node whose `layout` slot holds a typed `ComponentTree` (JSON). The renderer reads that tree and projects it to React. Every interactive node (button, form, row-click) routes back through `POST /api/v1/ui/action`. No client knows what a BACnet device or a heartbeat is — the IR is domain-neutral.

Four routes matter in Studio:

| Route | CLI behind it | Use when |
|---|---|---|
| `/pages` | `GET /api/v1/nodes?filter=kind==ui.page` | Browsing every `ui.page` node; "New page" button creates one at root + seeds a layout + jumps to the builder |
| `/pages/<id>/edit` | `POST /api/v1/ui/resolve` with inline `layout` override + `POST /api/v1/slots` with `expected_generation` | Authoring a `ui.page` interactively — Monaco JSON editor, live preview, OCC-guarded autosave, non-dismissable conflict banner. See [sessions/DASHBOARD-BUILDER.md](../sessions/DASHBOARD-BUILDER.md). |
| `/ui/<page-id>` | `POST /api/v1/ui/resolve` → reads the page's `layout` slot | Rendering an authored page you control end-to-end |
| `/render/<node-id>` | `GET /api/v1/ui/render?target=<id>` → looks up the target's `KindManifest.views`, substitutes `$target` bindings | Rendering "the default view for this node's kind" without authoring a page per instance |

All four backend endpoints return the same `{ render, subscriptions, meta }` shape. The renderer doesn't care which endpoint produced the tree. The builder at `/pages/<id>/edit` is a parallel affordance to the CLI — both write the same `ui.page.layout` slot.

LLM heuristic: if the user wants *one specific dashboard* → author a `ui.page`. If the user wants *every instance of kind X to have a page* → ship a `views:` entry on the kind manifest (block-authored, not CLI) and use `/render/<id>` to hit any instance.

---

## Session bootstrap

```bash
agent health
agent capabilities -o json   # pin the ir_version
agent ui vocabulary -o json  # the full IR component-union JSON Schema
```

`agent ui vocabulary` is the authoritative list of component types you can emit. An LLM reads this once, then authors without probing — it's the equivalent of MCP `tools/list` for SDUI components.

---

## Authoring a `ui.page` — end to end

### Step 1 — create the page node

`ui.page` lives under any folder. A common convention is `/dashboards/<name>`:

```bash
agent nodes create / sys.core.folder dashboards    # once
agent nodes create /dashboards ui.page heartbeat-monitor
```

### Step 2 — write the `layout` slot

This is the tree the renderer projects. `layout` is a single JSON value with shape `{ ir_version, root: ComponentTree }`:

```bash
agent slots write /dashboards/heartbeat-monitor layout '{
  "ir_version": 1,
  "root": {
    "type": "page",
    "id": "root",
    "title": "Heartbeat Monitor",
    "children": [
      {"type":"row","id":"hdr","children":[
        {"type":"heading","id":"h","content":"flow-1 / heartbeat","level":2},
        {"type":"badge","id":"b","label":"live","intent":"ok"}
      ]},
      {"type":"table","id":"t",
       "source":{"query":"path==/flow-1/heartbeat","subscribe":true},
       "columns":[
         {"title":"Node","field":"path"},
         {"title":"Count","field":"slots.out.payload.count"},
         {"title":"State","field":"slots.out.payload.state"}
       ],
       "page_size":10}
    ]
  }
}'
```

Key points an LLM should remember:

- Every component gets a **stable `id`**. Subscription plans key off these ids; two components with the same id break live-update routing.
- `source.subscribe: true` on a `table` is what turns on live rows — the backend derives `node.<id>.slot.<name>` subjects for every node the query matches, and the React hook patches the table's cached row in place on each matching event (no refetch per tick).
- `source.query` is RSQL (same grammar as `agent nodes list --filter`). Test queries with `agent ui table --query '…'` first.
- `columns[].field` is a dot-path into the row JSON (`path`, `kind`, `slots.<name>`, `parent_id`).
- **Output-slot envelope.** Source and transformer kinds write a `Msg` envelope (`{_msgid, topic?, payload: <real value>}`) to their output slot — not a bare scalar. A `field: "slots.out"` cell renders `[object Object]`. Drill into `slots.out.payload.count` / `slots.out.payload.state` for the scalars (the heartbeat example above). The envelope is exactly what [spi/src/msg.rs](../../crates/spi/src/msg.rs) serialises; Stage 2 of [NODE-RED-MODEL.md](../design/NODE-RED-MODEL.md) stripped `_ts` / `_source` / `_parentid` — timestamps ride the SSE frame, provenance moves to trace context.

### Step 3 — verify before opening Studio

Dry-run validates both **shape** and **bindings**. Three failure modes it catches:

1. Malformed component tree (unknown `type`, missing required fields, wrong enum values).
2. Unresolved bindings — `{{$target.not_a_slot}}`, `{{$stack.unknown}}`, ref-walks through missing nodes.
3. `$page.*` reads whose key is not declared in the page's `page_state_schema.properties` slot (only when the page has a non-null schema).

```bash
agent ui resolve --page <page-id> --dry-run -o json
```

If `{"errors": []}` it's good. Otherwise you get a list of `{location, message}` with the exact failure:

```json
{ "errors": [
  { "location": "page/<id>/layout", "message": "layout is not a valid ComponentTree: unknown variant `heading2`, expected one of ..." },
  { "location": "root.children[2].label", "message": "binding must start with `$stack`, `$self`, `$user`, or `$page` — got `$target`" },
  { "location": "root.children[3].content", "message": "unresolved $page.missing — not declared in page_state_schema.properties" }
] }
```

Full resolve (returns the tree + subscription plan):

```bash
agent ui resolve --page <page-id> -o json | jq '.render, .subscriptions'
```

### Step 4 — open in Studio

```
http://localhost:3000/ui/<page-id>
```

---

## Making a page live — subscription plans

The backend emits a subscription plan alongside every resolve/render response. It has two shapes:

| Plan shape | `widget_id` | Triggers | Client behaviour |
|---|---|---|---|
| **Tree-binding plan** | target node UUID | `{{$target.<slot>}}` references in the template | Re-resolve the whole tree → substituted values refresh |
| **Table plan** | authored table component id (e.g. `"t"`) | `{type:"table", source:{subscribe:true, query:…}}` | Invalidate just that table's React-Query key → rows refetch |

You don't compose plans — you compose *templates*, and the backend derives the plan. Rules of thumb:

- Want a slot value to appear in a header / badge / text? Use `{{$target.<slot>}}`. Works with `/render/<id>`. Does *not* work on `/ui/<page>` since there's no `$target`.
- Want a live list of nodes matching a query? Use a `table` with `subscribe: true`.
- Want both? Combine them in one page.

Test the plan with:

```bash
agent ui resolve --page <page-id> -o json | jq '.subscriptions'
```

Each plan's `subjects` array should contain one `node.<id>.slot.<name>` per slot the template references. Empty subscriptions on a page with a subscribed table = query matched zero nodes → check the query string.

---

## Authoring a kind view — block-authored default screens

When every instance of `sys.logic.heartbeat` should get the same dashboard, don't create N pages — add a `views:` entry to the kind manifest. The `/ui/render` endpoint picks it up automatically.

Kind manifests live in YAML alongside the block ([`crates/domain-logic/manifests/heartbeat.yaml`](../../crates/domain-logic/manifests/heartbeat.yaml) is the live example):

```yaml
views:
  - id: overview
    title: Heartbeat overview
    priority: 100
    template:
      ir_version: 1
      root:
        type: page
        id: hb-overview
        title: "{{$target.name}}"
        children:
          - type: row
            id: hdr
            children:
              - type: heading
                id: h
                content: "{{$target.path}}"
                level: 2
              - type: badge
                id: state
                label: "{{$target.out.payload.state}}"
                intent: info
```

Then hit any instance:

```bash
agent ui render --target <instance-node-id>
agent ui render --target <instance-node-id> --view overview   # explicit
```

Supported `$target` bindings: `id`, `path`, `name`, `kind`, and any slot name — with an optional dot-path into the slot value (e.g. `$target.out.payload.state` walks into the Msg envelope).

When no `view` parameter is supplied, the highest-`priority` view wins; ties break by declaration order.

If a target is a `ui.page`, `/render/<id>` falls through to the page's `layout` slot — same tree `/ui/resolve` returns. This is why Studio sidebar clicks can use a single route regardless of whether the clicked node is an authored page or a block-kind instance.

---

## Editing a running dashboard

| Intent | Command |
|---|---|
| Tweak the layout | `agent slots write /dashboards/foo layout '<new JSON>'` |
| Tweak with OCC guard (recommended when the Studio builder is also open) | `agent slots write /dashboards/foo layout '<new JSON>' --expected-generation <N>` — 409 + `generation_mismatch` if someone else wrote first |
| Fix one component | read `layout`, patch in JSON, rewrite the whole slot — no RFC-6902 patches in Phase 1 |
| Delete the page | `agent nodes delete /dashboards/foo` |
| Invalidate a client's cache | every write auto-emits `slot_changed` — clients pick it up over SSE |

There is no migration / version concern: rewriting the `layout` slot replaces the tree atomically. The next `/ui/resolve` call returns the new tree.

If the Studio builder has the same page open at `/pages/<id>/edit`, an unguarded CLI write will still succeed — but the next keystroke in the builder will trip its conflict banner (non-dismissable, reload-or-export). Using `--expected-generation` from the CLI is the symmetrical guard: the CLI refuses to clobber a concurrent builder save.

---

## Actions — wiring buttons to the backend

Every interactive component (`button`, `link`, `menu` item, `form.submit`, table `row_action`) carries an `action`:

```json
{ "type": "button", "id": "scan", "label": "Scan devices",
  "action": { "handler": "bacnet.scan", "args": { "network": "$target.id" } } }
```

The handler must be registered in the agent's `HandlerRegistry` (Rust-side). To test a handler from the CLI:

```bash
agent ui action \
  --handler bacnet.scan \
  --args '{"network":"<node-id>"}' \
  --target scan
```

Response is a tagged union — `toast` / `navigate` / `patch` / `full_render` / `form_errors` / `download` / `stream` / `none`. The client applies each variant automatically; the CLI just pretty-prints the response.

Unregistered handler → `404 not_found` with stable error shape. Good failure mode for LLMs: if you emit an `action` pointing at a handler that doesn't exist, dry-run resolve still succeeds (the tree is valid), but clicking the button surfaces the missing handler at runtime.

---

## Tables, in isolation

Sometimes you don't need a page — just a query result:

```bash
agent ui table --query 'kind==sys.logic.heartbeat' --page 1 --size 50 -o json
```

This is the same endpoint the authored `table` component hits internally. Useful for validating a query before baking it into a layout.

---

## Common failures and what they mean

Every error follows [CLI.md § 1](../design/CLI.md#1-deterministic-json-output-contract-not-best-effort). Specific codes for dashboard authoring:

| Code / status | Cause | Fix |
|---|---|---|
| `422 unprocessable_entity` + `layout is not a valid ComponentTree` | Unknown component `type`, missing required field | Check against `agent ui vocabulary -o json` |
| `422` + `page has no 'layout' slot` | You called `/ui/resolve` on a page that was never written to | `agent slots write …/layout '{...}'` |
| `dry-run errors` with `location: root.<path>` + binding message | Unresolved `$stack`/`$self`/`$user`/`$page` binding or unknown source | Check the binding grammar in SDUI.md § "Bindings"; `$target.*` only works on `/ui/render`, never on `/ui/resolve` |
| `dry-run errors` with `unresolved $page.<field>` | Page has a non-null `page_state_schema` and the binding references an undeclared key | Add the key to the schema's `properties`, or remove the binding |
| `404 not_found` (`/render`) | Target node doesn't exist, OR its kind has no `views` declared | `agent nodes get <path>` to confirm; `agent kinds list` to confirm views |
| `409 generation_mismatch` (`slots write --expected-generation`) | Someone else wrote to the slot between your read and write | Re-read the node (`agent nodes get <path> -o json`), rebase your edits on the new generation, retry |
| `413 payload_too_large` | Tree exceeded a DoS limit (see SDUI.md § "Size & DoS limits") | Split into sub-pages or reduce row counts |
| `subscriptions: []` when you expected updates | Table has `subscribe: false`, or the query matched zero nodes | Flip `subscribe: true`; test the query independently |
| Row cell shows `[object Object]` | Slot holds a Msg envelope `{_msgid, topic?, payload}` | Drill into the payload: `slots.<name>.payload.<field>` in `columns[].field` |

---

## Recipe — "give me a live dashboard for /flow-1"

End-to-end, one script. Assumes `/flow-1/heartbeat` already exists (see [FLOW.md](./FLOW.md)):

```bash
set -e

# 1. Ensure a home for the page
agent nodes list --filter 'kind==sys.core.folder;path==/dashboards' -o json \
  | jq -e '.data | length > 0' \
  || agent nodes create / sys.core.folder dashboards

# 2. Create the page
PAGE_JSON=$(agent nodes create /dashboards ui.page flow-1-overview -o json)
PAGE_ID=$(echo "$PAGE_JSON" | jq -r '.id')

# 3. Write the layout (live table + heading)
agent slots write /dashboards/flow-1-overview layout '{
  "ir_version":1,
  "root":{
    "type":"page","id":"root","title":"flow-1 overview",
    "children":[
      {"type":"heading","id":"h","content":"flow-1","level":1},
      {"type":"table","id":"nodes",
       "source":{"query":"parent_path==/flow-1","subscribe":true},
       "columns":[
         {"title":"Name","field":"path"},
         {"title":"Kind","field":"kind"},
         {"title":"Lifecycle","field":"lifecycle"}
       ],
       "page_size":25}
    ]
  }
}'

# 4. Validate
agent ui resolve --page "$PAGE_ID" --dry-run -o json | jq '.errors'

# 5. Confirm subscription wiring — should be non-empty once nodes exist
agent ui resolve --page "$PAGE_ID" -o json | jq '.subscriptions'

echo "Open http://localhost:3000/ui/$PAGE_ID"
```

What the LLM should show the user:

- The page URL (`/ui/<id>`).
- The subscription plan — evidence that "live" is actually wired.
- The dry-run result — evidence the tree parsed.

---

## Discovery checklist for LLMs

Before authoring dashboards from a user prompt, read once per session:

1. `agent capabilities -o json` — pin `ir_version`.
2. `agent ui vocabulary -o json` — the full IR schema (component union).
3. `agent schema ui resolve -o json` / `agent schema ui render -o json` / `agent schema ui action -o json` — request/response shapes.
4. `agent kinds list -o json` — to know which kinds have `views` declared.

Then:

- **One page, one instance** → `agent nodes create … ui.page … && agent slots write … layout '<json>'`.
- **All instances of a kind** → author `views:` on the kind manifest, use `/render/<id>`.
- **Ad-hoc query result** → `agent ui table --query '…'`.
- **Test an action handler** → `agent ui action --handler … --args '…'`.

Everything past this is composition of the eight commands under `agent ui` and `agent slots`. No hidden surface.
