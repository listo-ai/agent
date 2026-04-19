# Slot Value Storage & Historization — Scope

Status: draft (revised after peer review)
Owners: platform/graph
Depends on: [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md), [RUNTIME.md](RUNTIME.md), [QUERY-LANG.md](QUERY-LANG.md)

## Purpose

Define how a node preserves slot values over time — current value, change history, and structured records — across edge, cloud, and standalone deployments, without adding a parallel system outside the graph.

## Problem

Slots today hold live values. There is no declared way to:

- Store arbitrary value types uniformly (`bool`, `number`, `string`, `json`).
- Record a slot's value on change, on interval, or on demand.
- Query historical values through the same surfaces as current values.
- Keep the same behaviour on a 512 MB edge box as on a cloud pod.

Without this, every extension that wants history invents its own table and every flow that wants "last hour of values" calls a different API per driver. That is the exact failure mode [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) exists to prevent.

## Goals

1. **Uniform slot value shape.** One `SlotValue` tagged union covers `Null | Bool | Number | String | Json | Binary`. Same wire format, same storage, same RBAC.
2. **Historization as a node.** Record policies are a node kind (`sys.core.history.config`) attached as a child of the node whose slots are being recorded. **One config per node, per-slot policy declared in its settings** — not one config per slot (see "Cardinality" below).
3. **Three write triggers, one implementation.**
   - **COV** — semantics depend on slot type (see "COV semantics per type" below).
   - **Interval** — periodic sample at `period_ms`, optionally aligned to wall-clock multiples of `period_ms` from epoch.
   - **On demand** — only records when a flow or REST call fires `history.record`.
4. **One database per deployment, schema-split by access pattern.** The same DB holds OLTP tables, time-series tables, and structured-history tables. Edge = single SQLite file. Cloud = single Postgres with the TimescaleDB extension enabled. No separate telemetry service, no dual-write coordination.
5. **Storage split as a hard contract (schema, not DB).** `Bool` and `Number` slots → time-series tables (rolling buckets on edge, Timescale hypertables on cloud). `String`, `Json`, `Binary` → regular `slot_history` table. Declaration at schema time binds the table shape; no implicit routing, no overrides. If an author wants time-series semantics (downsample, range aggregation), they declare `Number`.
6. **Edge ↔ cloud parity.** The historizer trait is identical in both. The `TelemetryRepo` seam ([CODE-LAYOUT.md `data-tsdb`](CODE-LAYOUT.md)) exists to hide SQL-dialect differences (SQLite window functions vs Timescale `time_bucket`), **not** to hide a separate database.
7. **Queryable through existing surfaces.** Current value via `GET /slots/{slot}` (unchanged); scalar history via `/api/v1/telemetry/*` (range + aggregation + downsample); structured history via RSQL on `/slots/{slot}/history`.
8. **Generic.** Any node kind — first-party or extension — gets historization for free by declaring slot types and letting users attach a `HistoryConfig` child.

## Non-goals

- Full-text search over historical records.
- Complex aggregations beyond Timescale/SQLite native avg/min/max/downsample. `GROUP BY` analytics stays on its own endpoint.
- Cross-tenant history queries. Same per-tenant isolation as the rest of the graph.
- **Cross-schema history queries.** A single query returning both scalar (time-series tables) and structured (regular tables) records is not supported. Callers that need a merged timeline issue two queries and merge client-side. Documented as an API contract.
- **Separate telemetry database.** One DB per deployment. If scalar volume ever outgrows shared Postgres in a cloud deployment, the `TelemetryRepo` seam allows splitting the time-series tables to a dedicated Timescale cluster without touching callers — but that's a future migration, not a v1 design concern.
- Hard real-time recording latency. Target: p99 < 500 ms on edge, best-effort beyond.
- Retroactive backfill of history for slots that predate the attached config.
- Migrating data between table shapes (time-series ↔ regular) after the fact. A slot's declared type is permanent for history purposes.
- Historical access-control re-evaluation. A user who gains slot-read access today can read all prior history for that slot; revocation removes future reads only. If deployments need stricter semantics, that's a v2 hardening story.
- Pattern-based / template history configs (e.g. "all output slots on children of kind X"). Explicit per-node configs only in v1.

