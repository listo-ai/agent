# Analytics Engine — Scope

Status: draft
Owners: platform/analytics
Depends on: [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md), [SLOT-STORAGE.md](SLOT-STORAGE.md), [QUERY-LANG.md](QUERY-LANG.md), [OVERVIEW.md](OVERVIEW.md), [RUNTIME.md](RUNTIME.md)
Related workflow: [ADDING-NEW-SERVICE-WORKFLOW.md](../../../ADDING-NEW-SERVICE-WORKFLOW.md)

## Purpose

Deliver a SkySpark-class analytics capability — scheduled and event-driven rules over historized slot data, producing derived points, faults, and KPIs — as a **pure compute service**. Triggering and result routing are done by the existing flow engine; analytics-engine is a stateless function: `(rule_id, context) → intents`.

## Problem

Operators want three things and none of them should be shoehorned into existing surfaces:

1. **Derived points** — `ahu.efficiency = cooling_output / electrical_input`, recomputed periodically, historized.
2. **Fault rules** — "zone_temp − setpoint > 2°C sustained 15 min during occupied hours."
3. **Rollups / KPIs** — daily kWh per meter, weekday/weekend split, 5-year retention.

Non-solutions that each fail for a specific reason:

- **Hand-coded blocks per rule.** 500 inconsistent implementations, no common RBAC, no common retention, no common authoring.
- **Rules-as-flows (logic drawn on a canvas).** Visual graphs do not scale past a few dozen rules. You cannot diff, grep, refactor, or code-review a canvas. SkySpark is text rules for a reason.
- **In-DB views / continuous aggregates only.** Timescale-only, cloud-only, no imperative logic, no sandbox. Edge and standalone are left out.
- **A fully self-contained analytics service with its own scheduler, its own output sinks, its own event bus integration.** That duplicates the flow engine. The flow engine already does cron, event triggers, leader election, audit, and sinks; building a second one inside analytics-engine is waste.

The honest answer is a **dedicated compute service** that owns the rule/dataset model and the DataFusion+Rhai engine, and **delegates everything orchestration-flavoured to flows**.

## Goals

