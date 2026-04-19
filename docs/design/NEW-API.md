# Adding a new REST endpoint — developer checklist

When you add a handler in [`crates/transport-rest`](../../crates/transport-rest), it's not done until **three** client surfaces match: the Rust client, the TS client, and the CLI. This doc is the ordered checklist — no hunting through code, no forgetting the fixture gate.

**Ship rule:** a PR that introduces a REST endpoint without updating all three clients is incomplete. Reviewers send it back.

---

## When this applies

- You add a new route in [`crates/transport-rest/src/*.rs`](../../crates/transport-rest/src/) (e.g. `crates/transport-rest/src/kinds.rs`).
- You add a new response field or change an existing one on an existing route.
- You add / change / remove a query parameter or request-body field.
- You add a new error code or HTTP status to an existing route.

If all you're doing is changing an internal detail that doesn't leak through the wire (refactor, perf, logging), skip this doc.

## Why the overhead is worth it

The Rust client is the same crate the **CLI** consumes. The TS client is the same package the **Studio frontend** consumes. Anything you ship in REST that isn't reflected in both clients is silently unreachable by the two primary users of the platform. The fixture gate is the mechanical guarantee that REST ↔ client shape never drifts — breaking it in CI is cheaper than discovering it in production.

---

## The five touchpoints, in order

### 1. REST handler — the source of truth

| File | What |
|---|---|
| `crates/transport-rest/src/<feature>.rs` | Handler + DTOs (`#[derive(Serialize)]`, `#[derive(Deserialize)]`) |
| `crates/transport-rest/src/routes.rs` | Register the route under `/api/v1/…` via `.merge(crate::<feature>::routes())` |
| `crates/transport-rest/src/lib.rs` | `pub mod <feature>;` |

**Rules for the wire shape** (these are the contract the clients mirror):

