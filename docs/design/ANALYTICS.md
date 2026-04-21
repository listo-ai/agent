# Analytics Engine — Scope

Status: draft (revised after peer review)
Owners: platform/analytics
Depends on: [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md), [SLOT-STORAGE.md](SLOT-STORAGE.md), [QUERY-LANG.md](QUERY-LANG.md), [OVERVIEW.md](OVERVIEW.md), [RUNTIME.md](RUNTIME.md)
Related workflow: [ADDING-NEW-SERVICE-WORKFLOW.md](../../../ADDING-NEW-SERVICE-WORKFLOW.md)

## Purpose

Deliver a SkySpark-class analytics capability — scheduled and event-driven rules over historized slot data, producing derived points, faults, and KPIs — as a **pure compute sidecar**. Triggering and result routing are done by the existing flow engine; analytics-engine is a stateless function: `(rule_id, context) → intents`.

## Target persona

**The target rule author is an integrator or controls engineer comfortable with SQL and light scripting** — someone who'd use a spreadsheet, a SQL console, and a Python notebook interchangeably. The authoring surface (YAML header + DataFusion SQL + Rhai) is *one layer too deep* for a pure controls engineer who's only touched graphical rule builders; it's *one layer too shallow* for a pure software engineer who'd reach for Python + pandas.

Implication: **the first-party rule library (Stage 11) is not optional polish.** Operators write ~20% of their rules from scratch and copy-paste-edit the other 80%. Ten worked examples is a floor, not a ceiling — plan to grow it with each customer deployment.

## Problem

Operators want three things and none of them should be shoehorned into existing surfaces:

1. **Derived points** — `ahu.efficiency = cooling_output / electrical_input`, recomputed periodically, historized.
2. **Fault rules** — "zone_temp − setpoint > 2°C sustained 15 min during occupied hours."
3. **Rollups / KPIs** — daily kWh per meter, weekday/weekend split, 5-year retention.

Non-solutions that each fail for a specific reason:

- **Hand-coded blocks per rule.** 500 inconsistent implementations, no common RBAC, no common retention, no common authoring.
- **Rules-as-flows (logic drawn on a canvas).** Visual graphs do not scale past a few dozen rules. You cannot diff, grep, refactor, or code-review a canvas. SkySpark is text rules for a reason.
- **In-DB views / continuous aggregates only.** Timescale-only, cloud-only, no imperative logic, no sandbox. Edge and standalone are left out.
- **A fully self-contained analytics service with its own scheduler, its own output sinks, its own event bus integration.** Duplicates the flow engine.

The honest answer is a **dedicated compute sidecar** that owns the rule/dataset model and the DataFusion+Rhai engine, and **delegates everything orchestration-flavoured to flows**.

## Goals

1. **Three concepts.** *Dataset*, *Rule*, and the derived *Schedule view*. Everything else (runs, intents, side effects) is either audit or reuses existing graph surfaces.
2. **Text-first authoring.** A Rule is a Rhai script with a typed header declaring input Datasets and declared output intents. No canvas. Diffable, greppable, code-reviewable, unit-testable.
3. **Off-the-shelf engine.** DataFusion for columnar query, Arrow as zero-copy handoff, Rhai for imperative logic, sandboxed. Zero custom languages, zero custom parsers.
4. **Direct TSDB read.** Analytics-engine reads Timescale/SQLite through its own read-only pool. DataFusion push-down (time range, kind filter, bucket) happens in the database.
5. **Vectorised heavy lifting.** Hot-path helpers (`sustained`, `occupied_hours`, `last_change`, `baseline_deviation`, ...) are implemented in Rust as DataFusion UDF/UDAF/UDWF over Arrow columns. Rhai sees only pre-aggregated scalar or short-vector results. Rhai never iterates rows on the hot path.
6. **Pure function — return intents, never apply them.** A rule run produces a structured `IntentList`. The flow engine applies intents through existing `slot.write` / `alarm.raise` blocks. Single authorization + audit path.
7. **Flow engine owns scheduling and orchestration.** Two new flow blocks: `analytics.run_rule` and `analytics.apply_intents`. Cron, event triggers, leader election, retry, audit, downstream chaining — all reuse flows.
8. **Zenoh for service-to-service.** The `run_rule` RPC rides Zenoh queryables. Payload is **CBOR** (self-describing; version-tolerant across rolling upgrades). HTTP exists only for outside-world CRUD (Studio, CLI).
9. **Admission control.** A concurrency semaphore in the queryable caps in-flight rule runs. Edge defaults to `max_concurrent_rules = 1`; cloud is configurable per pod. Rejected calls return `AnalyticsBusy` with `retry_after_ms`; flow engine retries with jitter.
10. **Optional sidecar, coordinated release.** Analytics-engine is optional per deployment, but when deployed it shares the agent's DB and its migrations are coordinated with agent upgrades. **Not independently versioned** against the agent; don't claim otherwise.
11. **Edge / cloud parity, one binary.** SQLite backend on edge/tests, Postgres+Timescale on cloud. Feature-flagged out on legacy ARM (armv7, 256 MB).
12. **Clean reuse of platform primitives.** REST follows [QUERY-LANG.md](QUERY-LANG.md); logging/tracing/errors follow [OVERVIEW.md](OVERVIEW.md). No new patterns.

