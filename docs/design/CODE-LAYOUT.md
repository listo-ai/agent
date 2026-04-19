# Engineering Guidelines — Code Structure & Separation of Concerns

A document for humans and AI assistants contributing to the codebase. Read this before writing code. These rules are not suggestions.

## The first rule: small files

**Hard limits, enforced in code review:**

| Limit | Value | Rationale |
|---|---|---|
| Max lines per file | **400** | Anything larger is multiple responsibilities pretending to be one |
| Max lines per function | **50** | If it needs more, it's doing more than one thing |
| Max public items per module | **~10** | A module with 30 pub items is a grab-bag, not a module |
| Max depth of nesting | **4** | Deeper means extract a function |

When a file approaches 300 lines, stop and ask: *what are the responsibilities here?* Split before you hit 400, not after.

These limits apply equally to code written by humans and code written by AI assistants. An AI that generates a 1,200-line "complete solution" is producing a liability, not an asset. Break it up or reject it.

## The second rule: separation of concerns is mandatory

Three layers. They **never** mix. Code that mixes them will not be merged.

| Layer | Responsibility | What it knows about |
|---|---|---|
| **Data layer** | Talking to the database | SQL, SeaORM, migrations, entity mapping |
| **Domain layer** | Business logic, rules, orchestration | Types, traits, operations on data — NO database, NO HTTP |
| **Transport layer** | REST, gRPC, NATS, CLI — however the outside world reaches us | Routing, serialization, authentication — NO business logic, NO SQL |

The compiler cannot enforce this. We enforce it in code review. If you're tempted to write SQL in a handler, stop. If you're tempted to write HTTP response logic in a domain function, stop.

## The dependency direction

One rule:

```
transport  →  domain  →  data
```

Never the other way. Data never imports domain. Domain never imports transport. This is how you keep things testable and reusable.

If you find yourself wanting to violate this — the usual reason is "it's simpler" — you're feeling the friction that *forces* good design. The friction is the point.

## Crate structure

The repo is a Cargo workspace. Each crate has exactly one job. **This layout supersedes the coarser `/packages/...` sketch in README.md** — the README was written before this doc.

```
/crates
  /spi                      # Cross-cutting contracts — protobuf, JSON schemas, trait definitions
  /query                    # Generic query framework (RSQL → AST → SeaQuery)
  /auth                     # JWT verification, JWKS caching, AuthContext, RBAC types, revocation deny-list client
  /messaging                # NATS client, subject taxonomy, outbox pattern
  /audit                    # Audit event types and emission
  /observability            # Tracing, metrics, structured log formatting (renamed from "telemetry" — not storage)
  /config                   # Config loading, env precedence

  /data                     # Data layer
    /data-entities          #   Shared entity structs + column enums (logical shape)
    /data-repos             #   Repository trait definitions (not impls)
    /data-sqlite            #   SQLite-native impls, migrations, queries (edge + standalone)
    /data-postgres          #   Postgres-native impls, migrations, queries (cloud)
    /data-tsdb              #   Time-series store — a seam, not a database. Holds the `TelemetryRepo` trait + two impls:
                            #     • edge impl: rolling SQLite tables with time bucketing and retention (no TSDB extension available)
                            #     • cloud impl: TimescaleDB (Postgres extension) — hypertables, continuous aggregates, compression, retention policies
                            #   Shares a Postgres *instance* with data-postgres in cloud deployments, but the code stays separate:
                            #   time-series access patterns (range + bucket + downsample + group-by-tag) don't belong next to OLTP queries.

  /domain                   # Domain layer — pure business logic. Each crate is in practice a node-kind
                            # registration + rules: it declares its kinds, facets, containment schema,
                            # slot schemas, and business operations on them. Registers them with the graph crate.
    /domain-flows           #   `acme.core.flow` kind — flow lifecycle, validation, versioning
    /domain-devices         #   device kinds — registration, state machine, commissioning rules
    /domain-extensions      #   extension kinds — manifest, capabilities, lifecycle
    /domain-fleet           #   rollouts, deployments, targeting (operates on graph subtrees)

  /transport                # Transport layer — external surfaces
    /transport-rest         #   Axum handlers, OpenAPI schemas (utoipa)
    /transport-grpc         #   gRPC services, generated from protobuf
    /transport-nats         #   NATS subject handlers
    /transport-cli          #   clap subcommands
    /transport-mcp          #   MCP server adapter (feature-gated, off by default)

  /graph                    # THE CORE. Implements the "everything is a node" model (see EVERYTHING-AS-NODE.md).
                            #   Node trait, SlotMap, Link, containment schema, facet registry, lifecycle machine,
                            #   event bus, placement enforcement, cascading delete. Every domain crate registers
                            #   its node kinds here. Persistence goes through data-repos. Not a layer — it's the
                            #   substrate every other layer sits on.

  /engine                   # Flow engine — crossflow integration, live-wire executor, node runtime, extension supervisor composition.
                            # Not a layer — a vertical. Consumes graph, domain traits, messaging, extensions-host, auth.

  /extensions-sdk           # SDK for Rust extension authors (public API — stricter semver)
  /extensions-host          # Extension process supervisor, IPC over UDS gRPC, cgroup limits

  /apps
    /agent                  # The single binary — ties everything together. Role selected at runtime (edge/cloud/standalone).
    /studio                 # Tauri + React app (JS/TS, separate pnpm workspace, not a Cargo crate)
```

