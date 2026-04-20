# Tags + Node Presentation + Query Integration

Session scope for introducing:

- Human-friendly tags on nodes/resources using your notation:
  - label list: `[code, person, things]`
  - key/value tags: `{site:abc}`
- Runtime node presentation updates (`status/color/icon/message`) for the canvas
- First-class filtering through the generic query system in [QUERY-LANG.md](../design/QUERY-LANG.md)

Authoritative neighbors: [FLOW-UI.md](FLOW-UI.md), [RUNTIME.md](../design/RUNTIME.md), [QUERY-LANG.md](../design/QUERY-LANG.md), [NODE-MUTATION.md](../design/NODE-MUTATION.md).

## How this fits the mutation rulebook

Per [NODE-MUTATION.md](../design/NODE-MUTATION.md), a node has exactly four mutation primitives. Nothing in this doc introduces a fifth. Each feature below maps to an existing primitive:

| Feature | Primitive | Where the value lives |
|---|---|---|
| Tag a node with `[labels]` and `{kv}` | **Write slot** (`POST /api/v1/slots`) | `config.tags` slot on the node |
| Runtime status / color / icon update | **NodePresentationUpdate envelope** (slot-shaped, bus-delivered) | Per-node presentation store (not persisted on every tick) |
| Query by tag or status | **Generic query** (no mutation) | `QuerySchema` exposes `tags.labels`, `tags.kv.*`, `presentation.status` |
| Rename or re-home a tagged node | **Patch structure** (`PATCH /api/v1/node`) — tags and presentation follow automatically via subtree repath | unchanged — the slot stays on the same `NodeId` |

No new REST endpoint is added for tags. No new Rust method is added on `GraphStore` for tags. The existing slot-write path does the work; the query pipeline makes it discoverable.

## Goal

Ship one consistent model for classification + operational visibility:

1. Operators can tag nodes/resources quickly with labels and key/value metadata.
2. Runtime can decorate nodes with status/icon/color/message without mutating manifests.
3. API/CLI/MCP can filter by tags and status through the same generic AST pipeline (no custom per-resource parser).

## UX and syntax

Accepted user-facing shorthand:

- Labels: `[code, person, things]`
- Key/value: `{site:abc, zone:w1}`
- Combined: `[code,person]{site:abc}`

Normalization rules:

| Rule | Behavior |
|---|---|
| Case | Lowercase canonical form (`Code` → `code`) |
| Whitespace | Trim around labels/keys/values |
| Duplicates | Remove duplicates in labels |
| Allowed chars (labels) | `a-z0-9._-/` |
| Allowed chars (keys) | `a-z0-9._-/` |
| Allowed chars (values) | UTF-8 text, trimmed, max length 128, no control chars |

Parser rules for ambiguity (must be deterministic):

| Case | Rule |
|---|---|
| Comma/colon in values | Require quoted values: `{note:"hello, world"}`, `{path:"a:b"}` |
| Quotes in values | Escape with `\"` |
| Empty values | Disallowed (`{site:}` invalid) |
| Unquoted values | Allowed only when they contain no `,` or `:` |
| Labels with special chars | Allowed by normalization set; URL-encoded at transport boundaries |

Reserved key namespace:

- `sys.*` is reserved for platform-managed tags.
- User-created keys under `sys.*` are rejected by validation.

Canonical internal type:

```yaml
tags:
  labels: ["code", "person", "things"]
  kv:
    site: "abc"
    zone: "w1"
```

## Data model

Tags are a config-role slot on any taggable entity (node first, extensible to flows/devices/users):

- Slot path: `config.tags`
- Slot schema:
  - `labels: string[]` (unique)
  - `kv: object<string,string>`

Node presentation is runtime-state, separate from config defaults:

```yaml
presentation:
  status: None | Unknown | Ok | Warning | Error
  color: string?     # token or hex
  icon: string?      # lucide name
  message: string?   # tooltip text
```

Status semantics:

- `None`: node does not report status (hide status dot entirely)
- `Unknown`: node reports status, but no current health yet (gray dot)
- `Ok|Warning|Error`: operational states

Kind capability flag:

- `reports_status: bool` in kind metadata.
- If `reports_status=false`, default status is `None`; runtime should not emit status updates for that node kind.

Manifest defaults remain in kind metadata (color/icon defaults), while runtime emits sparse overrides per node instance.

## Query-system integration (must work with generic pipeline)

### Queryable fields

Every queryable resource that supports tags exposes these logical fields in `QuerySchema`:

| Field | Type | Operators | Notes |
|---|---|---|---|
| `tags.labels` | TextArr | `contains`, `in` | Array membership filters |
| `tags.kv.*` | Text (pattern field) | `eq`, `ne`, `in`, `exists` | Dynamic key lookup (`*` = key name) |
| `presentation.status` | Enum | `eq`, `ne`, `in` | For runtime/ops views |

Pattern-field support is added once in validator/translator (generic), not per resource.

Logical/comparison operators are inherited from the generic query grammar in [QUERY-LANG.md](../design/QUERY-LANG.md):

- `;` = AND
- `,` = OR
- grouped expressions with `(...)`
- comparison operators by field type (`eq/ne/in/contains/exists` for tags; numeric/date comparisons on compatible fields)

### Filter examples (RSQL style)

- Has label `code`: `filter=tags.labels=contains=code`
- Has any of labels code/person: `filter=tags.labels=in=(code,person)`
- `site=abc`: `filter=tags.kv.site==abc`
- `site` key exists: `filter=tags.kv.site=exists=true`
- Warning or error nodes on site abc:
  `filter=tags.kv.site==abc;presentation.status=in=(Warning,Error)`

Parenthesized OR + AND:

- `filter=(tags.kv.site==abc,tags.kv.site==def);tags.labels=contains=code`

### AST representation

No special-case AST node required. Reuse existing field/operator/value forms:

- field path can be dotted (`tags.kv.site`)
- validator resolves `tags.kv.<key>` through schema pattern rules
- translator maps to JSON/array operations by dialect

This preserves the “one parser, one validator, one translator” contract in [QUERY-LANG.md](../design/QUERY-LANG.md).

## Storage + indexing

Target portability across SQLite (edge) and Postgres (cloud):

| Backend | Suggested storage | Suggested index |
|---|---|---|
| Postgres | `tags_labels text[]`, `tags_kv jsonb`, `presentation_status text` | GIN on `tags_labels`, GIN on `tags_kv`, btree on `presentation_status` |
| SQLite | `tags_labels_json text`, `tags_kv_json text`, `presentation_status text` | Config-driven generated columns for selected hot keys + btree indexes |

Implementation detail remains in repo/data layers; query schema stays logical.

Runtime-status persistence contract:

- `presentation_status` is persisted as last-known state for queryability.
- Writer: runtime/engine on status transition only (`None/Unknown/Ok/Warning/Error` change).
- Cadence: transition-based upsert with debounce floor (default 1s per node) to avoid row churn.
- Scope: REST query of `presentation.status` is against last-known persisted status, not a live in-memory-only view.

## Runtime transport for presentation

Add a dedicated update envelope (same bus, separate topic/kind):

```yaml
NodePresentationUpdate:
  node_instance_id: UUID
  seq: u64
  ts: datetime
  patch:
    status: None|Unknown|Ok|Warning|Error?
    color: string?
    icon: string?
    message: string?
  clear: [status|color|icon|message]?
```

Merge semantics:

1. Base from manifest defaults (`color/icon`) + status baseline (`None` if `reports_status=false`, else `Unknown`).
2. Apply patch as sparse update (fields not present are preserved).
3. Per field, apply last-writer-wins using `seq` (and `ts` tie-breaker if needed).
4. `clear` removes only listed fields (field-local clear), restoring fallback defaults for those fields.
5. Out-of-order patches with older `seq` are ignored per field.

Frontend keeps a separate `PresentationStore` keyed by node instance for cheap updates.

## Canvas rendering behavior

Minimal, explicit semantics:

- Status dot on node header:
  - `None` → hidden (node does not report status)
  - `Unknown` → gray (distinct from `Ok`)
  - `Ok` / `Warning` / `Error` → mapped semantic color
- Icon left of title:
  - runtime override icon if present
  - else manifest default icon
- Header accent color:
  - runtime override color
  - else manifest default color
  - else theme default token
- Message shown as tooltip on status dot

## Layered code — how this lands in the repo

Everything below obeys [CODE-LAYOUT.md](../design/CODE-LAYOUT.md): `transport → domain → data`, one responsibility per file, each function under 50 lines.

### Tag write — a slot write, not a new endpoint

**Transport** — `crates/transport-rest/src/routes.rs` already exposes `POST /api/v1/slots`. No handler is added. The frontend posts a normalized `tags` payload; no server-side parsing of `[labels]{kv}` shorthand (parsing happens client-side and in CLI).

**Domain** — `crates/graph/src/slot.rs` (or a new `crates/domain-tags` if validation grows). A domain helper lives alongside other slot writes, not on a bespoke "tags endpoint":