## Non-goals

- **Dashboards / charts.** Studio owns presentation. This scope is compute only.
- **Custom tag query grammar (Axon-style).** Node selection stays RSQL. DataFusion SQL is a *rule-author tool*, not a user-facing query surface.
- **Schedule as a separately-writable entity.** See "Schedule is a projection" below — the flow is the source of truth.
- **Parallel scheduler, parallel event bus, parallel output sinks.** Flow engine does all three.
- **Parallel authorization system.** Analytics-engine returns intents; the agent authorizes each intent at apply time.
- **Applying side effects directly from analytics-engine.** Ever. If analytics-engine wrote slots, audit and rate limiting would fragment.
- **Independent DB or independent release cadence from the agent.** Tried mentally, rejected — would re-invent tenant scoping and RBAC.
- **ML / forecasting.** v1 is aggregation + imperative rules.
- **Retroactive backfill.** v2; requires idempotent past-dated slot writes.
- **Streaming analytics (per-sample evaluation).** Windowed scheduled / event-triggered runs only.
- **Bundled into the agent binary.** Separate process for resource isolation and optional-deployment reasons.

## Key decisions

| Decision | Choice | Why |
|---|---|---|
| Repo shape | New top-level repo `listo-ai/analytics-engine`, onboarded per ADDING-NEW-SERVICE-WORKFLOW.md | Independent deploy, separate optional compile |
| Release coupling | **Coordinated with agent migrations; not independently versioned** | Shared DB makes migration ordering a coordination problem; own it honestly |
| Domain model | `Dataset` + `Rule` (+ projected `Schedule` view) + `rule_run` audit | Smallest surface; no dual-ownership |
| Authoring surface | Text (Rhai) + DataFusion SQL + YAML header | Diffable, testable, scales to thousands |
| Columnar engine | [DataFusion](https://datafusion.apache.org/) | Mature Rust, Arrow-native, push-down, parallel execution |
| In-memory model | [Apache Arrow](https://arrow.apache.org/) | Zero-copy between DataFusion and Rhai bindings |
| Imperative language | [Rhai](https://rhai.rs/) | Embeddable, sandboxable, no GC, no runtime deps |
| Hot-path helpers | **Rust UDF/UDAF/UDWF over Arrow** — not Rhai loops | Preserves DataFusion vectorisation on predicate evaluation |
| Read path | Direct TSDB via DataFusion `TableProvider` | Push-down impossible through HTTP |
| Write path (apply) | **Flow engine.** Rules return intents; `apply_intents` block expands them | Single authorization + audit |
| Scheduling + triggers + leader election | **Flow engine** | Don't build a second one |
| Service-to-service RPC | **Zenoh queryables, CBOR payload** | Already-operational messaging; self-describing wire format tolerates rolling upgrades |
| External RPC | HTTP (axum) | Studio, CLI |
| OLTP storage | Same DB as agent, schema `analytics` | One backup, one pool |
| Result storage | Nothing. Intents flow through the graph. | "Everything as a node" holds |
| Audit storage | `analytics.rule_run` | Only analytics-specific persisted state |
| Query framework | RSQL + `#[derive(Queryable)]` | Zero custom controllers |
| Rhai sandbox | ops cap, memory cap, call-depth cap, fs/net disabled | Same floor cloud and edge |
| Row-count cap | 500k post-SQL default, per-rule override | Predictable memory |
| Concurrency cap | Edge: 1 concurrent run. Cloud: per-pod configurable (default 4). | Prevents OOM on edge; `AnalyticsBusy` surfaces backpressure |
| DataFusion SQL dialect | DataFusion's own, documented explicitly | Avoids Postgres-vs-SQLite confusion |
| Legacy ARM (armv7 256 MB) | Not supported | DataFusion doesn't fit |

## Note on messaging (pre-empting reviewer confusion)

The platform's operational messaging backbone is **Zenoh**. NATS is listed in [OVERVIEW.md § Key libraries](OVERVIEW.md#key-libraries) as *(planned)* and is **not in the deployed stack**. There is no NATS→Zenoh hop in a rule trigger — the full path (slot-change event → flow engine → `analytics.run_rule` → analytics-engine → reply) is Zenoh end-to-end.

## Domain model

Two persisted resources + one derived view + one audit table in the `analytics` schema.

### Dataset

A named, versioned selection of history. Compiles to a DataFusion view.

```yaml
# dataset: zone_temps_hourly
input:
  kind: sys.device.zone_sensor     # or: path_glob: "/bldg/*/zone/*"
  slot: temp
window:
  relative: 24h                    # or: absolute: { from, to }
bucket: 1h
agg: avg
```

Datasets can reference other Datasets (DAG, not just tree). Depth capped at 4 levels; cycle detection is a compile-time error (`DatasetCycle`), checked at Dataset create/update time against the full closure, not just immediate dependencies.

### Rule

Text-first. Header declares input Datasets and declared output shapes. Body has an optional DataFusion SQL stage followed by a Rhai stage.

```yaml
# rule: zone_fault_sustained
inputs:
  temps:     { dataset: zone_temps_hourly }
  setpoints: { dataset: zone_setpoints_hourly }
outputs:
  - intent: slot_write
    path_expr: "row.path"
    slot: fault
  - intent: alarm
    severity_expr: "severity"
resources:
  max_rows: 500_000
  max_rhai_ops: 10_000_000
  timeout_ms: 30_000

sql: |
  -- UDWF does the heavy lifting; Rhai sees scalars per row
  SELECT t.path,
         sustained(t.value - s.value > 2.0, '15m') AS fault,
         occupied_hours(t.path, t.ts_bucket) AS is_occupied
  FROM temps t
  JOIN setpoints s ON t.path = s.path AND t.ts_bucket = s.ts_bucket

rhai: |
  // Rhai only decides on aggregated scalars — not row loops
  let active = rows.filter(|r| r.fault && r.is_occupied);
  let severity = if active.len() > 3 { "warning" } else { "none" };
  return { fault: active, severity };
```

### Schedule (derived view, not a persisted entity)

**Schedule is a read-only projection over flows that reference a given `rule_id`.** It is not stored in `analytics.schedule`; it is computed on demand by querying the flow engine for `analytics.run_rule` blocks whose config references the rule.

```
GET /api/v1/analytics/rules/{id}/schedules
  → queries flow engine for flows containing analytics.run_rule(rule_id={id})
  → returns [{ flow_id, trigger, jitter_ms, enabled, last_run_at }, ...]
```

**Creating a schedule = creating a flow.** Studio's "add schedule" action is a flow-upsert against the agent's API; analytics-engine is not in the write path. This eliminates dual-ownership by construction. If the user edits the flow directly, the projected Schedule reflects it on next read — no sync, no drift flag, no reconciliation.

Consequence: analytics-engine does not store cron strings. The rule YAML does not declare a default cron; that's a flow concern.

### Rule run (audit)

Written per Zenoh invocation. Not a user resource; exposed read-only via RSQL for debugging.

```
rule_id, started_at, ended_at, status, rows_in, intents_out,
trigger_kind, error_code, error_message, duration_ms,
trigger_context_hash, dry_run (bool)
```

## Dry-run mode

Every rule run accepts `dry_run: bool` (default `false`). When `true`:

- Engine executes normally; `IntentList` is computed and returned.
- `rule_run` row is written with `dry_run=true`.
- Caller (flow engine, Studio "Test" button, CLI) receives the intents but **does not** pass them to `apply_intents`.

This is the rule-author feedback loop. Stage 3 deliverable.

## The two new flow blocks

Two blocks, full stop.

| Block | Inputs | Output | What it does |
|---|---|---|---|
| `analytics.run_rule` | `rule_id`, `trigger_context`, `dry_run?` | `IntentList` | Zenoh `get` to `listo/<tenant>/analytics/rule/run`; CBOR payload; typed errors (`PlanTooLarge`, `RhaiSandboxViolation`, `Timeout`, `AnalyticsBusy`, `AnalyticsUnavailable`) surface as flow-node errors. `AnalyticsBusy` triggers flow-level retry with jitter. |
| `analytics.apply_intents` | `intents: IntentList`, `dry_run?` | — | Expands each intent into `slot.write` / `alarm.raise` / `event.emit`. Per-intent error isolation. If `dry_run=true`, logs intents but does not apply. |

## Intent wire format

Typed, serde-serializable, versioned. **Encoded as CBOR over Zenoh** (self-describing, version-tolerant across rolling upgrades).

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Intent {
    SlotWrite  { path: String, slot: String, value: SlotValue },
    Alarm      { severity: Severity, message: String, source: String, dedup_key: Option<String> },
    EventEmit  { topic: String, payload: serde_cbor::Value },
}

pub struct IntentList {
    pub version: u32,
    pub rule_id: Ulid,
    pub run_id: Ulid,
    pub intents: Vec<Intent>,
}
```

New intent variants are additive; use `#[serde(other)]` on the receiver side to tolerate unknown variants from a newer sender during a rolling upgrade.

## Time-alignment semantics

Rules commonly join Datasets by `(path, ts_bucket)` across devices that don't sample in lockstep. The alignment rules:

1. **Bucket boundaries are wall-clock-aligned** (multiples of `bucket_ms` from Unix epoch). A `1h` bucket is always `00:00, 01:00, 02:00, …` UTC — never relative to the query's `from`. Matches the leaning answer in [QUERY-LANG.md open questions](QUERY-LANG.md#open-question). Cross-rule and cross-chart comparisons are trivially correct.
2. **Missing samples in a bucket for one series produce `NULL`** in that bucket's aggregate. Join semantics follow SQL: `JOIN` drops rows where either side is NULL; `LEFT JOIN` preserves the left side. Rule authors pick.
3. **Clock skew >$bucket_ms$** means a sample can land in an earlier or later bucket than "expected." The doc does not attempt to correct for this; SLOT-STORAGE.md records NTP sync state per sample, which is available as a column if a rule needs to filter suspect samples (`WHERE ntp_synced AND last_sync_age_ms < 60_000`).
4. **Partial buckets at range boundaries** are flagged in the Arrow output via `edge_partial_start` / `edge_partial_end` columns when the query window doesn't align cleanly with bucket boundaries. Consumers decide whether to drop, keep, or flag them in results.

Cross-device rules (e.g., `zone_temp` and `setpoint` from different controllers) join on `(path_normalised, ts_bucket)` where `path_normalised` is a rule-author-supplied expression — typically the parent AHU path or a shared zone tag. No automatic "same controller" coupling.

## Concurrency and admission control

The Zenoh queryable is backed by a tokio semaphore:

| Deployment | `max_concurrent_rules` default | Rationale |
|---|---|---|
| Edge (ARM/x86 ≥ 512 MB) | **1** — serial execution | 500k rows × 40 bytes × concurrent rules would blow the 150 MB budget; serial is safe |
| Standalone | 1 | Same reasoning |
| Cloud pod (2–4 GB) | 4 | Conservative; tune per workload |
| Cloud pod (8 GB) | 8 | |

Behaviour when the semaphore is full:

- Zenoh reply is `AnalyticsBusy { retry_after_ms }`.
- `analytics.run_rule` block surfaces this as a retryable error.
- Flow engine's retry policy (existing) re-queues with jitter.
- No in-engine queue. No in-engine state. Backpressure is the flow engine's job.

## Stack

| Concern | Crate | Notes |
|---|---|---|
| Async runtime | `tokio` | Platform standard |
| HTTP server (external CRUD) | `axum` + `tower-http` | Platform standard |
| Service-to-service RPC | **`zenoh`** | Queryable on `listo/<tenant>/analytics/rule/run` |
| Wire codec | **`ciborium`** (CBOR) | Self-describing, version-tolerant; no bincode on the wire |
| Serialization | `serde` + `serde_json` + `serde_yml` | JSON/YAML for persistence + HTTP |
| JSON Schema | `schemars` | Auto-docs for rule/dataset headers |
| Error handling | `thiserror` in libs, `anyhow` in the binary | Platform standard |
| Logging / tracing | `tracing` + `tracing-subscriber` | Platform standard |
| Columnar engine | [`datafusion`](https://crates.io/crates/datafusion) | Query, planning, push-down |
| Columnar memory | [`arrow`](https://crates.io/crates/arrow) | RecordBatch / Array handoff |
| Rule scripting | [`rhai`](https://crates.io/crates/rhai) | Sandboxed scripting |
| TSDB providers | [`datafusion-table-providers`](https://crates.io/crates/datafusion-table-providers) | SQLite + Postgres `TableProvider` |
| SQLite | `rusqlite` (bundled) | Platform standard |
| Postgres | `sqlx` | Platform standard |
| Query framework | workspace `query` crate | RSQL + Queryable derive |
| Datetime | `jiff` | Platform standard |
| Testing | `tempfile` + `testcontainers` | SQLite in-process is default |

**Not in the stack:** scheduler crate, NATS, gRPC, Arrow Flight, output-sink crate, bincode on the wire.

## Architecture

```
  ┌───────────────────────────────────────────┐
  │ Agent (flow engine)                       │
  │                                           │
  │   [cron / slot.changed / manual trigger]  │
  │               │                           │
  │               ▼                           │
  │     [analytics.run_rule block] ──zenoh──┐ │
  │         (dry_run? flag)                 │ │
  │               │                         │ │
  │               ▼                         │ │
  │     [analytics.apply_intents block]     │ │
  │       ├─▶ [slot.write]                  │ │
  │       └─▶ [alarm.raise]                 │ │
  └─────────────────────────────────────────│─┘
                                            │
                                   Zenoh queryable
                          `listo/<tenant>/analytics/rule/run`
                                   CBOR payloads
                                            │
  ┌─────────────────────────────────────────▼─┐
  │ analytics-engine  (sidecar; 1 or N)       │
  │   ┌─── admission semaphore ───┐           │
  │   │   (edge: 1, cloud: N)     │           │
  │   │ AnalyticsBusy on overflow │           │
  │   └───────────┬───────────────┘           │
  │               │                           │
  │   HTTP (axum)          Zenoh queryable    │
  │   • CRUD datasets      • run_rule         │
  │   • CRUD rules           (dry_run aware)  │
  │   • list schedules                        │
  │     (projection over flows)               │
  │   • list runs        ┌──────────────┐     │
  │         │            │  Rule Engine │     │
  │         └──────┬─────┴──────┬───────┘     │
  │                │            │             │
  │       ┌────────┴────┬───────┴────────┐    │
  │       ▼             ▼                ▼    │
  │  ┌─────────┐  ┌───────────┐  ┌──────────┐ │
  │  │ Dataset │  │DataFusion │  │   Rhai   │ │
  │  │compiler │─▶│ + Arrow   │─▶│ sandbox  │ │
  │  │(DAG     │  │+ UDF/UDAF │  │(scalars  │ │
  │  │ check)  │  │+ UDWF     │  │ only)    │ │
  │  └────┬────┘  └─────┬─────┘  └────┬─────┘ │
  │       │             │             │       │
  │       ▼             ▼             ▼       │
  │  OLTP schema   TSDB (read-only)  IntentList│
  │  analytics.*   via TableProvider (CBOR    │
  │                (push-down)       reply)   │
  └───────────────────────────────────────────┘
```

## Storage

- **OLTP tables** in the `analytics` schema, same DB as the agent:
  - `analytics.dataset` — id, tenant_id, name, spec, version, created/updated
  - `analytics.rule` — id, tenant_id, name, header, sql, rhai, version, enabled, resources
  - `analytics.rule_run` — id, rule_id, started_at, ended_at, status, rows_in, intents_out, trigger_kind, error_code, error_message, duration_ms, trigger_context_hash, dry_run
- **No `analytics.schedule` table.** Schedules are projected from flows.
- **Migrations** owned by this repo, applied on startup with `sqlx::migrate!`. **Migration ordering is coordinated with the agent** — the deployment runbook is explicit that agent migrations run first.

## Testing strategy

Explicit per-stage what "CI green" means. All tests are `tempfile`-SQLite by default; Timescale via `testcontainers` is gated behind `--features timescale-tests`.

| Test tier | What's covered | Where |
|---|---|---|
| **Dataset compiler unit tests** | Resolution, cycle detection, depth cap, window translation, SQL push-down shape | `analytics-core` |
| **Rule engine unit tests (fixture-based)** | Given a seeded SQLite + a rule YAML + expected IntentList, assert round-trip. No Zenoh. | `analytics-core` |
| **Helper UDF/UDAF/UDWF tests** | Each helper exercised against Arrow inputs with golden outputs | `analytics-core` |
| **Sandbox / bad-rule tests** | A "bad rule" suite: infinite loops, excessive memory, forbidden stdlib, type errors, SQL injection attempts, dataset cycles. Each must fail with the typed error claimed in the API. | `analytics-core` |
| **Zenoh RPC integration tests** | Spin up analytics-engine + a mock flow-engine block; assert end-to-end call semantics (success, timeout, busy, dry-run, CBOR version-tolerance) | `analytics-engine` integration tests |
| **Flow ↔ analytics integration** | Real flow engine with `analytics.run_rule` + `analytics.apply_intents` blocks, real analytics-engine, fixture database; assert slot writes land and audit is correct | workspace integration suite |
| **Push-down verification (Stage 2 gate)** | For each representative query shape, inspect the generated TSDB SQL and confirm predicate/bucket/agg are pushed down. Automated, not manual. | `analytics-core` |
| **Backend parity tests** | Full rule-engine suite runs against Timescale via testcontainers; diffs against SQLite results | feature-gated |
| **Soak test** | 24h, cloud + edge profiles | Stage 10 |

## Deployment profiles

| Profile | Analytics engine shape |
|---|---|
| Cloud — multi-tenant | Sidecar deployment, N pods, Postgres+Timescale, Zenoh-routed |
| Cloud — single-tenant | Single sidecar pod, same DB as agent |
| Edge — x86 / ARM64 ≥ 512 MB | Single process co-located with agent, SQLite, local Zenoh queryable, `max_concurrent_rules=1` |
| Edge — legacy ARM (armv7) | Not supported — compile-time out |
| Standalone appliance | Single process bundled with agent, SQLite |
| Developer laptop | Single process, SQLite, default for `cargo run` and tests |

## Surfaces

| Surface | Shape |
|---|---|
| **Zenoh queryable** | `listo/<tenant>/analytics/rule/run` — CBOR `{rule_id, trigger_context, dry_run}` → `IntentList` or typed error. Admission-controlled via semaphore. |
| **HTTP REST** | `/api/v1/analytics/datasets`, `/rules`, `/runs` — RSQL per QUERY-LANG. `/rules/{id}/schedules` (projected from flows). `/rules/{id}/test` (dry-run shortcut). |
| **Flow blocks** | `analytics.run_rule`, `analytics.apply_intents` |
| **CLI** | `listo-analytics rule list/create/edit/test/run`, `dataset ...` — all call HTTP |
| **Studio** | Analytics section: Monaco rule editor (Rhai + DataFusion SQL grammars), "Test" button using dry-run, run-history drill-down, schedule list (read from projected endpoint; "Add schedule" upserts a flow via agent API) |
| **OpenAPI** | Auto-generated via `utoipa` + `QuerySchema` |
| **Shared DB** | `analytics` schema; migrations owned here; ordering coordinated with agent |

## Global search — the two-endpoint split

Platform-wide search is **two endpoints, deliberately not one**. Trying to unify them forces live ECS state through a columnar engine and forces admission control onto every palette keystroke — wrong shape for 95% of lookups, wrong shape for the remaining 5%.

| Endpoint | Engine | Question it answers | Latency target |
|---|---|---|---|
| `/api/v1/search` | [RSQL](QUERY-LANG.md) fan-out over existing `Queryable` scopes | "Where is X?" / "What matches Y?" — lookup, navigation, palette-grade | p95 < 50 ms, synchronous |
| `/api/v1/analyze` | **This engine** — thin wrapper over `analytics.run_rule` with `dry_run=true` and an ephemeral Rule | "Compute / aggregate / correlate / sustained-condition over time" | p95 < 500 ms, sync on edge, 202 + job on cloud |

### `/search` — not analytics-engine's problem

Lives in the agent, fans out RSQL across every scope's existing `QuerySchema` (flows, nodes, slots, blocks, audit, kinds, …), merges top-N, fuzzy-ranks with `nucleo-matcher`. No columnar engine, no Zenoh round-trip, no admission semaphore. Auth comes free via `enforced_from = auth.tenant_id`. Time-series is explicitly excluded — same rule as [QUERY-LANG.md § "What 'generic' means here"](QUERY-LANG.md#what-generic-means-here).

Analytics-engine is not in this path.

#### Reusable scope contract

Every scope that wants to be searchable implements **one trait, in a domain crate** — not in `transport-rest`. The HTTP handler becomes a ~10-line dispatcher.

```rust
// crates/search-core/src/lib.rs  (new crate)
pub trait SearchScope: Send + Sync {
    /// Stable scope id — `"kinds"`, `"flows"`, `"nodes"`, `"slots"`, `"audit"`, …
    fn id(&self) -> &'static str;

    /// The RSQL schema this scope exposes. Same `QuerySchema` the framework
    /// already uses — no parallel definition.
    fn schema(&self) -> &QuerySchema;

    /// Run a validated query and return JSON-shaped hits. Scope owns its own
    /// materialisation (registry walk, SeaQuery, whatever it needs).
    async fn query(&self, q: &ValidatedQuery, ctx: &AuthContext)
        -> Result<Vec<SearchHit>, SearchError>;
}

pub struct SearchHit {
    pub scope: &'static str,
    pub id: String,
    pub display: String,     // ranked by nucleo-matcher against the user's text
    pub path: Option<String>,
    pub payload: serde_json::Value,  // the full DTO for the client
}
```

Where things live:

| Crate | Owns |
|---|---|
| `query` (exists) | RSQL parser, `QuerySchema`, validator, JSON-walking executor |
| `search-core` (new, ~300 LOC) | `SearchScope` trait, `SearchHit`, merge + rank loop, palette DSL parser |
| `domain-kinds` (new or folded into `graph`) | `KindsScope: SearchScope`, `KindDto`, `derive_org`, `kinds_query_schema`, `placement_allowed` |
| `domain-flows`, `domain-audit`, … | Their own `Scope` impls — each is the source of truth for its query surface |
| `transport-rest` | Thin `/api/v1/search` handler that builds `AuthContext`, calls `search_core::run(scopes, query, auth)`, returns JSON. **No domain logic.** |
| `transport-cli` | Same pattern — `listo find …` calls `search_core::run` directly, no HTTP hop |
| `ai-runner` / MCP | MCP tool `search` also calls `search_core::run` — not a separate HTTP client |

The reuse test: if adding a new transport (e.g. Zenoh queryable for fleet-wide search) requires *any* change in `domain-*` crates, the layering is wrong.

#### Migration: hard-delete `/api/v1/kinds`

[`transport-rest/src/kinds.rs`](../../crates/transport-rest/src/kinds.rs) is the reference implementation of the anti-pattern this design fixes — domain logic (`KindDto`, `derive_org`, `placement_allowed`, `kinds_query_schema`) sits inside the transport crate. It ships today; it gets **hard-deleted** when `/search` lands. Not deprecated, not aliased, not redirected. Hard delete.

**Move, don't duplicate.** Every item in `kinds.rs` has exactly one destination:

| Current location (`transport-rest/src/kinds.rs`) | New location | Notes |
|---|---|---|
| `KindDto`, `from_manifest` | `domain-kinds` (or `graph::kinds::dto`) | Also consumed by CLI and MCP — must not live in a transport crate |
| `derive_org` | Same, as `pub(crate)` helper of `KindDto` | Keep unit tests with it |
| `kinds_query_schema()` | `domain-kinds`, exposed via `KindsScope::schema()` | One definition, many transports |
| `placement_allowed` | **Already belongs in `graph`** — it mirrors `GraphStore::create_child`'s logic. Move it there and have both call sites share it. This is a pre-existing duplication the migration cleans up. |
| `ListKindsQuery` (facet / placeable_under / filter / sort) | Gone. The palette DSL subsumes all of these: `#kinds facet:isCompute placeable_under:/iot/broker filter:org==com.listo` → compiled into the validated query passed to `KindsScope`. |
| `list_kinds` handler | Gone. `/api/v1/search?scope=kinds&…` handles every call site. |
| Tests in `kinds.rs` | Move with the code. Any test that exercised the HTTP handler moves to `search-core` integration tests against a real `KindsScope`. |

**UI cleanup (same PR, not a follow-up):**

- Every `/api/v1/kinds` call in `studio`, `ui-core`, `block-ui-sdk`, `agent-client-ts`, `agent-client-rs`, `agent-client-dart` → migrated to `/api/v1/search?scope=kinds` with identical filter semantics. Palette in Studio uses the new DSL directly.
- Remove `getKinds(...)` / `listKinds(...)` helpers from every client library. Replace with `search({ scope: "kinds", ... })`. No backwards-compat shim — the generated TS types will fail to compile if any caller missed the migration, which is the signal we want.
- Grep-gate: `git grep -n "api/v1/kinds\|list_kinds\|getKinds\|listKinds"` returns zero hits before merge. CI enforces it.

**Why hard-delete, not deprecate:** the endpoint is internal, every caller is in the workspace, the contracts repo controls the wire types. A deprecation window buys nothing and guarantees the old shape stays indexed in docs, tests, and muscle memory.

**Migration ordering (single PR, or two if the diff is unreadable):**

1. Land `search-core` crate + move `KindDto`/helpers to `domain-kinds`.
2. Add `/api/v1/search` handler; register `KindsScope` (and any other scopes that have a `QuerySchema` already).
3. Flip every UI/CLI/client caller in the same PR.
4. Delete `transport-rest/src/kinds.rs` and its route wiring.
5. CI grep-gate catches stragglers.

This is the template every other scope follows — when `flows`, `audit`, etc. migrate, the transport-side change is one line (register another scope); the domain-side change is "implement `SearchScope`." No more per-resource endpoints.

### `/analyze` — a thin façade over the rule runner

Analytics-engine exposes an HTTP endpoint that accepts an ad-hoc rule body (inputs + optional SQL + optional Rhai), builds a transient `Rule` in-memory, and calls the existing Zenoh queryable with `dry_run=true`. Same CBOR wire, same admission semaphore, same UDFs, same sandbox, same typed errors (`AnalyticsBusy`, `PlanTooLarge`, `Timeout`, …).

```
POST /api/v1/analyze
{
  "inputs":   { "zone_temps_hourly": { dataset_ref | inline } },
  "sql":      "SELECT path, sustained(value - setpoint > 2, '15m') AS fault FROM ...",
  "rhai":     "rows.filter(|r| r.fault)",
  "row_cap":  10000,
  "timeout_ms": 5000
}
→ Arrow RecordBatch (application/vnd.apache.arrow.stream) or JSON
```

Behavioural notes:

- **Saved analyses are Rules.** "Save this query" upserts an `analytics.rule` and (optionally) a flow that schedules it — no new concept, no parallel storage.
- **No intents returned.** `/analyze` short-circuits `IntentList` assembly; the response is the raw Arrow result of the SQL/Rhai stage. Rule runs that *do* produce intents go through `analytics.run_rule` as before.
- **`rule_run` audit still written** with `dry_run=true` and `trigger_kind="adhoc_analyze"` — the same debugging surface works for ad-hoc queries.
- **Admission control is shared** with scheduled runs. On edge (`max_concurrent_rules=1`), an ad-hoc `/analyze` call competes with scheduled rules; backpressure surfaces as `AnalyticsBusy`. This is intentional — an interactive query should not starve a scheduled fault rule.

### Palette routing

A single palette (Studio + CLI + Rhai bindings) parses user input and auto-routes:

| Predicate shape | Routes to |
|---|---|
| Equality / like / in / time-range against scope fields | `/search` |
| Contains any aggregation, UDF, `OVER`, `GROUP BY`, or DataFusion SQL block | `/analyze` |
| User prefix `#find …` | `/search` forced |
| User prefix `#analyze …` or `#sql …` | `/analyze` forced |

Routing is syntactic — no heuristics, no round-trip guessing.

### Why this split is load-bearing

- **Lookup and analysis have different latency budgets by a factor of ~10×.** Unifying them forces every palette keystroke through DataFusion planning + Zenoh; unacceptable UX.
- **Lookup and analysis have different failure modes.** Search returns fewer results; analysis returns `AnalyticsBusy` and the caller retries with jitter. Conflating them forces the palette to grow retry logic.
- **Lookup spans every resource; analysis spans time-series + derived.** The QUERY-LANG framework already covers the former — search is ~500 LOC of glue. Analytics already covers the latter — `/analyze` is ~200 LOC of HTTP handler. Neither needs new infrastructure.
- **Security posture matches the work.** `/search` results are naturally row-level-authorised via `QuerySchema`. `/analyze` already runs inside the Rhai sandbox with resource caps — arbitrary SQL cannot escape.

### Non-goals for `/analyze`

- **Not a general BI query endpoint.** Studio dashboards hit their own read paths; `/analyze` is for rule authoring and power-user ad-hoc.
- **Not bypassable for writes.** `/analyze` is `dry_run`-hardcoded. If a caller wants writes, they create a Rule and a flow.
- **Not independently rate-limited.** Admission control is shared with scheduled runs; that's the point.

## Stages

| Stage | Deliverable | CI gate |
|---|---|---|
| **0** | Repo onboarding; `mani.yaml`; CI green on SQLite; feature flags | Repo builds; `mani run build --projects analytics-engine` green |
| 1 | Crate scaffolding: `analytics-core`, `analytics-domain`, `apps/analytics-engine`; SQLite OLTP migrations | Unit tests on migrations & domain types |
| 2 | Dataset compiler; `TableProvider` over `TelemetryRepo`/`HistoryRepo`; cycle+depth checks. **Push-down verification is a hard gate.** | Push-down tests pass; Dataset compiler unit tests green |
| 3 | Rule engine: SQL stage, Rhai stage, Arrow bindings, **UDF/UDAF/UDWF helpers** (`sustained`, `occupied_hours`, `last_change`, baseline), row cap, timeout, sandbox, `IntentList` assembly, **`dry_run`** | Rule engine fixture tests + bad-rule suite green |
| 4 | Zenoh queryable with CBOR + admission semaphore; agent-side `analytics.run_rule` + `analytics.apply_intents` blocks | Zenoh integration tests (success/timeout/busy/dry-run/version-tolerance) |
| 5 | HTTP + RSQL CRUD; projected schedule endpoint; manual-run / dry-run endpoints; OpenAPI | HTTP integration tests |
| 6 | Studio "Add schedule" upserts flow via agent API; Schedule projection reads back | End-to-end test: create schedule → flow appears → projection returns it |
| 7 | Timescale parity via testcontainers | Backend parity suite green |
| 8 | Studio rule editor (Monaco + grammars), Test button, run history | Studio integration smoke test |
| 9 | CLI | CLI smoke test |
| 10 | Soak test — cloud: 1000 rules, 10 pods, 24 h, Zenoh p99 < 200 ms. Edge: 50 rules, ARM 512 MB, admission serializes correctly, RSS flat, zero missed schedules. | Soak report attached to release |
| 11 | First-party rule library — 10 worked examples as tested fixtures; treat as growing asset | Library tests green |

**Revised effort estimate: 8–12 weeks for one senior engineer** to stages 0–7 + basic UI. (Review flagged 5–7 as optimistic; accepted. DataFusion TableProvider alone is 2–3 weeks when push-down must actually work end-to-end.)

**Biggest single risk:** DataFusion push-down against Timescale via `datafusion-table-providers`. Stage 2 is a hard gate.

## Open questions

- **Rule versioning.** Immutable versions with a "current" pointer vs. mutable with history. Lean immutable.
- **Rhai helpers shape.** Compiled-in module v1; revisit hot-reload / user helpers in v2.
- **Tenant context propagation.** Trigger → block → Zenoh key expression → engine validation. Confirm the bind happens at the block, not at the trigger.
- **Trace ID end-to-end.** Trigger → Zenoh → run → apply_intents → slot.write. Covered by existing `tracing` propagation? Verify in Stage 4.
- **Result fan-out cap.** Lean `OutputFanoutExceeded` at 10k intents/run, with per-rule override up to 100k.
- **Dataset composition cycle at runtime.** Cycles are compile-time errors, but if a dataset's dependency is later edited to introduce a cycle, does the next compile fail cleanly? Confirm with a test.
- **First-party rule library lifecycle.** How do we ship updates to the 10 seed rules? Embedded in the binary, fetched at runtime, or operator-installed? Lean embedded for v1.

## One-line summary

**Dedicated stateless compute sidecar in its own repo: two persisted concepts (Dataset, Rule) plus a projected Schedule view over flows, text-first Rhai rules with an optional DataFusion SQL stage, direct TSDB read via Arrow `TableProvider` with hot-path helpers as Rust UDF/UDAF/UDWF. Rule runs return an `IntentList` over a Zenoh queryable (CBOR wire, admission-controlled); flow engine owns scheduling, triggering, retry, and applying intents via existing `slot.write` / `alarm.raise` blocks. Dry-run first-class. HTTP only for Studio/CLI CRUD. Shared DB with coordinated migrations — an optional sidecar, not an independently-versioned service.**