**Rule:** an app crate (`agent`) is a thin composition — it wires the other crates together and runs. Almost no logic of its own. If there's logic in `apps/agent/src/`, it belongs in a library crate.

**Feature flags on `apps/agent`** strip role-irrelevant code from the compiled binary:

| Feature | Pulls in | Default for |
|---|---|---|
| `role-edge` | `data-sqlite`, `data-tsdb` (SQLite shape), NATS leaf | Edge targets |
| `role-cloud` | `data-postgres`, `data-tsdb` (TSDB shape), NATS cluster client, fleet orchestrator | Cloud targets |
| `role-standalone` | Both of the above | Dev / appliance |
| `mcp` | `transport-mcp` | Off unless explicitly enabled |

One source tree, targeted binaries. Edge binaries do not link Postgres; cloud binaries do not link the edge-specific SQLite paths.

## Why so many small crates

| Benefit | Why it matters |
|---|---|
| Parallel compilation | Cargo builds independent crates in parallel; big crates serialize |
| Explicit boundaries | You can't import from a crate you didn't declare as a dependency |
| Reusability | `auth`, `messaging`, `query` — all usable in extensions, tools, future products |
| Testing | Smaller crates = smaller test surfaces = faster test cycles |
| AI comprehension | An AI assistant given a focused crate produces focused changes; given a 50k-line monolith produces chaos |

## Naming conventions

| Pattern | Example | Use for |
|---|---|---|
| Noun | `Flow`, `Device`, `Extension` | Entity types, value objects |
| Noun + trait-y ending | `FlowRepository`, `MessageBus` | Traits |
| Verb noun | `DeployFlow`, `RotateCredential` | Commands / domain operations |
| Verbed noun | `FlowDeployed`, `DeviceRegistered` | Events / past-tense |
| `-Error` suffix | `FlowValidationError` | Error enums |

No `Helper`, `Util`, `Manager`, `Handler`, `Service` as primary names. These are placeholders that signal the author didn't know what the thing actually does. Be specific.

## Public API discipline

Every crate has:

| File | Purpose |
|---|---|
| `src/lib.rs` | Re-exports the crate's public API. Nothing else. |
| `src/error.rs` | The crate's public `Error` type, constructed from internal errors |
| `src/<module>.rs` | Focused modules, each under 400 lines |

**If a type isn't in `lib.rs`, it's private.** Resist the urge to `pub use` everything. The surface area of a crate's public API is a debt — every exported type is a promise to not break it.

