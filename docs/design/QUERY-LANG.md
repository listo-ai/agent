# Query Language — Generic Framework

"Generic" is the keyword. A lot of platforms fail here: they build the query layer for their specific domain, then every new resource type requires bespoke code. This is the framework-level approach instead.

## What "generic" means here

**Any OLTP resource type — flows, devices, users, extensions, audit events, tenants, points — exposes the same query surface via the same mechanism, with zero per-resource custom parser/translator/endpoint code.**

**Time-series / telemetry is NOT on this path.** Telemetry lives in a dedicated TSDB (see README.md messaging backbone) and has its own query shape: time range + aggregation + downsample + group-by-tag. The generic RSQL framework here is for relational resources. Cramming time-series queries into RSQL would produce a slow, wrong answer to a different question.

Adding a new resource type = declaring its queryable schema. That's it. No new controllers, no new parsers, no new SQL builders.

## The generic query pipeline

One pipeline, used by every REST endpoint, every internal service, every extension:

```
Request string  →  Parser  →  AST  →  Validator  →  Translator  →  Query  →  Response
                                          ↑
                                 Resource Schema
                              (declared, not coded)
```

Each stage is generic. The only resource-specific thing is a **declarative schema** per resource type — no imperative code.

## The resource schema (the only thing that varies)

Every resource type declares a `QuerySchema` — a struct, not code:

```rust
// Conceptually — exact API TBD
QuerySchema {
    fields: {
        "id"         → Field { ty: Uuid,  ops: [Eq, Ne, In] },
        "name"       → Field { ty: Text,  ops: [Eq, Ne, Like] },
        "tenant_id"  → Field { ty: Uuid,  ops: [Eq], enforced_from: Auth },
        "created_at" → Field { ty: Time,  ops: [Lt, Le, Gt, Ge] },
        "tags"       → Field { ty: TextArr, ops: [Contains, In] },
    },
    default_sort: [("created_at", Desc)],
    max_page_size: 1000,
    allowed_expand: ["owner", "latest_reading"],
}
```

Declarative. Data, not code. Can be derived from a macro on the entity struct:

```rust
#[derive(Queryable)]
#[queryable(
    sort_default = "-created_at",
    max_page_size = 1000,
)]
struct Device {
    #[query(ops = "eq,ne,in")]
    id: Uuid,
    #[query(ops = "eq,ne,like")]
    name: String,
    #[query(ops = "eq", enforced_from = "auth.tenant_id")]
    tenant_id: Uuid,
    // ... etc
}
```

One derive. All REST query capability drops in automatically.

## The generic components

| Component | What it does | Why it's generic |
|---|---|---|
| **Parser** | Turns filter/sort/pagination strings into AST | Doesn't know what resource; only knows the grammar |
| **Validator** | Checks AST against a `QuerySchema` | Takes any schema; same validation rules for all |
| **Authorizer** | Injects forced predicates from the request's `AuthContext` (see [AUTH.md](AUTH.md) — `org_id`, `sub`, roles). `enforced_from = "auth.tenant_id"` binds the schema field to the verified JWT claim. | Same for every resource — the schema says which fields are auth-enforced, the auth layer owns the values |
| **Translator** | AST → SeaQuery `Condition` | Pure AST transformation; resource-agnostic |
| **Executor** | Runs translated query against the DB | SeaORM — already resource-generic |
| **Responder** | Serializes results + pagination metadata | Same wire format for every resource |

Add a new resource? Add a `QuerySchema`. The pipeline handles the rest.

## The generic REST shape

Every resource endpoint looks identical:

```
GET    /api/v1/{resource}                 # list with query params
GET    /api/v1/{resource}/{id}            # single
POST   /api/v1/{resource}                 # create
PATCH  /api/v1/{resource}/{id}            # update
DELETE /api/v1/{resource}/{id}            # delete
```

Query params follow RSQL + standard pagination:

```
?filter=name==Alice;created_at=gt=2026-01-01
&sort=name,-created_at
&page=2
&size=100
&select=id,name,status
&expand=owner
```

Same parameters, every resource. One handler, parameterized by `QuerySchema`. A controller for a new resource is roughly:

```rust
async fn list_devices(ctx: Context, q: Query) -> Response {
    query::list::<Device>(ctx, q).await
}
```

That's it. `query::list` is generic over any `Queryable` type.

## The generic response shape

