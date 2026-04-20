# Versioning & Compatibility

How the platform stays maintainable for years as blocks, flows, and the host itself evolve independently. The short version: **don't version the platform as one number — version each contract surface, and match capabilities at install time.**

## The surfaces that change

Things that have contracts, and therefore need independent versioning:

| Surface | Where it's defined | Consumers |
|---|---|---|
| **`block.proto`** | `crates/spi/proto/` | Every block process |
| **`Msg` envelope** | `crates/spi/src/msg.rs` | Every node; Function-node JS API |
| **Node manifest schema** (`node.schema.json`) | `crates/spi/schemas/` | Every node kind declaration |
| **Flow document schema** (`flow.schema.json`) | `crates/spi/schemas/` | Persisted flows |
| **Host-function ABI** (for Wasm nodes) | `crates/blocks-sdk` | Every Wasm block |
| **Node kind** (e.g. `sys.io.http.client`) | The kind's own manifest | Flows that use the kind |
| **Public REST/gRPC API** (`/api/v1/`) | `crates/transport-rest` | External clients, SDKs |
| **Database schema** | `crates/data-sqlite`, `crates/data-postgres` | Migrations per backend |
| **Agent binary** | `crates/apps/agent` | Humans reading `yourapp --version` |

Eight or nine independently-versioned surfaces. Treating them as "the platform version" is the trap that kills long-term maintainability — one unrelated bump breaks everything.

## The core mechanism: capability manifests

The host **publishes** what it provides. Blocks **declare** what they need. Installation is a set-match; anything missing is a specific error.

### Host-provided capability manifest

Every running agent exposes its capability set — machine-readable, introspectable.

```yaml
# GET /api/v1/capabilities  |  yourapp capabilities
platform: { version: "1.4.2", role: "edge" }

capabilities:
  # Contract versions — semver-disciplined, add-only within a major.
  - id: spi.block.proto
    version: 1.3.0
    deprecated: []
  - id: spi.msg
    version: 1.0.0
  - id: spi.node.schema
    version: 1.1.0
  - id: spi.flow.schema
    version: 1.0.0
  - id: host_fn.wasm
    version: 1.2.0

  # Runtime features — whether a sandbox / store / transport is available on this deployment.
  - id: runtime.wasmtime
    version: 1.0.0
  - id: runtime.extension_process
    version: 1.0.0
  - id: feature.jetstream
    version: 1.0.0         # absent on Core-only edges
  - id: feature.tsdb.timescale
    version: 1.0.0         # absent on edge (no TSDB)
  - id: feature.tsdb.sqlite_rolling
    version: 1.0.0         # absent on cloud
  - id: data.postgres
    version: "17"
  - id: data.sqlite
    version: "3.45"

  # Deprecated-but-present capabilities carry a window.
  - id: spi.msg
    version: 0.9.0
    deprecated_since: "1.2"
    removal_planned: "2.0"
```

### Block's required-capabilities declaration

```yaml
# block.yaml — manifest excerpt for a BACnet driver
id: com.example.bacnet
version: 2.1.0

requires:
  - id: spi.block.proto
    version: "^1.2"          # semver range; ≥ 1.2, < 2.0
  - id: spi.msg
    version: "^1"
  - id: runtime.extension_process
  - id: spi.node.schema
    version: "^1.1"          # needs the schema feature added in 1.1

# Optional capabilities — install succeeds if missing, feature gracefully degrades.
optional:
  - id: feature.tsdb.timescale
    reason: "used for historical trend queries in the UI; absence disables that tab"
```

### Install-time match

```
extension_requires  ⊆  host_provides   →  install
otherwise                              →  refuse with a specific error
```

Error format is structured, not a stack trace:

```
cannot install com.example.bacnet@2.1.0 on platform 1.4.2 (role=edge):

  × required capability `spi.node.schema` version ^1.1 — host provides 1.0
    platform 1.5 or newer is needed (spi.node.schema@1.1 added in 1.5)

  × required capability `runtime.wasmtime` — not provided on this agent
    wasmtime is disabled in this build. Use a binary built with --features wasmtime.

  ⚠ optional capability `feature.tsdb.timescale` — not provided
    historical trends will be unavailable.
```

Users fix the specific thing, or get an actionable next step. No "try again" guessing.

## Semver rules per surface

Not all surfaces use semver the same way. Clear rules per surface keep the system honest:

| Surface | Version rule | Enforcement |
|---|---|---|
| `block.proto` | **Add-only** within a major. Removing or changing a field requires a major bump. | CI diffs the `.proto` against the last release; fails on removed/renamed fields. |
| `Msg` envelope | Add-only for user-visible fields (`payload`, `topic`, user customs). Platform-reserved (`_*`) may change across majors only. | Serde round-trip tests across versions. |
| Node manifest schema | Add-only within major. New optional properties fine; new required properties are a major bump. | JSON-Schema-aware diff in CI. |
| Flow document schema | Add-only within major. Migrations required for any major bump. | Migration test suite: every old flow fixture round-trips through to the current version. |
| Host-function ABI | Add-only within major. Renaming or changing a signature is a major bump. | Wasm contract tests against the last release binary. |
| Node kind | Per-kind semver. Adding an optional slot or setting is minor; renaming or tightening validators is major. | Kind's own migration routines (see below). |
| Public API (`/api/v1/`) | Per existing rules: add-only within major, `Deprecation`/`Sunset` headers, 12-month window. | Existing `api-changes.yaml` gate in CI. |
| Database schema | Forward-only migrations per backend. No version numbers consumed externally. | Migration test suite runs forward from every prior release. |
| Agent binary | Human-readable semver. **Not** used for compat gates — consumers look at capabilities. | Release tagging. |

The rule: **what changes, what doesn't change, and what CI enforces** are documented per surface and tested on every PR. A surface without an enforcement mechanism will drift.

## Node-kind evolution — schema migrations

Kinds evolve faster than the platform. A BACnet point kind at version 1 has `instance_number: int`; version 2 adds `device_instance: int` (required for disambiguation after a rearchitecture). Flows persisted against v1 need to load cleanly on v2.

Mechanism:

1. Kind manifest carries a `schema_version` field: `sys.driver.bacnet.point@2`.
2. Persisted node config records the schema version it was saved with.
3. The kind ships **migrations** alongside it — Rust functions that take a v1 config and return a v2 config.
4. On load, the graph service walks migrations in order (v1 → v2 → v3 → current).
5. After migration, the config is validated against the current schema. Failure is surfaced clearly, not swallowed.
6. Down-migrations are optional; if absent, an older platform can't load a newer flow and says so explicitly.

Migrations are **authored with the kind**, not after the fact, and tested in CI with fixtures.

## Flow document migration

The flow document itself has `schema_version`. Same pattern: forward migrations, tested with old-flow fixtures, failure is explicit. Independent of kind-level migrations (both may run on the same flow load).

## Deprecation windows

Every capability and kind carries optional `deprecated_since` and `removal_planned`. Removing a capability requires:

1. A release that marks it deprecated (no behaviour change).
2. A minimum **12-month** window before removal, matching the public-API deprecation policy.
3. Blocks using deprecated capabilities get install-time warnings (not errors) during the window.
4. A release-notes entry naming each removal, with migration guidance.

This is the rule that makes "we can actually ship breaking changes someday without destroying the ecosystem" true.

## What this looks like in the crate layout

```
/crates/spi
  src/msg.rs                 # Msg envelope — versioned via its own file-top doc + CI diff
  src/capabilities.rs        # CapabilityId, CapabilityVersion, SemverRange types; matcher
  schemas/flow.schema.json   # has schema_version: 1
  schemas/node.schema.json   # has schema_version: 1
  proto/block.proto      # package block.v1

/crates/blocks-host
  src/capability_registry.rs # host-side: what this agent provides
  src/compat.rs              # install-time match; structured error type

/crates/transport-rest
  src/capabilities.rs        # GET /api/v1/capabilities handler
```

The `spi` crate owns the capability ID constants so every crate refers to them by name, not by string. New capability IDs are added by PR to `spi`; CI checks that removing an ID is paired with a deprecation flag.

## Interaction with the existing `/api/v1/` versioning

They don't replace each other — they cover different things:

| Question | Answer |
|---|---|
| "What API version does this client speak?" | URI versioning (`/api/v1/...`). |
| "What contract versions and features does this agent provide?" | Capability manifest. |

An external SDK pins to `/api/v1/`. An block running inside an agent declares required capabilities. Both mechanisms live in parallel because they answer different questions.

## Operational: `yourapp` commands

```
yourapp capabilities               # list this agent's provided capabilities (human-readable)
yourapp capabilities --json        # machine-readable, for scripting
yourapp ext check <manifest>       # dry-run the compat match without installing
yourapp ext upgrade <id>           # pick the newest block version compatible with this host
yourapp upgrade --dry-run          # report which installed blocks would break on a platform upgrade
```

That last one is the real win: before upgrading the platform, you can see which blocks would stop working and act on it deliberately.

## Operational: CI gates

Per release:

1. **Contract diffs** — `.proto`, JSON Schemas, host-function ABI diffed against the last release. Removed/renamed items fail the build unless paired with a deprecation window commit.
2. **Compat report generation** — the capability manifest is emitted as a release artifact and stored in a repo of compat snapshots.
3. **Old-flow fixture suite** — every prior release's sample flows round-trip through the migration pipeline on the new binary.
4. **Reference-block suite** — a set of first-party blocks (math, HTTP, MQTT, the demo driver) is rebuilt and install-matched against each release candidate.

A PR that breaks any of these fails CI with a specific message naming what broke.

## One-line summary

**Every contract surface versions independently; the host publishes a capability manifest; blocks declare required capabilities; installation is a set-match with structured errors; node kinds and flow documents carry forward-migration code they ship with; deprecation windows keep a 12-month runway before anything breaks — so blocks written today can still run in two years, and users upgrading the platform see exactly what would stop working before they do it.**