## Traits first, implementations second

When designing a module, write the trait before the implementation.

```rust
// domain/src/flows.rs — the interface
pub trait FlowRepository {
    async fn get(&self, id: FlowId) -> Result<Flow, RepoError>;
    async fn save(&self, flow: &Flow) -> Result<(), RepoError>;
    async fn list(&self, query: FlowQuery) -> Result<Vec<Flow>, RepoError>;
}

// data/src/flows.rs — the implementation
pub struct SeaOrmFlowRepository { /* ... */ }
impl FlowRepository for SeaOrmFlowRepository { /* ... */ }
```

Why: domain code depends on the trait, not the impl. Tests substitute a mock. Adding Postgres vs SQLite vs in-memory is swapping the impl. The dependency arrow stays pointed the right way.

## Error handling

| Rule | Reason |
|---|---|
| Every crate has its own `Error` enum | Errors describe what *this* crate failed at, not implementation details |
| Use `thiserror` for library crates | Derives, transparent conversions, `#[from]` |
| Use `anyhow` only in the binary (`apps/agent`) | Terminal — the place where typed errors become human messages |
| No `unwrap()` or `panic!()` in library code | Except in explicitly-documented invariant violations |
| Errors carry context, not just messages | Attach IDs, tenant, operation — not just "not found" |

## Testing discipline

| Test type | Where it lives | When to write it |
|---|---|---|
| Unit tests | Same file as the code, in `#[cfg(test)] mod tests` | For every public function with logic |
| Integration tests | `tests/` directory in each crate | Crate-level behavior across modules |
| End-to-end tests | `/e2e/` workspace member | Full stack via public API |
| Property tests | Via `proptest` | Anywhere there's a law to check (parsers, serializers) |

**Rule:** a change to domain logic must have a test. A change that's purely refactoring doesn't need new tests but must not break existing ones.

No sharing of test helpers across crates via `pub use`. If a helper is useful to multiple crates, it goes in a `testing` crate depended on only in `dev-dependencies`.

## Database code stays in `data/`

**What belongs in `data/`:**
- SeaORM entity definitions (`data-entities`)
- Repository trait *definitions* (`data-repos`) — domain imports these
- Repository trait *implementations* per backend (`data-sqlite`, `data-postgres`)
- Migrations — **separate sets per backend**. SQLite uses TEXT/INTEGER; Postgres uses UUID/TIMESTAMPTZ/JSONB + partitioning + RLS. Logical shape is shared; physical DDL is not.
- Query construction from generic AST (the `query` crate parses; `data-sqlite` / `data-postgres` each translate and execute)
- Time-series store impls (`data-tsdb`), which speak a different trait from OLTP repos