1. **Three concepts.** *Dataset*, *Rule*, *Schedule*. Everything else (runs, intents, side effects) is either audit or reuses existing graph surfaces.
2. **Text-first authoring.** A Rule is a Rhai script with a typed header declaring input Datasets and declared output intents. No canvas. Diffable, greppable, code-reviewable, unit-testable.
3. **Off-the-shelf engine.** DataFusion for columnar query, Arrow as zero-copy handoff, Rhai for imperative logic, sandboxed. Zero custom languages, zero custom parsers.
4. **Direct TSDB read.** Analytics-engine reads Timescale/SQLite through its own read-only pool. DataFusion push-down (time range, kind filter, bucket) happens in the database. No HTTP-paginated re-fetch, no duplicated bucketing.
5. **Pure function — return intents, never apply them.** A rule run produces a structured `intents` list (slot writes, alarms, events). The flow engine applies them through existing `slot.write` / `alarm.raise` blocks. This keeps authorization, audit, and rate-limiting in one place — the agent.
6. **Flow engine owns scheduling and orchestration.** Two new flow blocks: `analytics.run_rule` (calls the service, returns intents) and `analytics.apply_intents` (expands intents into existing sinks). Cron, event triggers, leader election, retry, audit, downstream chaining — all reuse what flows already do.
7. **Zenoh for service-to-service.** The `run_rule` RPC rides Zenoh queryables between the flow engine (in the agent) and analytics-engine. HTTP exists only for outside-world CRUD (Studio, CLI).
8. **Edge / cloud parity, one binary.** Same service, SQLite backend on edge and tests, Postgres+Timescale on cloud. Feature-flagged out on legacy ARM (armv7, 256 MB) — not a target.
9. **Clean reuse of platform primitives.** REST shape follows [QUERY-LANG.md](QUERY-LANG.md). Logging, tracing, error handling follow [OVERVIEW.md § Key libraries](OVERVIEW.md#key-libraries). No new patterns.

## Non-goals

- **Dashboards / charts.** Studio owns presentation. This scope is compute only.
- **Custom tag query grammar (Axon-style).** Node selection stays RSQL. DataFusion SQL is a *rule-author tool*, not a user-facing query surface.
- **Parallel scheduler, parallel event bus, parallel output sinks.** Flow engine does all three; don't duplicate.
- **Parallel authorization system.** Analytics-engine returns intents; the agent authorizes each intent at apply time via the same RBAC that governs any slot write or alarm.
- **Applying side effects directly from analytics-engine.** Ever. If analytics-engine wrote slots, you'd fragment audit and re-implement rate limiting. Intents only.
- **ML / forecasting.** v1 is aggregation + imperative rules. Defer.
- **Retroactive backfill.** Editing a rule does not recompute history. v2; requires idempotent past-dated slot writes.
- **Streaming analytics (per-sample evaluation).** Windowed scheduled / event-triggered runs only.
- **Bundled into the agent binary.** Separate service. Optional per deployment.

## Key decisions

| Decision | Choice | Why |
|---|---|---|
| Repo shape | New top-level repo `listo-ai/analytics-engine`, onboarded per [ADDING-NEW-SERVICE-WORKFLOW.md](../../../ADDING-NEW-SERVICE-WORKFLOW.md) | Independent deploy, compile, release cadence |
| Binary | Single `analytics-engine` binary, cross-compiled per [OVERVIEW.md § Build targets](OVERVIEW.md#build-targets-rust-triples) | Matches platform convention |
| Domain model | `Dataset` + `Rule` + `Schedule` — three tables plus a `rule_run` audit table | Smallest surface that covers all three use cases |
| Authoring surface | Text (Rhai) with typed input/output header | Diffable, testable, scales to thousands |
| Columnar engine | [DataFusion](https://datafusion.apache.org/) | Mature Rust, Arrow-native, SQL-compatible, parallel execution built in |
| In-memory model | [Apache Arrow](https://arrow.apache.org/) | Zero-copy between DataFusion and Rhai bindings |
| Imperative language | [Rhai](https://rhai.rs/) | Embeddable, sandboxable, no GC, no runtime deps |
| Read path | Direct TSDB with read-only role via DataFusion `TableProvider` | Push-down is impossible through HTTP; direct read is the only performant answer |
| Write path (apply) | **Not analytics-engine's job.** Rules return intents; flow engine's `apply_intents` block routes them to existing `slot.write` / `alarm.raise` | Single authorization + audit path; zero duplication |
| Scheduling | **Flow engine.** Cron triggers, slot-change triggers, manual triggers already exist | Don't build a second scheduler |
| Leader election | **Flow engine.** Already solved for scheduled flows | Don't build a second one |
| Service-to-service RPC | **Zenoh queryables.** Key: `listo/<tenant>/analytics/rule/run` | Already in the platform per OVERVIEW.md; free discovery + load balancing; edge-native |
| External RPC | HTTP (axum) | Studio, CLI, any outside-mesh caller |
| OLTP storage | Same DB as the agent, separate schema `analytics` | One backup, one pool story; schema boundary prevents coupling |
| Result storage | Nothing. Intents flow through the graph via slot writes. | "Everything as a node" holds. Retention/charts/telemetry free. |
| Audit storage | `analytics.rule_run` — start/end, status, rows in/out, error, trigger context hash | Runs are the only analytics-specific persisted state |
| Query framework | Rule/Dataset/Run exposed via RSQL + `#[derive(Queryable)]` | Zero custom controllers |
| Rhai sandbox | `max_operations`, `max_memory`, `max_call_levels`, disabled fs/net stdlib | Same floor cloud and edge so behaviour matches |
| Row-count cap | Hard platform default (500k post-SQL), per-rule override | Predictable memory; fail fast with `PlanTooLarge` |
| DataFusion SQL dialect | DataFusion's own dialect, documented explicitly | Avoid the Postgres-looks-like-SQLite confusion |
| Legacy ARM (armv7 256 MB) | Not supported | DataFusion doesn't fit the budget |

## Domain model

Three resources in the `analytics` schema. All OLTP, all queryable via RSQL.

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

Datasets can reference other Datasets (views over views), bounded depth (lean 4).

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
  SELECT t.path, t.ts_bucket, t.value - s.value AS delta
  FROM temps t JOIN setpoints s USING (path, ts_bucket)
  WHERE t.value - s.value > 2.0

rhai: |
  let fault = sustained(rows, |r| r.delta > 2.0, duration = "15m");
  let severity = if fault.any() { "warning" } else { "none" };
  return { fault, severity };
```

The `sustained`, `occupied_hours`, `last_change` helpers ship in a first-party Rhai module.

### Schedule

**A Schedule is a pointer to a flow.** Analytics-engine stores the rule + the *desired* trigger shape; the flow that actually runs it lives in the flow engine.

```yaml
# schedule: zone_fault_every_5m
rule: zone_fault_sustained
trigger:
  cron: "*/5 * * * *"                # or: on_slot_change: { path, slot }
jitter_ms: 5000
enabled: true
```

Creating a Schedule in analytics-engine triggers (via Zenoh) a "materialise this flow" request handled by the agent — or, equivalently, Studio can generate the flow directly when the user saves a schedule. Either way, the flow contains:

```
[trigger: cron "*/5 * * * *"]
   ↓
[analytics.run_rule (rule_id = ...)]
   ↓
[analytics.apply_intents]
   ├─→ slot.write (for each slot_write intent)
   └─→ alarm.raise (for each alarm intent)
```

The user sees the Schedule in the Analytics UI. The flow is an implementation detail. Whichever side the user edits (schedule cron in Analytics, or the flow in Flow Studio), the other side stays consistent because the flow's nodes reference the rule by id.

### Rule run (audit)

Written per Zenoh invocation. Not a user resource; exposed read-only via RSQL for debugging.

```
rule_id, started_at, ended_at, status, rows_in, intents_out,
trigger_kind, error_code, error_message, duration_ms, trigger_context_hash
```

## The two new flow blocks

Two blocks, full stop. Not a block set.

| Block | Inputs | Output | What it does |
|---|---|---|---|
| `analytics.run_rule` | `rule_id`, `trigger_context` (JSON) | `intents: IntentList` | Zenoh `get` to `listo/<tenant>/analytics/rule/run`; payload is `{rule_id, trigger_context}`; reply is the `IntentList`. Typed errors (`PlanTooLarge`, `RhaiSandboxViolation`, `Timeout`, `AnalyticsUnavailable`) surface as flow-node errors. |
| `analytics.apply_intents` | `intents: IntentList` | — | Expands each intent into a call on the existing `slot.write` / `alarm.raise` / `event.emit` blocks. Runs them in parallel with per-intent error isolation (one failed intent doesn't fail the batch). |

Everything else — cron, slot-change triggers, retries, leader election, downstream chaining, human approval gates — is standard flow composition.

## Intent wire format

Typed, serde-serializable, versioned enum:

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Intent {
    SlotWrite  { path: String, slot: String, value: SlotValue },
    Alarm      { severity: Severity, message: String, source: String, dedup_key: Option<String> },
    EventEmit  { topic: String, payload: serde_json::Value },
}

pub struct IntentList {
    pub version: u32,             // bump on breaking changes
    pub rule_id: Ulid,
    pub run_id: Ulid,
    pub intents: Vec<Intent>,
}
```

v1 ships three intent kinds. Adding a fourth = serde enum variant + `apply_intents` match arm. No other service changes.

## Stack

One library per concern. Matches [OVERVIEW.md § Key libraries](OVERVIEW.md#key-libraries); three new entries specific to this service.

| Concern | Crate | Notes |
|---|---|---|
| Async runtime | `tokio` | Platform standard |
| HTTP server (external CRUD) | `axum` + `tower-http` | Platform standard |
| Service-to-service RPC | **`zenoh`** | Queryable on `listo/<tenant>/analytics/rule/run` |
| Serialization | `serde` + `serde_json` + `serde_yml` + `bincode` | JSON/YAML for persistence & REST; bincode for Zenoh payloads |
| JSON Schema | `schemars` | Auto-docs for rule/dataset headers |
| Error handling | `thiserror` in libs, `anyhow` in the binary | Platform standard |
| Logging / tracing | `tracing` + `tracing-subscriber` | Platform standard |
| **Columnar engine** | [`datafusion`](https://crates.io/crates/datafusion) | New. Query, planning, execution, push-down |
| **Columnar memory** | [`arrow`](https://crates.io/crates/arrow) | New. RecordBatch / Array handoff |
| **Rule scripting** | [`rhai`](https://crates.io/crates/rhai) | New. Sandboxed embedded scripting |
| TSDB providers | [`datafusion-table-providers`](https://crates.io/crates/datafusion-table-providers) | SQLite + Postgres `TableProvider` impls |
| SQLite | `rusqlite` (bundled) | Platform standard |
| Postgres | `sqlx` | Platform standard |
| Query framework | `query` workspace crate from [QUERY-LANG.md](QUERY-LANG.md) | RSQL + schema derive for OLTP resources |
| Datetime | `jiff` | Platform standard |
| Testing | `tempfile` + `testcontainers` for Timescale | SQLite in-process is the default test path |

**Not in the stack:** no scheduler crate, no NATS, no gRPC, no Arrow Flight, no output-sink crate. All scheduling + routing is flow-engine territory.

## Architecture

```
  ┌───────────────────────────────────────────┐
  │ Agent (flow engine)                       │
  │                                           │
  │   [cron / slot.changed / manual trigger]  │
  │               │                           │
  │               ▼                           │
  │     [analytics.run_rule block] ──zenoh──┐ │
  │               │                        │  │
  │               ▼                        │  │
  │     [analytics.apply_intents block]    │  │
  │       ├─▶ [slot.write]                 │  │
  │       └─▶ [alarm.raise]                │  │
  └────────────────────────────────────────│──┘
                                           │
                                  Zenoh queryable
                        `listo/<tenant>/analytics/rule/run`
                                           │
  ┌────────────────────────────────────────▼──┐
  │ analytics-engine  (stateless; N pods)     │
  │                                           │
  │   HTTP (axum)          Zenoh queryable    │
  │   • CRUD datasets      • run_rule RPC     │
  │   • CRUD rules              │             │
  │   • CRUD schedules          ▼             │
  │   • list runs        ┌──────────────┐     │
  │         │            │  Rule Engine │     │
  │         └──────┬─────┴──────┬───────┘     │
  │                │            │             │
  │       ┌────────┴────┬───────┴────────┐    │
  │       ▼             ▼                ▼    │
  │  ┌─────────┐  ┌───────────┐  ┌──────────┐ │
  │  │ Dataset │  │DataFusion │  │   Rhai   │ │
  │  │compiler │─▶│ + Arrow   │─▶│ sandbox  │ │
  │  └────┬────┘  └─────┬─────┘  └────┬─────┘ │
  │       │             │             │       │
  │       ▼             ▼             ▼       │
  │  OLTP schema   TSDB (read-only)  IntentList│
  │  analytics.*   SQLite/Timescale  (reply)  │
  │                via TableProvider          │
  └───────────────────────────────────────────┘
           │                   │
           ▼                   ▼
     Shared DB            Shared DB
     (analytics           (graph/history
      schema)              schema, RO)
```

### Data flow for one rule run

1. Flow trigger fires (cron / slot-change / manual) inside the agent.
2. `analytics.run_rule` block sends a Zenoh query to `listo/<tenant>/analytics/rule/run` with `{rule_id, trigger_context}`.
3. Analytics-engine claims the run, writes `rule_run` row with `status=running`.
4. Dataset compiler resolves named Datasets → DataFusion `LogicalPlan` per input.
5. DataFusion executes against TSDB with push-down (range, kind, bucket, agg). Streaming Arrow batches return.
6. SQL stage (if present) runs over mounted inputs. Row cap enforced on final result.
7. Rhai stage runs with Arrow column bindings. Sandbox limits enforced by Rhai.
8. Intents assembled, `rule_run` row closed with final status. `IntentList` returned over Zenoh.
9. `analytics.apply_intents` expands the list, calling `slot.write` / `alarm.raise` per intent. Per-intent failures are audited, the batch as a whole succeeds.

## Storage

- **OLTP tables** in the `analytics` schema, same DB as the agent:
  - `analytics.dataset` — id, tenant_id, name, spec, version, created/updated
  - `analytics.rule` — id, tenant_id, name, header, sql, rhai, version, enabled, resources
  - `analytics.schedule` — id, tenant_id, rule_id, trigger, jitter_ms, enabled, flow_id (pointer to the materialised flow)
  - `analytics.rule_run` — id, rule_id, started_at, ended_at, status, rows_in, intents_out, trigger_kind, error_code, error_message, trigger_context_hash
- **No result tables.** Intents flow through the graph.
- **No duplicated telemetry schema.** Reads go through existing TSDB via DataFusion `TableProvider`.
- **Migrations** owned by the analytics-engine repo, applied on startup with `sqlx::migrate!`.

## Scaling shape

| Dimension | Edge / standalone | Cloud |
|---|---|---|
| Processes | 1 analytics-engine process | N stateless pods |
| Scheduling | Flow engine (agent) | Flow engine (agent / cloud agent) |
| Leader election | N/A — flow engine handles it | N/A — flow engine handles it |
| DataFusion parallelism | Single node, partition-parallel within a rule | Same, multiplied by pod count |
| Zenoh routing | Local queryable on the same host | Queryable across the mesh; Zenoh load-balances queries |
| Memory | 150 MB budget on ARM 512 MB; row cap enforced | Pod-sized (1–4 GB typical) |
| TSDB pool | SQLite file, WAL, read-only handle | Postgres read-only role, pool tuned per pod |

**Statelessness is the scale lever.** Analytics-engine holds no per-run state outside the DB. A pod can crash mid-run; the flow's timeout fires, the flow's retry policy re-invokes, the next Zenoh query lands on a healthy pod.

## Deployment profiles

From [OVERVIEW.md § Deployment profiles](OVERVIEW.md#deployment-profiles):

| Profile | Analytics engine shape |
|---|---|
| Cloud — multi-tenant | Separate deployment, N pods, Postgres+Timescale, Zenoh routed |
| Cloud — single-tenant | Single pod, same DB as agent |
| Edge — x86 / ARM64 ≥ 512 MB | Single process co-located with agent, SQLite backend, local Zenoh queryable |
| Edge — legacy ARM (armv7) | Not supported — feature-gated out at compile time |
| Standalone appliance | Single process bundled with agent, SQLite, local Zenoh |
| Developer laptop | Single process, SQLite, default for `cargo run` and tests |

## Surfaces

| Surface | Shape |
|---|---|
| **Zenoh queryable** | `listo/<tenant>/analytics/rule/run` — bincode `{rule_id, trigger_context}` → `IntentList`. The only service-to-service surface. |
| **HTTP REST** | `/api/v1/analytics/datasets`, `/rules`, `/schedules`, `/runs` — all RSQL per QUERY-LANG.md. For Studio/CLI only. |
| **Flow blocks** | `analytics.run_rule`, `analytics.apply_intents` — added to the flow engine's block registry |
| **CLI** | `listo-analytics rule list/create/edit/run`, `dataset ...`, `schedule ...` — all call HTTP |
| **Studio** | Analytics section: Monaco rule editor (Rhai + DataFusion SQL grammars), schedule editor, run-history drill-down. On save, Studio also upserts the materialised flow for each schedule. |
| **OpenAPI** | Auto-generated via `utoipa` + `QuerySchema` |
| **Shared DB** | New `analytics` schema; migrations owned by this service |

## Stages

| Stage | Deliverable | Prerequisites |
|---|---|---|
| **0** | Repo onboarding per ADDING-NEW-SERVICE-WORKFLOW.md; `mani.yaml` entry; CI green on SQLite path; feature flags decided | — |
| 1 | Crate scaffolding: `analytics-core` (engine), `analytics-domain` (types + repo traits), `apps/analytics-engine` (binary); SQLite OLTP migrations | 0 |
| 2 | Dataset compiler → DataFusion `LogicalPlan`; `TableProvider` adapter over `TelemetryRepo`/`HistoryRepo`; SQLite integration tests against seeded history. **Push-down verification is a gate** — if push-down doesn't work cleanly, surface it before Stage 3. | 1 |
| 3 | Rule engine: SQL stage + Rhai stage + Arrow bindings (column access, row iter, `sustained`, `occupied_hours`, `last_change` helpers); row cap, timeout, sandbox limits; `IntentList` assembly | 2 |
| 4 | Zenoh queryable on `listo/<tenant>/analytics/rule/run`; bincode payloads; typed errors; agent-side `analytics.run_rule` + `analytics.apply_intents` blocks | 3 |
| 5 | HTTP + RSQL CRUD for Dataset/Rule/Schedule/Run; manual-run endpoint; OpenAPI | 3 |
| 6 | Schedule → flow materialisation: Studio (or a small agent-side service) upserts a flow per Schedule, nodes reference `rule_id` | 5, flow engine stable |
| 7 | Postgres+Timescale backend parity: identical integration suite against testcontainers Timescale | 2–6 |
| 8 | Studio rule editor (Monaco with grammars), schedule editor, run history | 5 |
| 9 | CLI: `listo-analytics` subcommands | 5 |
| 10 | Soak test — cloud: 1000 rules, 10 pods, 24 h, pod-restart-resilient, Zenoh query p99 < 200 ms. Edge: 50 rules on ARM 512 MB, RSS flat, zero missed schedules over 24 h. | 1–9 |
| 11 | First-party rule library — ten worked-example rules (derived efficiency, sustained fault, occupancy-gated alarm, daily KPI, rolling average, deviation from baseline, min/max/avg, alarm flood suppression, demand spike detection, runtime accumulator) shipped as tested fixtures | 3 |

**Revised effort estimate:** ~5–7 weeks for one senior engineer to stages 0–7 + basic UI. Down from ~2–3 months because scheduler, leader election, event-trigger wiring, and output sinks are now flow-engine-owned.

**Biggest single risk:** DataFusion push-down against Timescale via `datafusion-table-providers`. Stage 2 is a hard gate. If push-down is weaker than expected, the fallback is to pull raw rows and bucket in Arrow — still works but raises memory pressure on cloud pods. Prototype first.

## Open questions

- **Schedule ↔ flow consistency.** If a user edits the materialised flow directly (changing the cron, adding nodes), does analytics-engine's Schedule row get out of sync? Lean: the flow is the source of truth for triggering; analytics-engine's Schedule carries only the *canonical* definition and a "drifted" flag if the flow diverges. Needs a concrete resolution policy.
- **Zenoh timeout / retry policy.** What's the default timeout for the `run_rule` queryable? 30 s matches the rule `timeout_ms` default, but long-running rules should surface progress. v1: flow-level timeout only. v2: progress events on a sibling Zenoh key.
- **Rule versioning.** Immutable versions with a "current" pointer (clean, migration-friendly) vs. mutable with history (simpler). Lean immutable.
- **Dataset composition depth cap.** Datasets-over-datasets bounded at 4. Confirm.
- **Result fan-out limits.** A rule over `kind=zone_sensor` × 10k zones emits 10k slot-write intents per run. Cap at analytics-engine (`OutputFanoutExceeded`) + rate-limit at the agent's apply_intents block. Pick the cap (lean 10k).
- **Rhai helpers shape.** Ship as compiled-in module v1; revisit hot-reload or user-authored helpers in v2.
- **Tenant context propagation.** Flow-engine trigger carries the tenant; Zenoh queryable is per-tenant via key expression; analytics-engine validates. Confirm the bind happens at the block and not at the trigger.
- **Trace ID end-to-end.** Trigger → Zenoh → run → apply_intents → slot.write. Single trace ID across all of it is essential for debug. Covered by existing `tracing` + agent trace-context propagation?

## One-line summary

**Dedicated stateless compute service in its own repo: three concepts (Dataset, Rule, Schedule), text-first Rhai rules with an optional DataFusion SQL stage, direct TSDB read via Arrow `TableProvider`. Rule runs return an `IntentList` over a Zenoh queryable; the flow engine owns scheduling, triggering, retry, and applying intents via the existing `slot.write` / `alarm.raise` blocks. HTTP only for Studio/CLI CRUD. Off-the-shelf DataFusion + Arrow + Rhai stack; no custom grammars, no result tables, no duplicate scheduler, no duplicate event bus.**