## Key decisions

| Decision | Choice | Why |
|---|---|---|
| Where policy config lives | A child node (`sys.core.history.config`) | Composable, RBAC-uniform, audit-uniform, extensions can add new store kinds |
| Cardinality | **One `HistoryConfig` per node**, per-slot policy in its settings | Collapses 500k-slot fan-out by ~20× on typical device trees; keeps everything declarative and queryable |
| Database topology | One DB per deployment (SQLite on edge, Postgres+Timescale on cloud) | One transaction domain, one backup, one auth, one connection pool |
| Storage split | Hard contract: `Bool|Number` → time-series tables, `String|Json|Binary` → regular `slot_history` table | Schema-level split in the same DB; authors pick table shape by picking the slot type |
| COV deadband applicability | `Number` only; other types use structural/byte equality | Deadband on non-numeric is meaningless |
| Edge store | SQLite with ring-buffer + bounded quota | Fits 256 MB budget, survives disconnection, syncs via outbox |
| Schema shape | `slots.value` as JSONB with `kind` discriminator | One column covers all types |
| COV heartbeat | Required (`max_gap_ms`), not optional | Without it, flat-line is indistinguishable from dead sensor |
| On-demand trigger | REST endpoint + flow node | Matches existing "write slot" symmetry |
| History-read RBAC | Identical to slot-read RBAC, evaluated at query time | Simple, matches graph-wide rule; no historical re-check |
| `historized` fleet events | Opt-in per config; default off for `cov`/`interval` | Per-record publish on chatty slots is expensive; the slot-change event is already on the bus |
| Sync priority | History bulk-sync rides a **separate outbox lane** from operational traffic | Command-acks, lifecycle events, and fleet commands must not be starved by history catch-up after reconnect |
| Sample caps | **Global per-slot sample cap** with per-slot override in `HistoryConfig` | Prevents runaway slots from eating disk; predictable disk budget across fleet |

## COV semantics per type

| Slot type | "Changed" means |
|---|---|
| `Number` | `|new - last_recorded| > deadband` (deadband required; `0` for any-change) |
| `Bool` | `new != last_recorded` |
| `String` | byte equality |
| `Json` | structural equality (recursive deep-equal) **up to 64 KiB**; beyond that, degrades to byte equality to cap CPU cost |
| `Binary` | byte equality, subject to size cap |

All types respect `min_interval_ms` (rate floor) and `max_gap_ms` (heartbeat ceiling).

## Cardinality — one config per node

A `HistoryConfig` references its parent node and declares a map of `slot_name → policy` in settings. A BACnet device with 20 historizable points has **one** `HistoryConfig` child, not 20. Per-slot policy stays explicit and declarative; fan-out drops from O(slots) to O(nodes).

Settings shape (sketch):

```yaml
slots:
  zone_temp:        { policy: cov,      deadband: 0.5, min_interval_ms: 1000, max_gap_ms: 900000 }
  damper_position:  { policy: interval, period_ms: 60000, align_to: wall, max_samples: 500000 }
  fault_code:       { policy: on_demand }
retention:
  keep_for_days: 30            # see open question on per-backend override
  max_samples_per_slot: 100000 # per-config override of the platform default (see "Sample caps" below)
publish_historized_events: false
critical: false
```

**Default cardinality is one `HistoryConfig` per node.** Multiple configs per node are allowed but opt-in, gated by a platform setting (`history.allow_multiple_configs_per_node`, default `false`). The typical use case is "short-term + long-term" profiles, and it should be a deliberate operator choice, not accidental.

## Sample caps — global default, per-slot override

Every historized slot has a ceiling on how many samples it retains. This is orthogonal to time-based retention (`keep_for_days`) — whichever limit hits first wins.

| Level | Setting | Default |
|---|---|---|
| **Platform (edge)** | `history.max_samples_per_slot` in agent config | 100,000 |
| **Platform (cloud)** | same | 10,000,000 |
| **Per config** | `retention.max_samples_per_slot` in `HistoryConfig` settings | inherits platform |
| **Per slot** | `slots.<name>.max_samples` in `HistoryConfig` settings | inherits config |