```rust
// crates/domain-tags/src/lib.rs — one reusable validator.
pub struct Tags { pub labels: Vec<String>, pub kv: BTreeMap<String, String> }

pub fn validate_tags(raw: &JsonValue) -> Result<Tags, TagsError> {
    // lowercase, trim, dedupe labels; reject `sys.*` keys; enforce limits.
}
```

Called by `GraphStore::write_slot` for any slot whose schema declares `role=config` and `id=config.tags`. The registration is data (kind manifest), not a code branch.

**Data** — `data-repos` already persists slot JSON. Indexing (`tags_labels text[]`, `tags_kv jsonb`) is added per-backend in `data-sqlite` and `data-postgres`. No new trait, no new repo method.

### Presentation update — runtime bus, not a mutation

**Transport** — envelope arrives over the same message bus the engine already uses. No REST route.

**Domain** — `crates/domain-presentation/src/patch.rs` (new, focused crate) owns the merge rules:

```rust
pub struct Presentation { pub status: Status, pub color: Option<String>, pub icon: Option<String>, pub message: Option<String> }

pub fn apply_patch(base: &Presentation, patch: &PresentationPatch) -> Presentation {
    // last-writer-wins by seq; `clear` strips listed fields.
}
```

Kept out of `graph` because presentation is runtime-only; `graph` stays concerned with identity and slots.

**Data** — last-known `presentation_status` is persisted by the engine on transition (debounced per the runtime contract), using an existing `SlotRepo` write — not a new table. The `presentation` runtime store on the frontend is in-memory.

### Query by tag or status — generic, no per-resource code

**Transport** — `GET /api/v1/nodes?filter=tags.labels=contains=code` already routes through the generic parser.

**Domain** — `crates/query` (shared) resolves `tags.kv.<key>` via one new pattern-field rule. Added once; every resource inherits it.

**Data** — `data-sqlite` and `data-postgres` each map the AST to backend-native JSON / array operators. Shared repo test suite ([data-repos `tests/`](../../crates/data/data-repos/tests)) runs both against the same query corpus in CI.

### Rename or move a tagged node — one PATCH call

Tags and presentation are stored on the node (slot + runtime store keyed by `NodeId`). A structural patch repaths the subtree but never touches slot bodies or presentation state:

```ts
// Frontend
await agent.nodes.patchNode("/dashboards/hb-simple", { name: "rollup-v2" });
// → PATCH /api/v1/node?path=/dashboards/hb-simple  body: { name: "rollup-v2" }
// → GraphStore::patch_node → rename_node → NodeRenamed events
// Tags come along for free (same NodeId); presentation store re-keys via the event.
```

The four primitives (`create`, `delete`, `write-slot`, `patch-structure`) are all this feature set ever calls.

## Auth, audit, and limits

- `config.tags` supports dedicated capability `tags:write` (in addition to broader config permissions) so ops can tag without full config mutation rights.
- Tag updates are audited as slot writes (`who/when/before/after`).
- Presentation updates are runtime events (not user config changes) and are included as operational audit events on transition only (dedup repeated same-state emits within 5s default window).
- Limits:
  - max labels per entity: 64
  - max kv entries per entity: 64
  - key length: 64
  - value length: 128

Migration:

- Existing entities are treated as `tags = { labels: [], kv: {} }` lazily (no mandatory backfill migration).
- Optional background backfill may materialize normalized empty tags for index warmup, but is not required for correctness.

## Rollout stages

| Stage | Deliverable |
|---|---|
| 1 | `config.tags` schema + parser/normalizer for shorthand syntax |
| 2 | Backend translator/index support (SQLite + Postgres) behind feature flag |
| 3 | QuerySchema exposure for `tags.labels` + `tags.kv.*` pattern fields (enabled only when stage 2 is on) |
| 4 | `NodePresentationUpdate` envelope + frontend `PresentationStore` |
| 5 | Flow canvas visuals (status dot, icon, header accent, tooltip) |

## Decisions

1. Keep tags and presentation in separate concerns:
   - tags = user config and discoverability
   - presentation = runtime operational state
2. Keep presentation in a dedicated frontend store to avoid re-rendering whole nodes on value ticks.
3. Extend generic query with pattern fields once, instead of per-resource custom filters.

## One-line summary

**Adopt `[labels]` + `{key:value}` tags as canonical `config.tags`, add runtime `NodePresentationUpdate` for status/icon/color/message, and make both filterable through the existing generic query pipeline via `tags.labels`, `tags.kv.*`, and `presentation.status` without per-resource query code.**
