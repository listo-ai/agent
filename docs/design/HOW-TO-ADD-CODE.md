# HOW TO ADD CODE — Start Here Every Session

You are an AI coding assistant working on the Listo platform. This doc is the single entry point for any coding session. It tells you:

1. **Which skills to load** for the language you're about to write.
2. **Where code must go** — the decision tree that routes your task to the right repo.
3. **What each library is for** and the hard rules that keep layering intact.
4. **How to actually run things** — via `mani` across the multi-repo workspace.

> **MUST READ:** This platform is a multi-repo workspace designed so that **anyone can build a new UI** against the same agent by pulling `@listo/agent-client`, `@listo/ui-kit`, and `@listo/ui-core` from npm. That only works if every contributor respects the library boundaries. If you put React in `agent-client-ts`, a hook in `ui-kit`, or reach from a block directly into `ui-core`, you break the ecosystem for everyone downstream. **This is not a style preference. It is a load-bearing invariant.**

---

## 0 — Load skills first

Before writing a single line, read **[`SKILLS/CODE-LAYOUT.md`](SKILLS/CODE-LAYOUT.md)** — the engineering-discipline bible. It defines the hard limits (400 lines / 50-line functions / no `pub mod utils`), the `transport → domain → data` dependency arrow, and the 20-line ceiling on REST handlers. **Read it once at the start of every session.** The rest of this doc assumes you've internalised it.

Then look up the skill set for the language you're about to touch:

| Language | Skills index |
|---|---|
| Rust | [SKILLS/rust.md](SKILLS/rust.md) |
| TypeScript / React | [SKILLS/ts.md](SKILLS/ts.md) |
| Go | [SKILLS/golang.md](SKILLS/golang.md) |
| Dart / Flutter | [SKILLS/dart.md](SKILLS/dart.md) |

These point at `~/.agents/skills/*/SKILL.md` bundles. Always load, at minimum, **language skill + API/interface design + TDD + code review** for the language you're editing.

---

## 1 — Non-negotiables

These are enforced in review. Violating one sends the PR back, no exceptions.

### Rule A — Everything. Is. A. Node.

No private entities. No parallel state. Devices, users, flows, alarms, dashboards, the agent's own health — **all nodes**. If you're about to write a subsystem that owns `Mutex<SomeState>` that nobody outside the subsystem can observe, **stop** — promote the state to a kind with status slots. See [EVERYTHING-AS-NODE.md](agent/docs/design/EVERYTHING-AS-NODE.md).

### Rule B — The graph is the world. One API, no back channels.

Engine, flows, Studio, MCP, CLI, blocks — **all** read and write the graph through the same slot API. No parallel models. When you add a subsystem, your first question is "what nodes does it contribute, what slots does it read/write?" — not "what new API do I need?"

> **Transport note:** The agent's pub/sub transport is **Zenoh**, not NATS. There is no NATS in the codebase. If you see NATS mentioned anywhere else, it's stale — Zenoh is the reality.

### Rule C — Modular libraries are load-bearing

The workspace is eight+ repos on purpose. A customer can build a mobile admin panel by depending on `@listo/agent-client` + `@listo/ui-kit` from npm, skipping everything else. That only works if:

- **`ui-kit` has zero logic.** No hooks, no stores, no API calls. Only Shadcn primitives + design tokens.
- **`agent-client-ts` has zero React.** It's a plain HTTP client, usable from Node, Bun, Deno, browsers, whatever.
- **`ui-core` owns the "brain".** Every React hook that talks to the agent lives here. **Studio and all other frontends must import hooks from `ui-core`, never from `agent-client-ts` directly.** If a hook wraps an API call, it lives in `ui-core` — period.
- **`block-ui-sdk` is a curated facade.** Blocks depend on it and nothing else. If a block needs something missing, add it to `block-ui-sdk` — do not reach into `ui-core` directly from a block.
- **`blocks/*` never path-dep the agent.** Only `agent-sdk` and the clients.

If you're about to cross one of these lines, you're wrong. Section 4 lists the MUSTs and MUST NOTs in full.