The precedence is slot → config → platform. Enforcement is a rolling window: when a new record lands that would exceed the cap, the oldest record for that slot is evicted in the same transaction. No async compaction, no "sweep every N minutes" — the cap is enforced at write time so disk usage stays predictable.

For time-series tables, the cap translates to either row-count on the hypertable/rolling-bucket, or to Timescale retention policies with matching chunk boundaries. For `slot_history`, it's a simple `DELETE ... WHERE id IN (SELECT ... LIMIT 1)` co-committed with the insert.

**Why a global default exists at all.** An extension author declaring a new node kind shouldn't be able to default-configure a slot that eats a gateway's disk in a week. The platform default is a safety net. Operators with real sizing information raise it per-config or per-slot.

## Buffered writes — in-memory pool, bulk flush

Per-historizer records go through an in-memory ring buffer and flush to disk in bulk. This is the primary throughput mechanism, not an optional optimisation — per-row commits on SQLite/Postgres are ~50–100× slower than batched transactions, and the design assumes edge devices handle thousands of slot changes per second without wilting.

### Flush triggers — whichever fires first

| Trigger | Default | Rationale |
|---|---|---|
| Time | **5 s** (edge), **10 s** (cloud) | Bounds data-loss window. 30 s is too long for edge — power events, SIGKILLs, and OOM kills all hit within that window. |
| Batch size | 500 records (edge), 2000 (cloud) | Caps per-commit memory and transaction time |
| Memory pressure | Force-flush at 80% of per-config queue cap | Bursts don't OOM the process |
| Graceful shutdown | Flush all buffers within a 2 s bounded timeout | Required by the SIGTERM safe-state path in [RUNTIME.md](RUNTIME.md) |
| Critical tier | **Every write, no buffering** | Audit-relevant configs (`critical: true`) pay per-write latency in exchange for never-lost guarantee |

Flush interval and batch size are settings on the `HistoryConfig`; defaults are sane for 99% of uses.

### Back-pressure and the `SlotWriteRejected` contract

When the historizer can't keep up, its behaviour depends on the config's tier:

| Tier | Buffer full behaviour | Producer sees |
|---|---|---|
| Default (`critical: false`) | Drop oldest records; emit `HistorizerOverflow` health event | Write succeeds; health event surfaces the loss |
| Critical (`critical: true`) | Refuse new writes — buffer bypass means the underlying store is the bottleneck | **Synchronous `SlotWriteRejected`** carrying `{ reason, retry_after_ms }` |

**`SlotWriteRejected` contract (critical tier only):**

- **Extensions:** propagate as a typed error to the caller. No retry inside the extension.
- **Flow engine:** fails the flow node with a `SlotWriteRejected` outcome; the flow's error-handling wires decide what happens next (retry node, alarm, drop).
- **REST/gRPC:** returns HTTP `503` with `Retry-After: <ms>` / gRPC `RESOURCE_EXHAUSTED` with equivalent metadata.
- **Historizer itself:** never retries internally. No unbounded in-process queues, no silent drops.

The rule is: critical-tier producers get a clear "no, try again in X" signal, and the decision of whether to retry belongs to them. This keeps back-pressure visible end-to-end rather than hidden inside the historizer.

### Crash semantics — stated as a contract

**Hard crash (SIGKILL, power loss, OOM kill) loses up to `flush_interval` worth of records from non-critical configs.** This is the tradeoff. Three knobs cover the cases users actually care about:

1. **Default tier.** ≤ 5 s loss window on edge. Fine for telemetry, trend logs, diagnostic history.
2. **Critical tier.** `critical: true` in the `HistoryConfig` settings bypasses the buffer entirely — every record commits per-write. Use for security events, lifecycle transitions, anything with regulatory implications. Expect ~100× lower throughput on edge; that's the cost.
3. **Graceful shutdown.** SIGTERM flushes all buffers before safe-state runs. Upgrades, operator-initiated stops, and systemd restarts do not lose data.

The slot's *live value* is never affected — only history records in-flight between the slot-change event and the next flush are at risk.

