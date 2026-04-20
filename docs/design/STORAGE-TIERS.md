# Storage Tiers

A framework for deciding *where* a piece of state lives in this codebase. Read this before you add a migration, a new table, or a new module that persists anything.

## The mistake we're heading off

"Everything is a node" is the load-bearing idea behind [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) and it's right for user-facing things. But the maximalist read — *everything* goes into `nodes` + `slots` — has two failure modes:

1. **High-volume append-only records shoved into slot history.** A thousand chat messages per session becomes a thousand nodes, or one fat `Json` slot that gets re-written on every append, blowing past the 64 KiB COV cap from [SLOT-STORAGE.md:74](SLOT-STORAGE.md#L74) and storing each append's full prior state in `slot_history`. O(N²) storage for something that's fundamentally an event log.
2. **Service scratch space forced into the graph.** Runtime state a plugin wants to remember between restarts, a rate-limit counter, a provider-session cache: none of these are user-facing, none are linkable, none have ACL. They don't need tree placement, revisions, or subscription events. Forcing them through the graph adds coordination cost for zero benefit.

The fix is three tiers. Pick the right one up front; the cost of getting this wrong compounds.

## The three tiers

| Tier | Storage | Use when | Not for |
|---|---|---|---|
| **1. Graph nodes + slots** | `nodes`, `links`, live `slots` (+ opt-in `slot_history` / `slot_timeseries` via [`sys.core.history.config`](SLOT-STORAGE.md)) | User-facing; has lifecycle/ACL; appears in the tree; linkable; manipulable through the UI or CLI | Append-only streams; service scratch; ephemeral caches |
| **2. Typed companion tables** | One dedicated table per bounded context. Repo trait in [`data-repos`](../../crates/data-repos/) with impls in `data-sqlite` + `data-postgres`. FK to `node_id` when relevant. | Append-only / high-volume / queried by non-trivial SQL / never manipulated as individual things by users | Things that need ACL at row level; things users want to see in the tree |
| **3. Document store** | Single `documents` table: `(namespace, key, body JSONB, version, updated_ms, ttl_ms)`. One repo trait: `DocumentStore`. | Ephemeral state; denormalized runtime caches; plugin scratch; stuff where schema overhead isn't worth it | Anything you'll ever want to SQL-query inside the body; anything user-facing |

Already present in the codebase: tier 1 (the graph), tier 2 examples (`slot_history`, `slot_timeseries`, `flow_revisions`, `preferences`). Tier 3 is new.

## The decision tree

```
Is it user-facing / appears in the tree / has ACL / is individually linkable?
  → TIER 1  (node + slots).

Is it append-only OR high-volume per parent OR queried by real SQL?
  → TIER 2  (dedicated table; new repo trait in data-repos).

Is it ephemeral / service scratch / cache / denormalized runtime state
a single crate owns end-to-end?
  → TIER 3  (document store; pick a namespace).

None of the above?
  → You haven't understood the data yet. Don't add storage until you do.
```

Answer the question in that order. Don't jump to tier 3 because it's the easiest — tier 3 is the **last** tier, not the default.

## Tier 1 — graph nodes (unchanged)

Everything in [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) and [SLOT-STORAGE.md](SLOT-STORAGE.md) applies. This doc doesn't change how nodes work; it clarifies what *isn't* a node.

## Tier 2 — typed companion tables

Pattern already used by every existing non-node persistence in the codebase. Rules:

1. **One crate owns the bounded context.** `slot_history` belongs to `domain-history`. `flow_revisions` belongs to `domain-flows`. A new tier-2 table comes with a new (or extended) repo trait in `data-repos`, two backend impls (`data-sqlite`, `data-postgres`), and exactly one domain crate consumer.
2. **FK to `node_id` when the row hangs off a node.** Messages hang off a session node; receipts hang off a workflow node; revisions hang off a flow node. The FK keeps cascading delete coherent.
3. **Migrations are per-backend.** Same logical shape on SQLite and Postgres, physical DDL is different (SQLite `INTEGER PRIMARY KEY`, Postgres `BIGSERIAL` + partitioning + RLS per [CODE-LAYOUT.md:196](CODE-LAYOUT.md#L196)).
4. **Not queryable via the graph's event bus or RSQL surface by default.** If authors need a query path, either expose it through a dedicated REST endpoint or plumb it into the time-series/history endpoints that already exist (see [Query-Lang integration](#integration-with-query-lang) below).

Examples that clearly belong here:

| Data | Table | Why tier 2 |
|---|---|---|
| AI chat messages | `ai_messages(session_node_id FK, ts_ms, role, content_json, tokens_in, tokens_out)` | High-volume per session; immutable after create; always queried by session + ts range. See [AI.md](AI.md). |
| Audit log | `audit_events(tenant, ts_ms, actor, action, target, payload_json)` | Append-only; compliance-grade; never user-mutable. |
| Fleet rollout receipts | `fleet_receipts(rollout_node_id FK, device_node_id FK, ts_ms, outcome, error)` | One row per target; queried by rollout + outcome. |

## Tier 3 — document store

One table, one trait, no schema per namespace, no cross-namespace queries.

### Schema

```sql
CREATE TABLE documents (
  namespace   TEXT    NOT NULL,   -- "ai.provider_cache", "block.runtime", ...
  key         TEXT    NOT NULL,   -- free-form inside the namespace
  body        TEXT    NOT NULL,   -- JSON text (SQLite) / JSONB (Postgres)
  version     INTEGER NOT NULL DEFAULT 0,
  updated_ms  INTEGER NOT NULL,
  ttl_ms      INTEGER,            -- NULL = no expiry
  PRIMARY KEY (namespace, key)
);
CREATE INDEX idx_documents_ns_ttl ON documents(namespace, ttl_ms)
  WHERE ttl_ms IS NOT NULL;
```

### Trait

Lives in `data-repos`. No joins exposed. No content-query API. CAS for concurrent writers.

```rust
pub trait DocumentStore: Send + Sync {
    fn get(&self, ns: &str, key: &str) -> Result<Option<Document>, RepoError>;
    fn put(&self, ns: &str, key: &str, body: &JsonValue, ttl_ms: Option<i64>)
        -> Result<Document, RepoError>;
    fn cas(&self, ns: &str, key: &str, expected_version: i64, body: &JsonValue)
        -> Result<Document, RepoError>;
    fn delete(&self, ns: &str, key: &str) -> Result<bool, RepoError>;
    fn list(&self, ns: &str, prefix: Option<&str>, limit: u32)
        -> Result<Vec<Document>, RepoError>;
    fn sweep_expired(&self, now_ms: i64) -> Result<u64, RepoError>;
}
```

### Three rules, enforced in code review

1. **Namespaces are owned by exactly one crate.** `ai.session_cache` belongs to `domain-ai`. `block.runtime.<plugin_id>` belongs to `domain-blocks`. No cross-namespace reads. If two domains need the same data, it's tier 2, not tier 3.
2. **No content queries.** Never `WHERE body->>'status' = 'active'`. The moment you need it, the thing belongs in tier 2. Lookups are by `(namespace, key)` or `(namespace, key_prefix)` only.
3. **No UI directly on tier 3.** Studio reads graph nodes and dedicated REST projections over tier 2. Tier 3 is service state, invisible to the outside. When users want to see it, promote it.

Rule 3 is the anti-junk-drawer guardrail. Without it, you wake up in two years with 400 namespaces, no schema, and queries like `body->>'$.really.nested.thing' = ?` scattered through the codebase — i.e. you have built a bad NoSQL database instead of a clean service-state store.

## Integration with [QUERY-LANG.md](QUERY-LANG.md)

The generic RSQL + pagination pipeline in [QUERY-LANG.md](QUERY-LANG.md) is explicitly for tier 1 (graph nodes — `/api/v1/nodes`) and time-series / structured-history (tier 2 specifically via the separate time-series contract at [QUERY-LANG.md § Time-series query shape](QUERY-LANG.md)). This doc extends the picture without breaking those rules:

| Tier | Query surface |
|---|---|
| **1** | RSQL filter/sort/pagination per resource (`GET /api/v1/nodes?filter=...`). Generic framework handles it. |
| **2 (time-series)** | Four-primitive contract: `from`, `to`, `bucket`, `agg` (+ `kind` fan-out). Separate endpoint family, not RSQL. |
| **2 (other append-only)** | Dedicated typed endpoints per concern (`GET /api/v1/audit?actor=...&from=...`). Uses the same `QuerySchema` machinery but exposes a resource-specific schema. |
| **3** | **Not queryable.** `get`, `put`, `list-by-prefix` only. If you need anything more, it's tier 2. |

The rule-of-thumb: **if a query goes through the RSQL grammar, the data it reads is tier 1 or tier 2. Tier 3 is invisible to that grammar.** Enforces the layering — RSQL can't accidentally start peering into service scratch space.

## Integration with [BLOCKS.md](BLOCKS.md) / plugins

Blocks already exercise all three tiers correctly; this doc codifies what's implicit today.

### What blocks get per tier

| Tier | Blocks interact with via | Examples |
|---|---|---|
| **1** | `BlocksSdk::graph().*` — `create_node`, `write_slot`, subscribe to events | Block state node (`sys.agent.block`), user-visible node kinds the block contributes |
| **2** | **Not directly.** Blocks that need persistent typed history write to a slot and let the configured `sys.core.history.config` historize it. They never see `slot_history` rows. | — |
| **3** | `BlocksSdk::storage()` — scoped `DocumentStore` | Last-run state, provider-session cache, per-tenant runtime counters |

### The block scratch contract

Per-block tier-3 access is **scoped** — the SDK hands each block a `DocumentStore` pre-bound to its namespace:

```rust
// Inside a block:
let store = ctx.storage();              // already namespace-scoped
store.put("last_run", &state, Some(ttl))?;
let prev = store.get("last_run")?;
```

Under the hood that resolves to namespace `block.runtime.<plugin_id>`. The block cannot name another plugin's namespace — the host owns the prefix.

### Quotas (non-negotiable for plugins)

Tier 3 for blocks **requires** both caps — a misbehaving block must not wedge the database. Enforced in `DocumentStore` impl, not in the SDK:

| Cap | Default | Override |
|---|---|---|
| `max_rows_per_ns` | 10_000 | Per-block in `block.yaml` under `capabilities.storage.max_rows`, host-approved on enable |
| `max_bytes_per_ns` | 50 MiB | Same mechanism; edge profiles default lower |
| `default_ttl_ms` | 7 days if `ttl_ms` omitted on `put` | Opt-out per write |

When a block hits its cap, `put` returns `RepoError::QuotaExceeded`; it's the block's problem to handle. No cascading failure into the host.

### What blocks still can't do

- Create their own tables (no tier-2 without a host-side domain crate).
- Read another plugin's namespace (host-scoped).
- Query inside `body` (same rule as everyone else — tier-3 is key-value).
- Persist node-shaped data outside the graph (use `graph.create_node` for that).

## Applied examples

| Feature | Tier 1 | Tier 2 | Tier 3 |
|---|---|---|---|
| AI chat session | Session node (title, provider, ACL) | `ai_messages` table | Provider-session cache, token-budget counters |
| Block instance | `sys.agent.block` node (lifecycle, config) | — | `block.runtime.<id>` scratch |
| Flow | `sys.core.flow` node | `flow_revisions` table | — |
| Device reading | Point node (live value) | `slot_timeseries` (via historization) | — |
| Audit trail | — | `audit_events` table | — |
| Rate limit counter | — | — | `rate_limit.<tenant>` namespace |
| Plugin UI remote-entry URL cache | — | — | `block.runtime.<id>.ui_cache` namespace |

## Promotion and demotion

When you get it wrong (you will, eventually):

- **Tier 3 → Tier 2**: the moment you write `WHERE body->>'X' = ...` or need a FK relationship, stop. Design a real table. Migrate by reading the tier-3 rows, writing to the new table, deleting the namespace. Usually a one-shot script in `tools/`.
- **Tier 2 → Tier 1**: you realize users actually want to see this thing individually. Model as a node kind, migrate rows to child nodes. Rare — most tier 2 is legitimately non-node.
- **Tier 1 → Tier 2**: explosive node count making the graph unusable. Move children into a typed table keyed by the parent node id. Rare; only hits on extreme fan-out.

Plan for migrations, don't design against them.

## What this doc does not try to do

- Replace the graph. Tier 1 is still the default for user data.
- Invent a new storage backend. All three tiers live in the same SQLite / Postgres deployment per [CODE-LAYOUT.md:56-67](CODE-LAYOUT.md#L56-L67). No Redis, no Mongo, no separate KV.
- Let plugins hand-write SQL. The `DocumentStore` trait is the only plugin-facing persistence surface; tier-1 access flows through the graph SDK.
- Make tier 3 queryable over RSQL. That line exists on purpose. If tier 3 grows a query surface, it stops being tier 3.

## One-line summary

**Three storage tiers — graph nodes for user-facing things, typed tables for append-only / high-volume domain data, a single document store for service scratch — with one crate owning each bounded context, one query grammar per tier, and explicit promotion paths when you pick wrong.**