### Rule D — Small units

400 lines per file, 50 lines per function, ~10 public items per module. Split first, then add.

### Rule E — Test lives with the code

Not later. Same PR. See [TESTS.md](agent/docs/design/TESTS.md).

### Rule F — `Msg` is immutable on the wire

Node-RED parity. Produce new messages via `Msg::new` / `Msg::child`; don't mutate. The Rhai Function-node is the one place `msg` feels mutable to the author — the runtime snapshots it on exit. See [EVERYTHING-AS-NODE.md § "Wires, ports, and messages"](agent/docs/design/EVERYTHING-AS-NODE.md).

### Rule G — Layer separation

`transport → domain → data` within the agent. Never the other way. No SQL in handlers, no HTTP in domain. See [SKILLS/CODE-LAYOUT.md](SKILLS/CODE-LAYOUT.md).

### Rule H — Versioning on contract surfaces

Changes to `spi`, public client APIs, kind manifests, and `Msg` shape are versioned per [VERSIONING.md](agent/docs/design/VERSIONING.md). Add-only within a major.

### Rule I — Transport handlers stay thin

This is the rule most often forgotten under deadline pressure. **A REST/gRPC/CLI handler does four things**: (1) extract inputs, (2) call a domain function, (3) map the result to a DTO, (4) return. If your handler has business logic, containment rules, graph walks, or anything that would apply equally to a gRPC or Zenoh surface — **that logic doesn't belong in `transport-*`**. Move it to `graph`, `domain-*`, or a shared crate, and have every transport call the same function.

The canonical smoke test: **if you swap REST for gRPC tomorrow, how much of this file changes?** If more than the route wiring and DTO shaping, your layering is wrong.

See [`SKILLS/CODE-LAYOUT.md` § "REST code stays in `transport-rest/`"](SKILLS/CODE-LAYOUT.md) for the full ruleset and the 20-line handler ceiling. Worked example in §7 below.

---

## 2 — Where does my code go? (the decision tree)

Walk this top-to-bottom. Stop at the first "yes".

### Q1. Am I changing a wire-level type?

*Examples: field added to `Msg`, new slot-schema key, new facet, new `KindManifest` field.*

→ **[`contracts/spi`](contracts)** (publishes `listo-spi` on crates.io; the TS client regenerates types from it).

Then: `mani run codegen` to regenerate `agent-client-ts/src/generated/`. Every downstream consumer (`agent`, `agent-sdk`, `agent-client-rs`, `ui-core`) will pick it up on next rebuild. **Do not copy types by hand** — the single source of truth is `spi`.

### Q2. Is this a new BUILT-IN node kind that ships inside the agent binary?

*Examples: `sys.logic.function`, `sys.compute.pluck`, `sys.logic.heartbeat`.*

→ **[`agent/crates/domain-*`](agent/crates)** — `domain-compute` for transforms, `domain-logic` for control flow, `domain-function` for scripting, or create a new sibling crate if the concern is new.

Then: register the kind in [`apps/agent/src/main.rs`](agent/crates/apps/agent/src/main.rs) alongside the others — `kinds.register(<X as NodeKind>::manifest())` plus `behaviors.register(…)`. Read [NODE-AUTHORING.md](agent/docs/design/NODE-AUTHORING.md).

### Q3. Is this a PLUGGABLE block (user-loadable, possibly third-party)?

*Examples: MQTT, BACnet, Modbus, project-management block.*

→ **[`blocks/com.<org>.<name>/`](blocks)** — standalone Cargo crate + optional MF UI bundle. Consumes `listo-blocks-sdk` (published).

**Do not** add a path dep on the agent workspace from a block. Only from `agent-sdk` (the public SDK) and `contracts` (the public types). If the SDK is missing something you need, **add it to `agent-sdk`** and bump — that's the correct direction for a change that benefits every block author.

### Q4. Is this a REST API endpoint on the agent?

*Examples: `GET /api/v1/search`, `POST /api/v1/nodes`, `GET /api/v1/history`.*

