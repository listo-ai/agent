# New Session Prompt — Start Here

You are an AI coding assistant working on this repository. This doc is the entry point for any coding session: it tells you what the project is, the non-negotiables, which docs to read for the task the user gave you, and how to work in the repo.

**Read this file first. Then read the docs it points you to for your specific task. Do not skip the routing — the docs are short, focused, and answer questions you'd otherwise guess at.**

---

## What this project is

A generic, extensible, flow-based integration platform. Rust, one codebase, runs on 512 MB ARM edge gateways and in the cloud. Users author flows in a desktop/browser Studio; the Control Plane ships flows to agents; agents execute them against pluggable extensions.

The thesis is a **node/slot/flow** model plus a first-class **extension system**. Anything a flow can touch — a protocol driver, an API, a database, a user account, the agent's own health metrics — is a node in a unified graph. Nothing is special-cased.

Common applications include BAS, industrial IoT, home automation, service orchestration, ETL, internal-tools glue. None of them is the reason the platform exists; the platform exists to be a good generic substrate for all of them.

## Non-negotiables

These are enforced in review. A PR that violates them gets sent back. No exceptions.

1. **Everything is a node.** Users, devices, extensions, alarms, flows, health metrics — all nodes in one unified graph with typed slots, lifecycle, event stream, facets, containment rules. If you're tempted to add an entity outside this model, stop and ask why. See [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md).
2. **Layer separation: `transport → domain → data`.** Never the other direction. No SQL in handlers. No HTTP in domain. No business logic in transport. See [CODE-LAYOUT.md](CODE-LAYOUT.md).
3. **Small files, small functions.** 400 lines per file max, 50 lines per function max, ~10 public items per module. If you're about to write a 1200-line "complete solution," stop and split it first.
4. **Traits first, implementations second.** Write the interface before the impl. Domain depends on traits, not on concrete impls.
5. **Node-RED-compatible `Msg` envelope** is the message shape on wires. Immutable on the wire; mutable at the QuickJS Function-node JS boundary. See [EVERYTHING-AS-NODE.md § "Wires, ports, and messages"](EVERYTHING-AS-NODE.md) and [NODE-AUTHORING.md](NODE-AUTHORING.md).
6. **Every contract surface is versioned independently** via capability manifests — extensions declare required capabilities, host advertises provided ones, install is a set-match. See [VERSIONING.md](VERSIONING.md).

## Doc index

| Doc | What it covers |
|---|---|
| [README.md](../../README.md) | Full stack overview — engine, extensions, DB, messaging, auth, Studio, CLI, MCP. Read for the big picture. |
| [OVERVIEW.md](OVERVIEW.md) | Deployment profiles, build targets, capability matrix per deployment, memory budgets. |
| [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) | **The core model.** Graph, nodes, slots, kinds, facets, containment, cascading delete, Msg envelope, slot API, settings schemas (single + multi-variant). |
| [NODE-AUTHORING.md](NODE-AUTHORING.md) | How to write a node kind — manifest anatomy, settings vs msg overrides, worked HTTP-client example. |
| [RUNTIME.md](RUNTIME.md) | Engine lifecycle, safe-state on shutdown, simulation vs commissioning modes, crossflow concepts, outbox backpressure. |
| [UI.md](UI.md) | Studio architecture, Tauri + Rsbuild + Shadcn, Module Federation + iframe isolation for untrusted extensions, build targets. |
| [AUTH.md](AUTH.md) | Zitadel integration, JWT verification, JWKS caching (24h ceiling), offline operation, revocation via NATS deny-list. |
| [MCP.md](MCP.md) | MCP server — tools, resources, prompts, stdio auth, prompt-injection mitigations, three-layer off switch. |
| [QUERY-LANG.md](QUERY-LANG.md) | Generic RSQL → AST → SeaQuery framework used by REST, CLI, SDKs, MCP, NATS filters. |
| [DOCKER.md](DOCKER.md) | Multi-arch distroless images, docker-compose overlays per profile, edge vs cloud NATS topology, signing. |
| [CODE-LAYOUT.md](CODE-LAYOUT.md) | Crate structure (`/crates/*`), naming rules, layer discipline, anti-patterns, AI-specific guidance. |
| [VERSIONING.md](VERSIONING.md) | Per-surface semver rules, capability manifest, extension install-time match, kind migrations, deprecation windows. |
| [../sessions/STEPS.md](../sessions/STEPS.md) | **Current coding stages** with `[DONE]` / `[DEFERRED]` markers. Temporary — will be removed when implementation catches up. Always check it to see what's already wired up before duplicating work. |

## Task router — read these docs before starting

Find your task. Read the listed docs **in the order given** before touching code.

