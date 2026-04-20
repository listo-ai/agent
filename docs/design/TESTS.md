# Test-Driven Development

How we test. Opinionated, practical, and specific to this codebase. Not a general TDD primer — the point is that TDD here means *these* patterns, against *these* primitives, in *these* places.

Pairs with [CODE-LAYOUT.md](CODE-LAYOUT.md) (where tests live) and [NEW-SESSION.md](NEW-SESSION.md) (the ground rules for every session).

## Why TDD for this codebase

This is a platform with a lot of contract surfaces — `Msg`, node manifests, slot schemas, capability matching, flow document format, graph placement rules, the log field contract. Contracts silently break when tests don't pin them down. TDD forces the contract to be the thing you write down first.

The other reason is mechanical: a typical change here crosses trait boundaries (domain ↔ data, engine ↔ graph, SDK ↔ host). Writing a failing test first forces you to specify the boundary in data shapes, not in prose. Once the test compiles and fails, the rest is mostly typing.

## The rule

**Tests arrive with the code. Never after.** A PR that adds logic without tests — even "simple" logic — gets rejected.

Strict red → green → refactor is the ideal loop, but the actual minimum is:

1. The test exists in the same PR as the code that makes it pass.
2. The test fails if you revert the code change. (If removing the implementation leaves the test passing, the test isn't testing the implementation.)
3. The test reads like a specification of intended behaviour, not a restatement of the implementation.

A change to domain logic must have a test. Pure refactors (no behaviour change) don't need new tests but must not break existing ones.

## Test categories — where each lives, what each is for

| Category | Home | Purpose | Example |
|---|---|---|---|
| **Unit** | Same file as the code, in `#[cfg(test)] mod tests` | Behaviour of one function or one type in isolation | `apply_step()` in `count.rs` — step/bound/wrap arithmetic |
| **Integration** | `crates/<crate>/tests/*.rs` | Behaviour of the crate through its public API, composed across internal modules | `graph/tests/persistence.rs` — store + repo + restore |
| **Contract** | `crates/<crate>/tests/contract.rs` + committed fixtures | Freeze a wire shape or JSON format so downstream consumers can trust it | `spi/tests/fixtures/msg/*.json` — Msg round-trip |
| **Trait-suite** | `crates/<trait-crate>/src/testing.rs` behind a `testing` feature | Acceptance tests every implementation of a trait must pass | `data-repos` `GraphRepo` suite run by `data-sqlite` and (later) `data-postgres` |
| **Property** | `crates/<crate>/tests/prop.rs` | Invariants that must hold for *any* valid input | Msg JSON serialise → parse round-trips for any payload |
| **Snapshot** | `tests/snapshots/` next to the integration test | Detect unintended changes to stable output (manifests, generated code, CLI help text) | Pre-migration `KindManifest` snapshots before `#[derive(NodeKind)]` landed |
| **End-to-end** | `/e2e/` workspace member (not yet in-repo) | Full stack via public API, minimal mocking | Agent boot → create scope → AI runner fires → PR opens |

Rules for choosing:

- If it's purely a function on types you control, start unit.
- If it crosses modules or needs a real DB/file/socket, integration.
- If the output is consumed by another crate or an external system, contract.
- If multiple impls exist behind a trait, trait-suite first so every impl runs the same acceptance.
- If a parser, serializer, or validator exists, add property tests for it.
- If the output is generated (codegen, proc-macro, schema render), snapshot test it.

## Writing a good test — the shape

A good test in this codebase has four parts, in this order:

```rust
#[test]
fn deleting_a_device_cascades_to_points_and_fires_link_broken() {
    //  ── Arrange ──
    let (graph, sink) = setup_with_demo_kinds();
    let dev  = graph.create_child(station_path(), "demo-device", KindId::new("sys.driver.demo.device"))?;
    let pt   = graph.create_child(&dev, "temp-1", KindId::new("sys.driver.demo.device.point"))?;
    let link = graph.add_link(slot_ref(&pt, "value"), slot_ref(&sink, "in"))?;

    //  ── Act ──
    graph.delete(&dev)?;

    //  ── Assert ──
    assert!(!graph.exists(&dev));
    assert!(!graph.exists(&pt), "child point should have been cascaded");
    assert_link_broken_emitted(&events, link);
}
```

1. **Name** reads as a sentence about behaviour. `deleting_a_device_cascades_to_points_and_fires_link_broken`, not `test_delete_2`. When a test fails, the name alone should tell you what regressed.
2. **Arrange** builds the minimum world needed. Use shared fixture helpers for anything that recurs; reject copy-paste setup.
3. **Act** is one operation. If it's two operations, it's two tests or the operation has grown.
4. **Assert** checks observable outcomes, not internal state. `graph.exists(&pt)` is observable. `graph.internal_tree.nodes.len()` is not.

### What to assert

- **Positive** outcome of the happy path.
- **Negative** outcome of rejected input (the error variant, not just "it errored").
- **Invariants** the operation is supposed to preserve (tree still consistent, no dangling links, no memory growth beyond bounds).
- **Side-effect** events when they're part of the contract (events emitted, slots updated, audit entries written).

### What NOT to assert

- Implementation detail. If the test fails because you renamed a private field, the test was testing the wrong thing.
- Panic messages. Assert the error variant; the message can change.
- Timing, unless the test's whole point is timing (use `tokio::time::pause` + `advance` for that — see "Determinism" below).
- Field order of JSON objects, HashMap iteration order, or anything else the language/runtime is allowed to change.

## Patterns specific to this codebase

### Trait-first means the trait tests its own contract

Every trait that has more than one implementation (`GraphRepo`, `MessageBus`, `TelemetryRepo`, `OutputDriver`, later `NodeBehavior`) owns a **shared test suite** behind a `testing` feature on the defining crate. Every implementor runs that suite from its own dev-dependencies. Example:

```
crates/data-repos/
  src/testing.rs        ← shared `GraphRepo` acceptance tests behind `testing` feature
  Cargo.toml            ← [features] testing = ["tokio/test-util"]  (or similar)

crates/data-sqlite/
  Cargo.toml            ← [dev-dependencies] data-repos = { workspace = true, features = ["testing"] }
  tests/repo.rs         ← calls data_repos::testing::run_all(SqliteGraphRepo::new_memory())
```

When the Postgres impl lands, it runs the same suite with a Postgres connection. A query that works on one and fails on the other is caught at the seam, not in production.

The shared suite tests the **contract** (empty snapshot, roundtrip, delete, generation-bump, etc.). Impl-specific tests (e.g. Postgres RLS, SQLite WAL behaviour) live alongside the impl, not in the shared suite.

### Fakes and mocks live in a `testing` module

Three patterns, in order of preference:

1. **Real impl + in-memory backing.** An in-memory `GraphRepo` is as real as a SQLite one — no surprises from mocking. Preferred.
2. **Hand-rolled fake.** When a real impl is unreasonable (e.g. `FlakyRepo` — refuses writes to prove write-through atomicity), write a small hand-rolled fake in the consumer's `tests/` directory. Keep it narrow.
3. **Mock framework.** Avoid. We've never needed one; if we do, it's usually a sign the trait is too wide.

Fakes that multiple crates would benefit from go in the shared `testing` crate (`dev-dependencies`-only for consumers, per CODE-LAYOUT). Not imported from production code, ever.

### Contract fixtures — committed, versioned, round-tripped

For any format that crosses a boundary (wire, storage, manifest), commit fixtures:

```
crates/spi/tests/fixtures/msg/
  minimal.json                ← bare {"payload": null}
  node_red_style.json         ← msg.payload + msg.topic + custom fields
  full.json                   ← every canonical field populated
  with_parent_id.json         ← msg.child() output
```

The fixture test parses each file, asserts fields and types, and — critically — **re-serialises and asserts byte-for-byte equality** where the format is supposed to be stable. Drift in field order, escaping, or whitespace is caught here.

When the TS SDK lands, the TS test suite parses the same fixtures. Cross-language wire stability is guaranteed by construction.

### Snapshot tests for generated output

Snapshot tests catch unintended changes to:

- `KindManifest` values emitted by `#[derive(NodeKind)]`.
- Generated OpenAPI specs from `utoipa`.
- CLI `--help` output (so docs don't silently drift from reality).
- Error messages returned to users over HTTP/gRPC.

Use `insta` for Rust snapshots (well-maintained, plays well with `cargo insta review`). Commit snapshot files. Review deltas explicitly — "accept this change" is a decision, not a shrug.

### Multi-backend parity — hot loop

Any feature that ships on SQLite edge and Postgres cloud runs its acceptance suite against both in CI. Not "most tests work on both, a few are SQLite-only." *All* portable tests run on both; impl-specific tests are the exception. This is how we keep the two backends from quietly drifting in behaviour.

### Property tests for parsers and serialisers

Anything that parses input or serialises output gets property tests via `proptest`:

```rust
proptest! {
    #[test]
    fn msg_json_round_trips(payload in arb_json_value()) {
        let original = Msg::new(payload);
        let bytes    = serde_json::to_vec(&original).unwrap();
        let parsed: Msg = serde_json::from_slice(&bytes).unwrap();
        prop_assert_eq!(original.id, parsed.id);
        prop_assert_eq!(original.payload, parsed.payload);
    }
}
```

Strategies (like `arb_json_value()`) are shared in the `testing` crate so every caller uses the same ones.

### "Write-through" / "fail-closed" patterns

A lot of the platform's correctness depends on "write fails → in-memory state untouched" invariants. Tests should make these mechanical:

```rust
#[test]
fn flaky_repo_leaves_memory_clean_on_write_failure() {
    let repo  = FlakyRepo::refuses_writes();
    let store = GraphStore::with_repo(KindRegistry::new(), sink, repo);
    let err   = store.create_root(KindId::new("sys.core.station"), "s").unwrap_err();
    assert!(matches!(err, GraphError::Backend(_)));
    assert_eq!(store.len(), 0, "write-through: memory must not mutate on backend failure");
}
```

If the test name describes the invariant, the failure message tells the next maintainer which invariant broke.

## Determinism — time, randomness, I/O

Flaky tests are worse than no tests — they train people to ignore failures. Determinism is enforced, not hoped for.

| Source of flakiness | Mitigation |
|---|---|
| Wall-clock time | Inject a `Clock` trait. Use `tokio::time::pause` + `advance` for timer-driven tests. Never `std::thread::sleep`. |
| Random ids (UUIDs) | Test harness supplies a seeded RNG or a counter-based id generator. Real UUIDs used only at the very top of integration tests where non-determinism is acceptable. |
| Filesystem | Use `tempfile` crate; clean up on drop. No fixed paths. |
| Network | Don't hit real networks from tests. Use `wiremock` (Rust) or an in-process HTTP server. |
| Process spawn | Use a fake for the block supervisor in unit tests; real spawn only in targeted integration tests marked `#[ignore]` unless `--all-targets` + `--include-ignored`. |
| Async task ordering | Single-threaded `#[tokio::test]` by default. Multi-threaded only when the test's whole point is concurrency. |
| Map iteration order | Use `BTreeMap` in assertions, or sort before compare. |

**A test that passes 10 times in a row and fails on the 11th is broken.** Mark as `#[ignore]` and fix it, don't re-run.

## What NOT to test

Writing tests has a cost. Spend it where it pays:

- **Trivial getters/setters / `impl Default`.** Compiler proves it; a test just re-asserts the struct literal.
- **Derived trait impls (`#[derive(Debug, Clone, Serialize, …)]`).** The derive macro is tested upstream.
- **Obvious compositions of tested pieces.** If `a()` and `b()` are both thoroughly tested, a trivial wrapper `fn c() { a(); b() }` rarely earns its own unit test — integration tests cover the composition.
- **Framework code (axum routing, serde plumbing).** Test your handler, not that Axum routes URLs.
- **`unwrap`-free error paths that are provably unreachable.** If the compiler can prove it, you don't need the test.
- **The value of compile-time constants.** `assert_eq!(FLOW_SCHEMA_VERSION, 1)` is not a test.

The litmus test: would the test ever catch a regression that another test doesn't? If no, don't write it.

## Fixtures and test data

- **Fixtures go with the test.** `tests/fixtures/<area>/*.json`, committed. Never generate fixtures at test time and assert on them — that's proving the generator is consistent with itself.
- **Minimal, targeted fixtures.** One fixture per interesting shape, not one giant fixture covering ten cases.
- **Named by what they represent**, not `fixture_1.json`. `node_red_style_custom_fields.json` tells you why it exists.
- **Updated deliberately.** Changing a fixture is a review item. A large "accept all snapshot changes" diff is a red flag — something's breaking that shouldn't be.

## CI gates — non-negotiable

Every PR runs:

```
cargo fmt --all --check                                     # formatting
cargo clippy --workspace --all-targets -- -D warnings       # lints, all targets, no warnings
cargo test --workspace                                      # every test
cargo test --workspace --all-features                       # feature-flagged paths
```

Additional gates land with specific features:

- **Old-flow fixture suite** (Stage 5+) — every prior release's flow fixtures round-trip through migrations.
- **Capability-diff check** (Stage 10) — `spi.*` capability removals without a deprecation flag fail the build.
- **No `println!` / `console.log` in library code** (per LOGGING.md) — grep-based test.
- **Msg round-trip fixture test** — once TS SDK ships, the same fixtures are parsed by both Rust and TS.

A PR that fails any gate doesn't merge. No "fix it in a follow-up."

## Running tests locally

The three commands that must pass before handing work back:

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For faster iteration inside one crate:

```
cargo test -p graph               # just the graph crate
cargo test -p graph persistence   # just tests matching `persistence`
cargo test -p graph -- --nocapture   # show println output during failures
```

For snapshot review:

```
cargo insta review                # walk snapshot diffs interactively
```

For debugging a flaky test suspicion:

```
cargo test -p <crate> <name> -- --test-threads=1          # force serial
for i in $(seq 1 100); do cargo test -p <crate> <name> || break; done   # loop until fail
```

## Writing tests before the code — the discipline

The useful shape of TDD here:

1. **Decide what behaviour you're adding.** One thing. Write its test first.
2. **Let the compiler drive you.** The test names the types, traits, and methods you need. If the test won't compile, that's the design feedback — adjust the trait, add the type, rename the method. You're designing the public interface.
3. **Make the test fail for the right reason.** A compile error is progress but not failure; a failing assertion is. If the test passes without the implementation, the test is wrong.
4. **Write the smallest code that makes the test pass.** Don't anticipate next week's needs. If the next test fails, you'll implement more then.
5. **Refactor with the test still passing.** Now you can move things around knowing the behaviour is pinned.

When this flow feels wrong — a big refactor, a performance optimisation, a migration — it's fine to skip ahead. But the rule still holds: tests arrive in the same PR.

## One-line summary

**Tests arrive with the code, prove behaviour not implementation, and cover every contract surface with fixtures and trait-suites; fakes and mocks live in a `testing` crate; determinism is enforced via injected clocks and seeded RNGs; CI runs fmt + clippy + test + feature-gated paths on every PR; a PR that fails any gate doesn't merge.**