→ **[`agent/crates/transport-rest`](agent/crates/transport-rest)** for the route.
→ Then mirror the client surface in each language:
  - **[`agent-client-rs`](agent-client-rs)** — Rust client.
  - **[`agent-client-ts`](agent-client-ts)** — TS client (Zod schemas + domain API).
  - **[`agent-client-dart`](agent-client-dart)** — Dart client.

For filter/sort/pagination surfaces, use the generic [RSQL query framework](agent/docs/design/QUERY-LANG.md) — add a `QuerySchema` declaratively, don't hand-roll a parser. One schema, every transport (REST, CLI, MCP) gets the filter surface for free.

### Q5. Is this a React HOOK, STORE, PROVIDER, or API-call WRAPPER?

*Examples: `useKinds()`, `useNode(path)`, `AuthProvider`, `SduiRenderer`, `ScopeProvider`, graph store.*

→ **[`ui-core`](ui-core)** — this is the "portable brain" that any React frontend (Studio, a mobile admin, a reporting dashboard) can consume. **Studio and all other frontends must import hooks from `ui-core`, never from `agent-client-ts` directly.**

**Never put hooks or business logic in `ui-kit`.** If you catch yourself typing `useQuery` inside `ui-kit/src/`, you're in the wrong repo.

### Q6. Is this a VISUAL primitive — Shadcn component, design token, icon/color picker?

*Examples: Button, Badge, Select, Dialog, Card, tailwind preset, `main.css` tokens.*

→ **[`ui-kit`](ui-kit)** — **pure visual primitives only**.

**MUST NOT** import: React Query, zustand, `@listo/agent-client`, any hook, any business logic. If a component needs data, it takes it as a prop — the hook wrapping it lives in `ui-core`.

### Q7. Is this a STUDIO PAGE, FEATURE, or app-level routing concern?

*Examples: FlowsListPage, BlocksPage, SettingsPage, global AI chat feature, router config.*

→ **[`studio`](studio)** — the application shell. Imports from `@listo/ui-kit`, `@listo/ui-core`, `@listo/agent-client`.

**MUST NOT** put Studio-specific pages in `ui-core`. If a page is reusable across frontends (e.g. a generic node editor), it can live in `ui-core/src/pages/` — but Studio-branded pages, navigation, and the Tauri shell stay in `studio`.

### Q8. Is this a BLOCK UI panel (MF bundle shipped inside a block)?

*Examples: mqtt-client Panel, bacnet Panel, project-management Kanban view.*

→ **[`blocks/<id>/ui-src/`](blocks)** — the block's MF bundle.
→ Only depend on **[`@listo/block-ui-sdk`](block-ui-sdk)** and `@listo/agent-client`.

**MUST NOT** `import` from `@listo/ui-core` directly from a block. `block-ui-sdk` is a curated re-export facade — if something you need isn't there, **add it to `block-ui-sdk`** first. Direct `ui-core` imports from a block are a layering violation every time.

### Q9. Is this the `agent <cmd>` CLI (built into the agent binary)?

*Examples: `agent kinds list`, `agent slots write`, `agent flows edit`.*

→ **[`agent/crates/transport-cli`](agent/crates/transport-cli)** — thin clap wrapper over `agent-client-rs`.

If the command needs a capability the Rust client doesn't expose, add the capability to **`agent-client-rs` first**, then call it from the CLI. Never hit the REST URL with a raw HTTP call from the CLI — the client is the abstraction seam, and every new CLI command validates the client surface.

### Q10. Is this the Tauri desktop shell?

*Examples: native menu bar, system tray integration, auto-updater hook, file-system access.*

→ **[`desktop`](desktop)** — the shared `src-tauri` backend. Used by Studio's desktop builds on Windows / macOS / Linux.

### Q11. Is this the supervisor / lifecycle / OTA?

*Examples: A/B slot updates, rollback on boot, opt-in consent, systemd integration.*

→ **[`listod`](listod)** — the supervisor daemon. Manages `agent` lifecycle, separate from the agent itself.

### Q12. Is this MCP server work?

*Examples: new MCP tool, prompt-injection mitigation, stdio auth.*