| Task | Read, in order |
|---|---|
| **Build a plugin / extension** (protocol driver, API integration, compute extension) | NEW-SESSION (this doc) → EVERYTHING-AS-NODE → NODE-AUTHORING → VERSIONING → UI (if it contributes UI) → CODE-LAYOUT (where extension crates live) → README (extension sections) |
| **Add a new built-in node kind** | NEW-SESSION → EVERYTHING-AS-NODE → NODE-AUTHORING → CODE-LAYOUT (`domain-*` crates) → STEPS (current stage) |
| **Work on the frontend / Studio** | NEW-SESSION → UI → EVERYTHING-AS-NODE (data model the UI renders) → NODE-AUTHORING (property-panel schemas + multi-variant forms) |
| **Work on the graph service (`/crates/graph`)** | NEW-SESSION → EVERYTHING-AS-NODE (entire doc — especially containment + cascading delete) → CODE-LAYOUT → RUNTIME (event model, subject taxonomy) → STEPS |
| **Work on the flow engine / runtime (`/crates/engine`)** | NEW-SESSION → RUNTIME → EVERYTHING-AS-NODE (wires, ports, messages; trigger policies) → NODE-AUTHORING (msg overrides) → CODE-LAYOUT |
| **Add or modify a REST API endpoint** | NEW-SESSION → QUERY-LANG → EVERYTHING-AS-NODE (slot API) → AUTH (AuthContext, RBAC) → CODE-LAYOUT (`transport-rest`) |
| **Work on auth / Zitadel / JWT** | NEW-SESSION → AUTH → CODE-LAYOUT (`/crates/auth`) |
| **Work on messaging / NATS / subjects** | NEW-SESSION → README (messaging section) → EVERYTHING-AS-NODE (event model, subject taxonomy) → CODE-LAYOUT (`/crates/messaging`, `transport-nats`) → RUNTIME (outbox) |
| **Database / persistence** | NEW-SESSION → CODE-LAYOUT (`data-*` crates, dual-backend rules) → EVERYTHING-AS-NODE (persistence schema) → README (database section) |
| **Time-series / telemetry** | NEW-SESSION → README (messaging + telemetry) → CODE-LAYOUT (`data-tsdb`). Remember: `data-tsdb` is a **code seam**, TimescaleDB is the cloud impl, SQLite rolling tables the edge impl. |
| **MCP server** | NEW-SESSION → MCP → AUTH → EVERYTHING-AS-NODE (slot API — MCP wraps it) |
| **Docker / deployment / Helm** | NEW-SESSION → DOCKER → OVERVIEW → README (cloud vs edge topology) |
| **CLI commands (`/crates/transport-cli`)** | NEW-SESSION → README (CLI section) → CODE-LAYOUT → QUERY-LANG (RSQL flags) |
| **Versioning / capabilities / extension compat** | NEW-SESSION → VERSIONING → CODE-LAYOUT (`/crates/spi`) |
| **Cross-cutting refactor / unsure where to start** | NEW-SESSION → CODE-LAYOUT → EVERYTHING-AS-NODE → STEPS — then **ask the user a clarifying question** before touching code |

If your task isn't in the table: read NEW-SESSION + CODE-LAYOUT + EVERYTHING-AS-NODE, then ask the user which other docs apply.

## How to actually work in this repo

### Before writing any code

1. Confirm you've read the docs the task router pointed at. If you haven't, do that first.
2. Check [STEPS.md](../sessions/STEPS.md) — is this task already partially done? Is it blocked on a deferred dependency from an earlier stage? Don't duplicate existing work; don't build on top of `[DEFERRED]` dependencies without flagging it.
3. Locate the right crate in [CODE-LAYOUT.md](CODE-LAYOUT.md). Be specific — if you're tempted to put something in `apps/agent`, you're almost certainly in the wrong place; `apps/agent` is thin composition only.
4. If what you're about to do needs a new trait, **design the trait before the impl.** Domain crates depend on traits, not on concrete `data-*` or `transport-*` types.
5. If what you're about to do touches a contract surface (`spi`, public API, manifest schemas, `Msg`, kind IDs), re-read the relevant section of [VERSIONING.md](VERSIONING.md) and make sure the change is add-only within the major or is correctly deprecating.

### While coding

- **No files over 400 lines. No functions over 50 lines. No `pub mod utils`.** If you're heading there, split now, not later.
- **Every crate has its own `Error` enum.** Use `thiserror` in libraries; reserve `anyhow` for the binary crate (`apps/agent`).
- **No `unwrap()` / `panic!()` in library code** except for explicitly-documented invariant violations.
- **Write no comments that restate the code.** Only leave a comment for non-obvious *why* — a hidden constraint, a workaround, a subtle invariant. Never reference "the current task" or PR context; that belongs in the commit message.
- **No `TODO`/`FIXME` without an issue number.**
- **Tests go with the code.** Unit tests in `#[cfg(test)] mod tests`; integration tests in `tests/`. A change to domain logic must have a test.
- **Message authoring:** use `spi::Msg` + `Msg::new` / `Msg::child`. Messages are immutable on the wire — you produce a new one, you don't mutate.
- **Slot role discipline:** `config` (user-authored, persisted, audited), `input` (live data in), `output` (live data out), `status` (engine-computed). Role determines RBAC, audit, and telemetry routing — getting it wrong breaks all three quietly.

### Running the build

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass before handing work back.

### Committing

- Only commit when the user explicitly asks.
- Never amend a previous commit — create a new one.
- Never skip hooks (`--no-verify`) unless the user explicitly says so.
- Never force-push to `main` / `master`.
- Commit message focuses on **why**, not what. One or two sentences. Don't recap the diff.

### What to do when stuck

Ask the user. Do not guess at design questions that affect the node model, layer boundaries, or contract surfaces — getting those wrong compounds. One sentence of "which of these two approaches did you want?" beats an hour refactoring the wrong direction.

## STEPS.md — where we are in implementation

For now, [STEPS.md](../sessions/STEPS.md) is the working plan: numbered stages, each producing something that runs, each proving a specific architectural risk. Each item is marked `[DONE]`, `[DEFERRED]`, or left unmarked (not started).

- **Always check STEPS first** when asked to implement something — it tells you what's already wired up and what's blocked.
- **Don't skip ahead of `[DEFERRED]` items** that a later stage depends on without flagging the situation to the user. The stage ordering exists because earlier stages are load-bearing for later ones.

STEPS will be removed once implementation is complete. This doc (NEW-SESSION) stays as the permanent orientation for new sessions.

## One-line summary

**Generic extensible flow platform. Everything is a node. Small files, strict layer separation, traits-first. Read the right docs for your task before coding. When in doubt, ask rather than guess.**