- Paths versioned via `/api/v1/`. Bumping to `/api/v2/` needs a 12-month window per [VERSIONING.md](VERSIONING.md). Don't.
- Response DTOs use serde's default field order — **stable**, never re-ordered alphabetically.
- Error shape is `{"error": "…"}` with the HTTP status carrying the code class (400 user error, 404 not found, 500 infra). Don't invent `{"message", "reason", "cause"}` — use the existing [`ApiError`](../../crates/transport-rest/src/routes.rs).
- Query params deserialise via `#[derive(Deserialize)]` on a struct with `#[serde(default)]` on optionals. Unknown query param → 400 with the field name in the error (serde's default behaviour).
- Timestamps are RFC 3339. Durations in `_ms` suffix unless the field name says otherwise.
- Booleans are real booleans, not `"true" | "false"` strings.

**Tests:** unit tests for the handler next to it (`#[cfg(test)] mod tests`). One happy path, one failure case per error code you introduced.

### 2. Rust client — [`clients/rs/`](../../clients/rs/)

The CLI depends on this crate. No CLI change compiles until this is done.

| File | What |
|---|---|
| `clients/rs/src/<feature>.rs` | One struct per "noun" (`Kinds`, `Plugins`, `Nodes`) holding `&HttpClient` + base path. One `async fn` per endpoint, returning `Result<T, ClientError>` where `T` is a DTO from `types.rs`. |
| `clients/rs/src/types.rs` | DTOs mirroring the REST handler's serde shape. Derive `Serialize + Deserialize` + `Debug + Clone`. Field names **exact** match (use `#[serde(rename = "…")]` only if the server DTO also uses it). |
| `clients/rs/src/lib.rs` | `pub mod <feature>;` + an accessor method on `AgentClient` (see existing `fn kinds()` / `fn plugins()`). |

**Rules:**

- The Rust client never duplicates logic — handlers ↔ clients ↔ CLI all agree on one DTO shape. If you rename a field in the REST handler, you rename it in `types.rs` in the same commit.
- Query string building: URL-encode values. Look at [`clients/rs/src/kinds.rs:46`](../../clients/rs/src/kinds.rs#L46) for the existing helper — reuse / extend it, don't reinvent.
- Error mapping: HTTP non-2xx → `ClientError::Http { status, message }`. Let `HttpClient::get` / `post` handle this; never parse bodies manually in the feature module.
- Every new method gets a doc comment with a one-line example. The `agent schema` subcommand reads these eventually.

### 3. TS client — [`clients/ts/`](../../clients/ts/)

Studio depends on this package.

| File | What |
|---|---|
| `clients/ts/src/schemas/<feature>.ts` | Zod schemas for every DTO. Field-for-field match with the Rust DTO. Missing zod schema = runtime parse failure in Studio. |
| `clients/ts/src/domain/<feature>.ts` | `<Feature>Api` interface + `create<Feature>Api(http, apiVersion)` factory. One method per endpoint. Returns zod-parsed values (not raw JSON). |
| `clients/ts/src/client.ts` | Import the factory, expose it as `readonly <feature>: <Feature>Api` on `AgentClient`. |
| `clients/ts/src/index.ts` | Re-export the schema types. |

**Rules:**

- Every response must round-trip through `<Schema>.parse(raw)`. No `as` casts on the wire boundary — zod is the validation layer.
- Enum values are camelCase strings matching the Rust serde `rename_all = "camelCase"` (e.g. `Facet` → `"isProtocol"` not `"IsProtocol"`).
- `null` stays `null`. Don't `??` it to undefined in transport — that's a Studio concern.
- Branded types (`KindId`, `NodePath` etc.) live in `schemas/` too if already established; reuse them.

### 4. CLI — [`crates/transport-cli/`](../../crates/transport-cli/)

| File | What |
|---|---|
| `crates/transport-cli/src/commands/<feature>.rs` | `#[derive(Subcommand)]` enum + `run(client, fmt, cmd)` async fn. One variant per endpoint method. |
| `crates/transport-cli/src/commands/mod.rs` | Register the module. |
| `crates/transport-cli/src/lib.rs` | Add the variant to the top-level `Command` enum + dispatch arm. |
| `crates/transport-cli/src/commands/meta.rs` | One `CommandMeta` entry per subcommand for `--help-json` + `agent schema` output. Mandatory — see [CLI.md § "LLM-friendly surface"](CLI.md). |

**Rules:**

- Table output uses `output::ok_table` with explicit `&[&str]` headers. Column order stable.
- JSON output uses `output::ok`, `output::ok_msg`, or `output::ok_status` — **never** construct JSON in a command body. `ok_msg` for "success returns a value the caller wants" (`slots write` → generation). `ok_status` for confirmation-only (`config set`, `lifecycle`). Re-read [output.rs:193-197](../../crates/transport-cli/src/output.rs) if unsure.
- Error handling: `?` on the `Result<…, ClientError>` — `main.rs` routes it through `CliError::from_client` with stable exit codes. Don't `anyhow::bail!` with a formatted string.
- Every subcommand has an example in its `CommandMeta`. LLM consumers read these.
- Exit codes are stable (`EXIT_SUCCESS` / `EXIT_USER_ERROR` / `EXIT_INFRA_ERROR` / `EXIT_INTERNAL_ERROR`). Never return raw integers.

### 5. Contract fixtures — the mechanical guarantee

| File | What |
|---|---|
| `clients/contracts/fixtures/cli-output/<command>/<scenario>.json` | Pinned JSON for every command × scenario (happy, each error). Used by `crates/transport-cli/tests/fixture_gate.rs`. |
| `crates/spi/tests/contract_fixtures_*.rs` | Wire-shape round-trips for any new `Msg`/`GraphEvent`/core DTO variants. |

**Rules:**

- One fixture per JSON-observable scenario you added. Empty list, populated list, not-found, bad input, precondition failed, infrastructure error.
- Fixtures support two wildcards (see `assert_shape_match` in `fixture_gate.rs`):
  - `"VARIES"` — any string (for UUIDs, timestamps, generated paths).
  - `null` — any value (used when a number is non-deterministic, e.g. `generation`).
- Adding a new command without fixtures **fails CI on the coverage check** in `fixture_gate.rs` (see the `every_variant_has_a_fixture` pattern from [STEPS.md Stage 3a-4](../sessions/STEPS.md)). If it doesn't fail today, add the coverage guard as part of this PR.

---

## Checklist (copy into your PR description)

```markdown
- [ ] REST handler + DTOs in `crates/transport-rest/src/<feature>.rs`
- [ ] Route registered in `routes.rs` + module exported in `lib.rs`
- [ ] Unit tests for handler (happy + each error code)
- [ ] Rust client method in `clients/rs/src/<feature>.rs`
- [ ] DTO in `clients/rs/src/types.rs` (or reused)
- [ ] Accessor wired on `AgentClient` in `clients/rs/src/lib.rs`
- [ ] TS zod schema in `clients/ts/src/schemas/<feature>.ts`
- [ ] TS API in `clients/ts/src/domain/<feature>.ts`
- [ ] TS client accessor in `client.ts` + re-export in `index.ts`
- [ ] CLI subcommand in `crates/transport-cli/src/commands/<feature>.rs`
- [ ] CLI `CommandMeta` entry in `commands/meta.rs` (for `--help-json` / `agent schema`)
- [ ] Fixture(s) in `clients/contracts/fixtures/cli-output/<command>/`
- [ ] `cargo fmt --all --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` all green
- [ ] Smoke test pasted in PR description: `cargo run -p agent` + each new `agent <feature> <verb>` command
```

---

## Anti-patterns — these fail review

| Anti-pattern | Why it's rejected |
|---|---|
| "TS client will come later" | Studio users silently can't use your feature until someone else circles back. Ship together. |
| CLI uses `reqwest` directly, bypassing `agent-client` | Duplicates logic. Every endpoint call path must go through `clients/rs`. |
| Hand-writing JSON in a CLI command | Breaks the deterministic-output contract in [CLI.md § "LLM-friendly surface"](CLI.md). Use `output::ok*`. |
| New DTO in `types.rs` with fields out of order vs REST DTO | Subtle serde drift; fixture tests pass because field order doesn't affect equality, but downstream consumers depending on stable order break silently. Mirror exactly. |
| TS schema using `z.any()` or `.passthrough()` for unknown fields | Future additions escape validation. Use `z.object({...}).strict()` (or `.catchall()` only when the REST side is explicitly open-ended, which is rare). |
| Error shape that isn't `{error: string}` | Breaks `CliError::classify_http`'s downstream parser. If you need structured error details, extend `ApiError` once, not per-endpoint. |
| Skipping fixtures "because the test passes" | The test passes because there's no coverage gate yet — the gate lands with the next endpoint. Be that next endpoint. |

---

## Worked example — the `kinds` endpoint landing

Concrete reference to copy (the `GET /api/v1/kinds` endpoint):

- Handler: [`crates/transport-rest/src/kinds.rs`](../../crates/transport-rest/src/kinds.rs)
- Route registration: `.merge(crate::kinds::routes())` in [`routes.rs`](../../crates/transport-rest/src/routes.rs)
- Rust client: [`clients/rs/src/kinds.rs`](../../clients/rs/src/kinds.rs) + `KindDto` in [`types.rs`](../../clients/rs/src/types.rs)
- TS client: [`clients/ts/src/schemas/kind.ts`](../../clients/ts/src/schemas/kind.ts) + [`clients/ts/src/domain/kinds.ts`](../../clients/ts/src/domain/kinds.ts)
- CLI: [`crates/transport-cli/src/commands/kinds.rs`](../../crates/transport-cli/src/commands/kinds.rs)

Use it as the template for the next endpoint.

---

## Decisions locked

1. **One PR, five touchpoints.** No "split the clients into a follow-up". Reviewers enforce this.
2. **DTOs are mirrored exactly**, not generated. Field order, names, types match. schemars-driven generation is a post-Stage-9 concern; until then, mirroring is manual and checked at review.
3. **Clients never talk to the agent via raw reqwest / fetch.** `agent-client` (Rust) and `@acme/agent-client` (TS) are the only call paths. CLI, Studio, tests, extensions — all of them.
4. **Fixtures are mandatory**, one per observable scenario. The fixture gate catches shape drift in CI.
5. **Errors have one shape** (`{error: string}` on the wire, stable `code` enum in the CLI). No per-endpoint exception.
6. **CLI output never constructs JSON directly** — always funnels through `output::ok*` helpers for the deterministic-output contract.

---

## One-line summary

**Every new REST endpoint lands in one PR that touches five places — handler, Rust client, TS client, CLI, fixtures — so Studio users, CLI users, and LLM-driven automation all see the surface the same day.**