### What we don't build

- **No userspace WAL.** SQLite's WAL + `PRAGMA synchronous=NORMAL` already defers fsync. A second write-ahead file in userspace doubles write amplification and re-implements what the DB gives us.
- **No per-slot buffers.** Per-config is enough; per-slot explodes the flush scheduler at 500k-slot scale.
- **No timestamp coalescing within a batch.** If two records land in the same millisecond, keep both. Dedup/downsample is retention's job, not the buffer's.

## Binary storage

- Stored in SQL `slot_history` as a blob column, **not** in the node's live slot (which holds a content hash + size).
- Per-record size cap: 1 MiB default, configurable per `HistoryConfig`, hard max 16 MiB.
- `Binary` historization is **off by default on edge profiles** (`armv7` and `aarch64` ≤ 512 MB); opt-in via deployment config. Cloud and standalone default on.
- **Byte-per-day cap alongside the row cap.** A single 16 MiB record consumes 16,000× more disk than a scalar record, so a pure row-count cap is the wrong primary limit for Binary slots. Setting: `slots.<name>.max_bytes_per_day` (default 100 MiB per slot on edge, 10 GiB on cloud). Whichever of row cap, byte cap, and `max_samples` hits first triggers eviction.

## Edge cases / risks

- **Chatty slots on small edge.** Per-config daily row cap declared in settings; breach → `Degraded` lifecycle + event, recording throttled.
- **Historizer internal back-pressure.** The historizer maintains a bounded per-config ring buffer (default 10k records on edge, 100k cloud). Overflow behaviour is tiered — see "Back-pressure and the `SlotWriteRejected` contract" above. Throughout this doc, "ring buffer" and "queue" refer to the same structure; the former is the implementation, the latter the observable shape.
- **Config-node fan-out at scale.** With one config per node (not per slot), a 500k-point tree of ~25k devices produces ~25k config nodes, not 500k. Still non-trivial but within the graph's designed envelope. `isSystem` facet hides them from default children listings; `list_children(path, include_system=false)` is the default.
- **Legacy ARM (`armv7`, 256 MB).** `Json` and `Binary` historization feature-gated off; scalar-only (`Bool`/`Number`) always available.
- **Extension crash mid-record.** Same `Stale` lifecycle rule as [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) — subscribers see the transition. In-flight records in the historizer queue survive the extension's death (the extension doesn't own the queue).
- **Clock skew on edge.** COV `min_interval_ms` and interval spacing use the agent's monotonic clock; wall-clock only for `align_to: wall` (align to the nearest multiple of `period_ms` from Unix epoch). Records carry both monotonic offset and wall-clock timestamp. **Each record also carries the edge's NTP sync state at record time**: `ntp_synced: bool` and `last_sync_age_ms`. Downstream consumers can weight or flag records written while the clock was drifting. Post-hoc wall-clock correction is still impossible (we never learn the true offset at record time), but consumers have the signal they need to detect suspect records.
- **`SlotValue` migration.** Existing `slots.value` is `TEXT NOT NULL` (JSON-encoded). Strategy: add nullable `kind` column; lazy-populate on next write; one-shot backfill at upgrade time behind a progress indicator, chunked to avoid long SQLite write locks (batch size 1000, commit per batch). **Routing is driven by the kind registry, not by `slots.kind`.** The column is a denormalisation for query filters (e.g. `?filter=kind==number`); its population status has no effect on historizer behaviour, because the historizer reads the declared slot type from the kind registry built at boot.

## Surfaces affected