```json
{
  "data": [ {...}, {...}, {...} ],
  "meta": {
    "total": 1523,
    "page": 2,
    "size": 100,
    "pages": 16
  },
  "links": {
    "self": "/api/v1/devices?page=2&size=100",
    "next": "/api/v1/devices?page=3&size=100",
    "prev": "/api/v1/devices?page=1&size=100"
  }
}
```

Same envelope for every resource. Clients write one response parser.

## OpenAPI comes along for free

Because `QuerySchema` is declarative, the OpenAPI generator reads it and produces the query-parameter documentation automatically. Every resource's endpoint shows up in the OpenAPI spec with its actual queryable fields, operators, and types. No hand-written docs, no drift.

## Generic beyond REST

Because the AST sits in the middle, other transports can consume the same schema:

| Surface | Input | Uses |
|---|---|---|
| REST API | URL query string → RSQL parser → AST | What we've been talking about |
| CLI | `yourapp device list --filter 'category==hvac'` | Same parser, same AST |
| gRPC | Structured filter message → AST | Typed filter protobuf, skips the parser |
| MCP tools | LLM-generated filter params → AST | Same validator and translator |
| NATS subjects | Subscription filters on events | AST compiled server-side into a JetStream consumer's filter expression; clients never evaluate AST locally. For Core NATS (no JetStream, e.g. edge leaf), fall back to subject-wildcard subscription + in-process AST evaluation on receipt. |
| Internal Rust | Builder API → AST directly | No string parsing at all |

One AST. Six transports. Every resource queryable the same way from every surface.

## What's NOT generic

Be honest about where generics end:

| Non-generic | Why |
|---|---|
| Per-resource business logic | A device has state machines; a user has password policies. These live in the resource handlers, not the query layer. |
| Complex aggregations | `SELECT avg(reading) GROUP BY device_id` — not every resource needs this, and the query layer isn't the right abstraction for it. Separate `/analytics` endpoints with their own contract. |
| Full-text search | Specialized indexes, different operators. If you need it, add a separate `search` parameter that bypasses the RSQL path. |
| Graph traversals | "Give me all points reachable from this AHU through `feedsRef` relationships." Not a filter. Needs its own endpoint with graph semantics. |
| Time-series telemetry | Separate TSDB, separate query shape (range + aggregation + downsample). Exposed via dedicated `/api/v1/telemetry/*` endpoints and the `query_telemetry` MCP tool — not through RSQL. |

Design the generic path to cover 90% of queries. Leave escape hatches for the 10% that don't fit — don't torture the grammar to cram them in.

## What a new resource actually costs

Adding a new resource to the framework:

1. Define the entity struct with `#[derive(Queryable)]` annotations — 10 lines
2. Register it with the REST router — 1 line
3. OpenAPI, CLI, SDK, docs — all auto-generated

Compare to a non-generic approach where each new resource needs a custom controller, custom query parsing, custom SQL, custom pagination, custom docs. That's hundreds of lines per resource and it drifts over time.

## Where this lives in the stack

A new crate, `/packages/query/`, owned by the framework:

```
/packages/query/
  src/
    ast.rs          # the AST types
    parser.rs       # RSQL parser (string → AST)
    schema.rs       # QuerySchema type
    validator.rs    # AST + schema → validated AST
    translator.rs   # validated AST → SeaQuery Condition
    rest.rs         # Axum extractor: request → Query
    cli.rs          # clap integration
    openapi.rs      # utoipa schema generator
```

Used by the Control Plane, the Edge Agent API, the CLI, the SDKs, extensions — one implementation, everywhere.

## Stack addition

- **Generic query framework** at `/packages/query/`
- **Grammar: RSQL** for strings, **typed AST** for programmatic use
- **Backend: SeaQuery** (already chosen via SeaORM) — dialect-portable to SQLite and Postgres
- **Schema: declarative**, derived from entity structs via `#[derive(Queryable)]`
- **Surfaces: REST, gRPC, CLI, MCP, NATS, internal Rust** — all go through the same AST

## One-line summary

**One declarative query schema per resource, one parser, one validator, one translator — every REST endpoint, CLI command, SDK call, and MCP tool inherits filtering, sorting, pagination, and OpenAPI docs automatically, without per-resource code.**

Want me to sketch the actual `Queryable` derive macro shape, or the `QuerySchema` type in more detail?