→ **[`agent-cli`](agent-cli)** for the MCP bootstrap, and **[`agent/crates/transport-mcp`](agent/crates/transport-mcp)** for the tool surface.

### Q13. Is this orchestration / repo management / developer tooling?

*Examples: `mani run` task, new repo in the workspace, version pin across repos.*

→ **[`repos-cli`](repos-cli)** for the tool itself; **[`mani.yaml`](mani.yaml)** for task/project definitions.

### Q14. Is this documentation?

- **Design spec / architecture** → `agent/docs/design/`.
- **Session / working plan** → `agent/docs/sessions/`.
- **Testing walkthrough** → `agent/docs/testing/`.
- **Cross-repo developer docs** → the **[`docs`](docs)** repo (currently sparse; will fill as public-facing docs emerge).

> This doc lives at the **workspace root** (`HOW-TO-ADD-CODE.md`). A mirrored copy exists at `agent/docs/design/HOW-TO-ADD-CODE.md` — the root copy is canonical.

### Still unsure?

→ Read [OVERVIEW.md](agent/docs/design/OVERVIEW.md) for the full repo map + dependency arrow, then ask the user.

---

## 3 — What each library is for

Cheat-sheet summary. Full map: [OVERVIEW.md § "The workspace"](agent/docs/design/OVERVIEW.md#the-workspace--repos-roles-and-ownership-boundaries).

| Repo | Published as | Owns |
|---|---|---|
| [`contracts`](contracts) | `listo-spi`, `listo-ui-ir` | Wire types. `Msg`, `KindManifest`, slot schemas, `block.proto`. **Zero internal deps.** |
| [`agent`](agent) | — (private) | The platform: engine, graph, blocks-host, domain-*, transport-*, the binary. |
| [`agent-sdk`](agent-sdk) | `listo-blocks-sdk` | Block-author SDK. `NodeBehavior`, `NodeCtx`, `run_process_plugin()`. |
| [`agent-client-rs`](agent-client-rs) | `listo-agent-client` | Rust HTTP client. Zero Rust-agent dep. |
| [`agent-client-ts`](agent-client-ts) | `@listo/agent-client` | TS HTTP client. **Zero React.** Zod schemas. |
| [`agent-client-dart`](agent-client-dart) | `listo_agent_client` | Dart/Flutter HTTP client. |
| [`ui-kit`](ui-kit) | `@listo/ui-kit` | Shadcn primitives + design tokens. **Zero logic.** |
| [`ui-core`](ui-core) | `@listo/ui-core` | The portable brain — hooks, stores, providers, SDUI renderer. |
| [`block-ui-sdk`](block-ui-sdk) | `@listo/block-ui-sdk` | Curated facade over `ui-core` for block MF bundles. |
| [`studio`](studio) | — (private) | The Studio app. One consumer of ui-core + ui-kit. |
| [`desktop`](desktop) | — (private) | Shared Tauri shell for native builds. |
| [`blocks`](blocks) | — (reference) | Example blocks; template for third-party authors. |

---

## 4 — Reusable-library rules (the MUST / MUST NOT list)

These are the rules that keep the modular architecture honest. Each rule has a reason — violating it breaks a downstream user's ability to assemble a different product from the same bricks.

| Repo | MUST | MUST NOT |
|---|---|---|
| `contracts` | Be standalone. Schemas + types only. | Depend on anything internal. Contain runtime logic. |
| `agent-client-ts` / `agent-client-rs` / `agent-client-dart` | Be a thin HTTP client. Validate responses at the boundary (Zod / serde). | Import React, zustand, or any UI library. Contain business logic beyond request/response shaping. |
| `agent-sdk` | Expose the `NodeBehavior` trait and related authoring surfaces. | Depend on the agent's internal crates. Leak `domain-*` types into the block author's view. |
| `ui-kit` | Ship Shadcn primitives + design tokens. Accept data via props. | Import React Query, zustand, `@listo/agent-client`, any hook. Have opinions about data shape. |
| `ui-core` | Own every hook / store / provider that talks to the agent. Re-export `agent-client` types when needed. | Contain Studio-branded pages or navigation. Import `@listo/studio`. |
| `block-ui-sdk` | Be a curated re-export facade over `ui-core`. | Grow its own implementation. Expose raw `ui-core` internals. |
| `studio` | Consume the libraries — never fork them. | Bundle its own copies of Shadcn primitives, graph store, SDUI renderer. |
| `blocks/*` | Path-dep only `agent-sdk` (published) and `contracts`. | Path-dep the `agent` workspace. Copy types from `spi`. |
| `agent` | Be the dead-end of the dependency graph — nothing consumes it. | Ever appear as a dep in another repo's manifest. |

### The "build a new UI" smoke test

Before merging, ask: **if someone deletes `studio/` entirely and only has `ui-kit` + `ui-core` + `agent-client-ts` on npm, can they build a working admin UI?**

If your change requires them to fork or reach into Studio's code — you broke the invariant. Put the reusable piece in `ui-core`, the primitive in `ui-kit`, and only the Studio-specific composition in `studio`.

---

## 5 — Workflow: drive everything with `mani`

The workspace is multi-repo. `mani` is the orchestrator. **Read [repos-cli/EXAMPLE.md](repos-cli/EXAMPLE.md) end to end once** — it's the canonical walkthrough for setting up and driving the workspace.

The three commands you'll actually run every day:

```bash
mani run build --all              # build everything (auto-detects lang per repo)
mani run test  --all              # test everything
mani run status --all             # git branch + ahead/behind + dirty-count per repo
```

Dev servers for the agent + Studio:

```bash
mani run dev-single --projects agent   # standalone :8080 + Studio :3000
mani run dev-cloud  --projects agent   # cloud :8081 + Studio :3002
mani run dev-edge   --projects agent   # edge  :8082 + Studio :3010
mani run dev        --projects agent   # cloud + edge + both Studios
mani run kill-dev   --projects agent   # nuke all agent/Studio dev ports
```

Cross-cutting housekeeping:

```bash
mani run clean-edge --projects agent   # wipe edge.db + installed blocks
mani run codegen                       # regenerate TS types from contracts
mani run fetch --all                   # git fetch --all --prune across every repo
```

The full task list is in [`mani.yaml`](mani.yaml) — `mani list tasks` to see it.

### Port map (pin this)

| Role | Agent | Studio |
|---|---|---|
| standalone (`dev-single`) | 8080 | 3000 |
| cloud (`dev-cloud`) | 8081 | 3002 |
| edge (`dev-edge`) | 8082 | 3010 |
| second edge | `HTTP_PORT=8083 STUDIO_PORT=3011 mani run dev-edge --projects agent` | — |

If you see "connection closed by peer" in MQTT logs or duplicate nodes in the graph, check for stale agent processes (`mani run kill-dev`) — two agents with the same config on different ports will fight over the MQTT broker's client_id slots and kick each other at 1 Hz.

---

## 6 — Task-specific reading

Find your task; read the listed docs **in order** before writing code.

| Task | Read, in order |
|---|---|
| Run local tests / start dev env | This doc → [TESTING.md](agent/docs/testing/TESTING.md) |
| Add a new built-in node kind | This doc → [EVERYTHING-AS-NODE.md](agent/docs/design/EVERYTHING-AS-NODE.md) → [NODE-AUTHORING.md](agent/docs/design/NODE-AUTHORING.md) → [SKILLS/CODE-LAYOUT.md](SKILLS/CODE-LAYOUT.md) |
| Build a new block | This doc → [EVERYTHING-AS-NODE.md](agent/docs/design/EVERYTHING-AS-NODE.md) → [NODE-AUTHORING.md](agent/docs/design/NODE-AUTHORING.md) → [BLOCKS.md](agent/docs/design/BLOCKS.md) → [VERSIONING.md](agent/docs/design/VERSIONING.md) |
| Add / modify a REST endpoint | This doc → [QUERY-LANG.md](agent/docs/design/QUERY-LANG.md) → [EVERYTHING-AS-NODE.md](agent/docs/design/EVERYTHING-AS-NODE.md) → [AUTH.md](agent/docs/design/AUTH.md) → [SKILLS/CODE-LAYOUT.md](SKILLS/CODE-LAYOUT.md) |
| Work on the Studio or alt frontend | This doc → [UI.md](agent/docs/design/UI.md) → [NODE-RED-MODEL.md](agent/docs/design/NODE-RED-MODEL.md) (msg shape the UI renders) |
| Work on `graph` / `engine` | This doc → [EVERYTHING-AS-NODE.md](agent/docs/design/EVERYTHING-AS-NODE.md) (entire) → [RUNTIME.md](agent/docs/design/RUNTIME.md) → [SKILLS/CODE-LAYOUT.md](SKILLS/CODE-LAYOUT.md) |
| Auth / Zitadel / JWT | This doc → [AUTH.md](agent/docs/design/AUTH.md) |
| Messaging / Zenoh / pub-sub | This doc → [EVERYTHING-AS-NODE.md § event model](agent/docs/design/EVERYTHING-AS-NODE.md) → [RUNTIME.md § outbox](agent/docs/design/RUNTIME.md) |
| Persistence / database | This doc → [SKILLS/CODE-LAYOUT.md § data-* crates](SKILLS/CODE-LAYOUT.md) → [EVERYTHING-AS-NODE.md § persistence](agent/docs/design/EVERYTHING-AS-NODE.md) |
| Time-series / telemetry | This doc → [QUERY-LANG.md § Time-series](agent/docs/design/QUERY-LANG.md#time-series-query-shape-the-non-rsql-companion) |
| MCP | This doc → [MCP.md](agent/docs/design/MCP.md) → [AUTH.md](agent/docs/design/AUTH.md) |
| CLI | This doc → [SKILLS/CODE-LAYOUT.md](SKILLS/CODE-LAYOUT.md) → [QUERY-LANG.md](agent/docs/design/QUERY-LANG.md) |
| Versioning / capabilities | This doc → [VERSIONING.md](agent/docs/design/VERSIONING.md) |
| Logging / tracing | This doc → [LOGGING.md](agent/docs/design/LOGGING.md) |
| Writing tests | This doc → [TESTS.md](agent/docs/design/TESTS.md) |
| Cross-cutting refactor / unsure | This doc → [OVERVIEW.md](agent/docs/design/OVERVIEW.md) → [SKILLS/CODE-LAYOUT.md](SKILLS/CODE-LAYOUT.md) → **ask the user** |

---

## 7 — Worked examples

Concrete walk-throughs of how the decision tree resolves for real tasks.

### Example A — "Filter the kind palette by publisher org"

1. Q4 (REST) → the kind palette is the `kinds` search scope at `/api/v1/search?scope=kinds`. Handler is [`crates/transport-rest/src/search.rs`](agent/crates/transport-rest/src/search.rs) and stays thin — real work goes in [`crates/graph/src/kinds/`](agent/crates/graph/src/kinds/). Add a `QuerySchema` field per [QUERY-LANG.md](agent/docs/design/QUERY-LANG.md), add `org` as a derived `KindDto` field.
2. Q5 (React hook) → `useKinds()` already exists in `ui-core`. Extend it to accept `ListKindsOptions`. → `ui-core/src/hooks/useAgent.ts`.
3. Q4 also → the TS / Rust clients wrap `/search?scope=kinds` ergonomically — add the new option to `ListKindsOptions` in both `agent-client-ts/src/domain/kinds.ts` and `agent-client-rs/src/kinds.rs`.
4. Q6/Q7 — palette UI. The visual primitives (`Select`, `Badge`) are in `ui-kit`; the grouping / filter logic goes in `ui-core`'s `FlowSidebar`. Studio consumes it unchanged.
5. Q9 — CLI — add `--filter` and `--sort` to `agent kinds list` in `transport-cli`, delegating to the new `agent-client-rs` method.

Result: one change cleanly lands in five repos, each touching only what it owns. Nothing in `ui-kit`, nothing in `studio` beyond the unchanged import chain, nothing in `transport-rest` beyond the field being forwarded by the thin dispatcher.

### Example B — "Add a Rhai Function node"

1. Q2 (built-in kind) → new crate `agent/crates/domain-function` with the manifest, behavior, and conversion helpers. Register in `apps/agent/src/main.rs`.
2. No client, client-ts, or UI change needed — the kind is discovered via `/api/v1/search?scope=kinds` which already emits arbitrary kinds.
3. Studio's palette picks it up automatically because `ui-core`'s `useKinds()` fetches from the endpoint.

Result: one new crate in `agent/`, zero edits in the UI stack. The modularity rule pays off — shipping a new node type requires no change to the UI repos.

### Example C — "Keep transport clean: the placement-check refactor"

A recent mistake, because this rule is the easiest to forget.

**What happened.** I added `placeable_under=<path>` to the kind palette to let the Studio filter to placement-admissible kinds. The handler needed to know "can candidate kind X live under parent kind Y?" — and that check was inlined right there in the transport layer.

**What was wrong.** That check is *domain logic* — it mirrors `graph::GraphStore::create_child`'s pre-mutation validation. By copy-pasting it into the transport layer I created two sources of truth. And they silently drifted: `graph` added an `isAnywhere` facet bypass; my transport copy didn't. Studio's palette quietly rejected kinds the graph would have accepted.

**What the decision tree should have caught.** Q4 says the route goes in `transport-rest`, but it says nothing about logic that the route *uses*. The "smoke test" in Rule I above: *if I swap REST for gRPC, how much of this file changes?* Answer: the containment check would have to travel with the route, which is a sign it's in the wrong layer.

**The fix.** Extracted [`graph::placement_allowed(parent_kind, parent_manifest, candidate) -> bool`](agent/crates/graph/src/placement.rs) — one pure function with its own tests. Both `GraphStore::create_child` (5 lines: "if `!placement_allowed(..)` → `PlacementRejected`") and [`graph::KindsScope`](agent/crates/graph/src/kinds/scope.rs) (one line: `.retain(|c| placement_allowed(...))`) call it. Missing `isAnywhere` carve-out now fixed in one place, not two. The subsequent migration to `/api/v1/search` kept this structure intact — the scope carries the logic, the transport handler is a dispatcher.

**Takeaway when writing a handler.** If you're typing a loop, a match, or a multi-step predicate inside `transport-*`, stop. That code belongs in `graph` / `domain-*` / a shared crate. The handler calls it — that's all.

### Example D — "Add an MQTT block"

1. Q3 (pluggable block) → new dir `blocks/com.listo.mqtt-client/` with `block.yaml`, `Cargo.toml`, `kinds/*.yaml`, `process/src/main.rs`, `ui-src/` for the MF panel.
2. The process binary depends on `listo-blocks-sdk` (published) — **not** the agent. The UI panel depends on `@listo/block-ui-sdk` (published) — **not** `@listo/ui-core` directly.
3. If the SDK is missing a capability (async slot-event back-channel), extend `agent-sdk` first, bump its version, and only then wire the block.

Result: a block that a third party could ship identically. The boundary between "platform" and "extension" is real.

---

## 8 — Commit etiquette

- Commit only when the user explicitly asks.
- Never amend; create a new commit.
- Never skip hooks (`--no-verify`) unless the user explicitly says so.
- Never force-push to `main`.
- Commit message focuses on **why**, not what. One or two sentences.

---

## 9 — What to do when stuck

Ask. Don't guess at:

- **Node model decisions** — Rule A / Rule B have knock-on effects that ripple for months.
- **Layer placement** — getting Q5/Q6 wrong (hook in ui-kit, primitive in ui-core) is a silent leak that accumulates.
- **Contract surface changes** — a breaking change in `spi` cascades to every consumer; flag before editing.

One sentence of "which of these two did you want?" beats two hours of refactoring the wrong direction.

---

## One-line summary

**Start here → pick the right repo via the decision tree → load language skills → write it → run `mani run build --all` → tests and commit when asked. Modular libraries are the product, not an implementation detail.**