| Surface | Change |
|---|---|
| Graph service | New kind `sys.core.history.config`; `SlotValue` union formalised |
| Persistence | Regular `slot_history` table + time-series tables (SQLite rolling buckets / Timescale hypertables); `slots.kind` column |
| REST | `POST /nodes/{path}/slots/{slot}/history/record`; `GET /nodes/{path}/slots/{slot}/history` (RSQL); scalar telemetry endpoint already exists |
| CLI | `yourapp node history record <path> <slot>`; `yourapp node history list <path> <slot> --from --to` |
| Studio | Property-panel "History" section on nodes with a `HistoryConfig` child; time-series chart widget |
| Flow engine | Built-in nodes: `history.record`, `history.read-range`, `history.on-recorded` trigger — **fires once per flush batch**, payload is `{ config_id, records: [...] }`; per-record iteration is the flow's job. Distinct from `slot.changed`. |
| Fleet bus (Zenoh) | Opt-in `graph.<tenant>.<path>.slot.<slot>.historized` events (off by default); see [FLEET-TRANSPORT.md](FLEET-TRANSPORT.md) |
| Outbox | **Separate lane** for history bulk-sync. History records do not share quota with operational traffic (commands, lifecycle, safety events). Own quota, own back-pressure signal, own drop policy (oldest-first telemetry). Prevents post-reconnect history catch-up from starving operational messages. |

## Stages

| Stage | Deliverable | Prerequisites |
|---|---|---|
| **0** | **Prerequisites audit + gap-fill.** Confirm outbox (with separate history lane), `data-tsdb` crate (`TelemetryRepo` seam + SQLite/Timescale impls), and RSQL framework are production-grade or produce stage plans for the gaps. Owners assigned per component. Verify TimescaleDB licensing covers the features we plan to use (Apache-2 core only, or TSL features explicitly accepted). **If TSL features are blocked, define a vanilla-Postgres fallback** (partitioned tables + manual bucket management) so Stages 3–4 are not blocked on a licensing decision. | — |
| 1 | `SlotValue` union + `slots.kind` column + chunked migration | Stage 0 |
| 2 | `sys.core.history.config` kind with all three variants; validation; placement rules; **one-per-node default, multiple opt-in via platform setting** | 1 |
| 3 | Historizer service — subscribes to slot events, applies policy, **in-memory ring buffer with bulk flush (time/size/shutdown triggers, critical-tier bypass)**, bounded queue, back-pressure health events | 2 |
| 4 | SQLite and Timescale table impls behind the `TelemetryRepo` trait; edge quota enforcement; Binary blob handling | 3, `data-tsdb` crate (Stage 0) |
| 5 | REST + RSQL history query path; telemetry path integration | 4, RSQL framework (Stage 0) |
| 6 | CLI + Studio property-panel UI + chart widget | 5 |
| 7 | Flow nodes (`history.record`, `history.read-range`, `history.on-recorded`) | 5 |
| 8 | Edge → cloud sync over **dedicated history outbox lane**; retention profile + sample-cap enforcement | 4, outbox with lane support (Stage 0) |
| 9 | Soak test — 24 h on edge profile: memory flat, disk bounded, sync resumes after network drop, **record-write p99 < 500 ms** under realistic load, **buffered flush keeps up without unbounded queue growth**, **SIGKILL data-loss window matches documented `flush_interval`** | 1–8 |

## Open questions

- **Retention profile shape** — single `keep_for_days` on the config, or separate per-table-shape (time-series downsample tiers vs regular-table row cap)? Leaning single field with an optional override escape hatch.
- **History discovery UX** — dedicated "History" tab vs standard child-node listing under the parent?
- **User-authored history backends** — allow extensions to contribute new backends in v1, or defer to v2?
- **Edge compression/downsampling** — opt-in per config, or platform-wide policy? (SQLite has no native hypertable compression; we'd implement coarse-bucket rollups manually if needed.)
- **Critical-config designation** — now defined as `critical: true` in `HistoryConfig` settings (bypasses buffer, per-write commit, refuses rather than drops under back-pressure). Open question: should `critical` also be a queryable **facet** on the config node for RBAC/audit filtering, or is the settings flag enough?

## One-line summary

**Slots carry a unified `SlotValue`; historization is an `sys.core.history.config` child node — one per parent node, per-slot policy in its settings — with `cov | interval | on_demand` triggers, buffered bulk writes (5 s default flush, critical tier bypasses), platform-default sample caps overridable per slot, and a dedicated history lane on the outbox; one database per deployment (SQLite on edge, Postgres+Timescale on cloud), schema-split at the table level: `Bool`/`Number` in time-series tables, `String`/`Json`/`Binary` in a regular `slot_history` table.**