**What does NOT belong in `data/`:**
- Business rules ("a flow can only be deployed if it's validated" — that's domain)
- HTTP status codes
- JSON serialization for API responses
- Logging other than DB-specific (slow queries, migration progress)

A domain function that wants to fetch a flow calls `repo.get(id)`. It doesn't know SeaORM exists, nor whether the backend is SQLite or Postgres. If we swap SeaORM for Diesel tomorrow, `domain/` does not change. That's the test.

**Testing parity:** the shared repository test suite (defined in `data-repos` or a `testing` fixture crate) runs against **both** backend impls in CI. A query that works on one and fails on the other is a bug caught at the seam, not in production.

## REST code stays in `transport-rest/`

**What belongs in `transport-rest/`:**
- Axum handler functions
- Request/response DTOs (separate from domain types)
- OpenAPI annotations (`utoipa`)
- Middleware (auth extraction, rate limiting, CORS)
- Serialization concerns

**What does NOT belong in `transport-rest/`:**
- Business logic
- Database calls
- Direct use of SeaORM types

A handler does exactly four things, in this order:

1. Extract inputs (path, query, body, auth context)
2. Call one or more domain functions
3. Translate the result to a response DTO
4. Return the response

If a handler is more than ~20 lines, it's probably doing domain logic. Move it.

## Logic stays in `domain/`

The domain layer is where the product lives. It's pure Rust, framework-free, no `axum`, no `sea_orm`, no `tonic`, no `tokio` dependencies beyond `async-trait`.

**What belongs in `domain/`:**
- Types (`Flow`, `Device`, `Deployment`)
- Operations on those types (`Flow::validate`, `Deployment::roll_out`)
- Traits describing what the domain needs from the outside world (`FlowRepository`, `MessageBus`, `Clock`)
- Business errors (`FlowValidationError::CycleDetected`)

**A domain function's signature never mentions:**
- An HTTP request/response
- A database connection
- A specific log format
- A NATS subject
- A file path

If it does, the dependency arrow is pointed the wrong way.

## Shared libraries — the common crates

Some concerns are genuinely cross-cutting. These live in small, focused library crates usable by:
- The agent (cloud + edge roles)
- Extensions (Rust SDK)
- Internal tools and tests
- Future products built on the same foundation

| Crate | What it owns | Used by |
|---|---|---|
| `spi` | Protobuf definitions, JSON schemas, trait signatures for extensions | Everyone |
| `query` | Generic RSQL → AST → SeaQuery pipeline + `AuthContext` binding for enforced predicates | Transport, extensions, CLI |
| `auth` | JWT verification, JWKS cache (24h stale ceiling), `AuthContext` type, RBAC checks, revocation deny-list consumer | Transport, extensions |
| `messaging` | NATS client wrapper, tenant-scoped subject taxonomy, outbox pattern with bounded disk + backpressure signals | Transport, domain events |
| `audit` | Audit event types, emission trait | Every layer that makes decisions |
| `observability` | Tracing setup, metrics registration, structured log formatting. **Not storage** — don't confuse with `data-tsdb`. | Every binary at startup |
| `config` | Config loader with precedence rules, YAML parser | Every binary at startup |
| `testing` | Test fixtures, mock traits, in-memory implementations, shared repo test suite run against SQLite and Postgres | Every crate's `dev-dependencies` only |

Rules for shared crates:

1. **Minimal dependencies.** A shared crate with 50 deps is not shared, it's a monolith in disguise.
2. **No opinions about your framework choice.** `auth` doesn't know Axum exists. It provides types Axum handlers can use.
3. **Semver-disciplined.** Breaking changes to a shared crate ripple everywhere. Discuss before making them.
4. **Documented invariants.** Each crate's `lib.rs` has a doc comment explaining what it guarantees and what it assumes about its caller.

## Plugin / extension SDK philosophy

The extension SDK (`extensions-sdk`) is a public API for third parties. Different rules apply:

| Rule | Reason |
|---|---|
| **Never break it.** | Third-party extensions depend on this. Additions only. |
| **Re-export selectively.** | Extensions shouldn't see our internals; only what they need. |
| **Provide builders, not constructors.** | Builders are extensible; constructors break on new fields. |
| **Default implementations for trait methods where possible.** | Adding a method is non-breaking if there's a default. |
| **Version the SDK independently.** | It has its own release cadence tied to the SPI. |
| **Example crates in the repo.** | `/examples/extension-hello-world/` — tested in CI, always works. |

## What goes in which crate — quick heuristic

When you're writing something and wondering where it belongs, ask:

| Question | Answer points to |
|---|---|
| "If I swapped the database, would this change?" | Yes → `data/` · No → `domain/` |
| "If I swapped REST for gRPC, would this change?" | Yes → `transport/` · No → `domain/` |
| "Is this logic specific to one resource?" | Yes → `domain-<resource>/` · No → shared crate |
| "Would an extension author need this?" | Yes → `extensions-sdk` or a shared crate · No → internal |
| "Is this about a specific HTTP status code or JSON shape?" | Yes → `transport-rest/` |
| "Is this about a specific SQL query?" | Yes → `data/` |
| "Is this a pure function of inputs?" | `domain/` or a utility crate |

## Anti-patterns — do not do these

| Anti-pattern | Why it's bad | What to do instead |
|---|---|---|
| `fn handler(db: DbConn, req: Request) -> Response { /* SQL + logic + JSON */ }` | Mixes all three layers | Split into handler → domain → repo |
| A single `lib.rs` file that's 2000 lines | No separation, slow to compile, impossible to navigate | Split into focused modules, each under 400 lines |
| `pub mod utils` | Nothing lives in `utils` — it's where things go to be forgotten | Name the thing. `pub mod time_parsing`, `pub mod retry`. |
| `impl FromRequest for Flow` | Makes `Flow` (domain) depend on Axum (transport) | Create `FlowDto` in transport, convert to `Flow` in the handler |
| Wide public API with `pub use crate::*` | Every type is now a backward-compatibility promise | Re-export intentionally from `lib.rs` |
| Domain code using `tokio::fs`, `reqwest` directly | Tight coupling to runtime/framework | Inject a trait; impl lives in a transport or data crate |
| Domain code using `serde_json::Value` for what should be a typed struct | Hides the schema; makes refactors risky | Use a typed struct. **Exception:** flow-document node config is legitimately untyped at the domain boundary (each node type has its own schema); `serde_json::Value` is acceptable there. |
| An AI-generated 1200-line "god file" | Unreviewable, untestable, unmaintainable | Reject. Ask for split files up front. |
| Skipping tests because "it's simple" | Simple things break too, and without tests regressions are free | Write the test |
| Reaching for `unsafe` to solve a borrow-checker fight | Almost always the wrong answer | Refactor; ask for help |

## For AI assistants working on this codebase

Read this carefully. Following these rules is not optional.

| Rule | Why it matters for AI contributions |
|---|---|
| **Never create a file over 400 lines.** | AI tends to produce monoliths; we enforce splits from the start |
| **Never mix layers in a single change.** | A PR that touches SQL, domain, and HTTP in one file will be rejected |
| **When asked to add a feature, start with the trait.** | Show the interface before the implementation |
| **When asked to refactor, preserve the dependency direction.** | Don't accidentally make `domain/` depend on `transport/` |
| **When unsure where code belongs, ask.** | Don't guess; the cost of misplaced code compounds |
| **When generating a new crate, justify its existence.** | Does it have a single responsibility? Is it reusable? Or is it a dumping ground? |
| **Always include tests.** | A change without tests is incomplete |
| **Re-read this document before big changes.** | Patterns drift when forgotten |

If you're about to write a function that does more than one thing — stop. Write the pieces separately. It will feel slower; it is not slower. The PR will merge faster, the code will last longer, and the next change will cost less.

## Code review checklist

Before merging any PR, the reviewer verifies:

- [ ] No file exceeds 400 lines
- [ ] No function exceeds 50 lines
- [ ] Each modified crate still has a single, clear responsibility
- [ ] Layers are not mixed — no SQL in transport, no HTTP in domain
- [ ] Dependency direction is preserved — `transport → domain → data`
- [ ] Public API in `lib.rs` is intentional, not accidental re-exports
- [ ] New code has tests
- [ ] Error types are specific to their crate
- [ ] No `unwrap()` or `panic!()` in library code
- [ ] No `TODO` or `FIXME` without an issue number
- [ ] No `pub mod utils` or similar dumping grounds
- [ ] Shared logic that could be reused lives in a shared crate
- [ ] If the change touches the extension SDK, semver rules are respected

A PR that fails any of these is sent back. No exceptions, no "we'll fix it later."

## Closing

These rules exist because the alternative is a codebase that works today and is unmaintainable in two years. We've seen both outcomes in this industry; we know which one is cheaper.

Follow the rules. Small files. Clear layers. Reusable libraries. One responsibility per crate. Tests that prove the domain is correct independent of the transport.

The codebase pays you back every time you touch it.