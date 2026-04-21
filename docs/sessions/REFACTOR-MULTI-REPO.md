# Refactor Plan — Multi-Repo Extraction

Long-term repo layout under `github.com/listo-ai`. Not production yet — breaking changes are free. The goal: anyone can build their own frontend, their own blocks, or their own integrations by depending on published libraries, not by forking the monorepo.

---

## Target repo map

```
github.com/listo-ai/
├── contracts               # Rust — wire types & schemas (spi, ui-ir) — the root of all deps
├── agent                   # Rust — the core platform (engine, graph, transports, agent binary)
├── agent-sdk               # Rust — block-author tools (blocks-sdk, block-client, block-domain)
├── agent-client-rs         # Rust — HTTP client library (standalone, publishable to crates.io)
├── agent-client-ts         # TypeScript — HTTP client library (standalone, publishable to npm)
├── ui-kit                  # TypeScript/React — Shadcn component library + design tokens
├── ui-core                 # TypeScript/React — reusable logic: auth, SDUI renderer, stores, hooks
├── studio                  # TypeScript/React — the Studio app (consumes ui-kit + ui-core)
├── block-ui-sdk            # TypeScript/React — hooks & helpers for block MF bundles
├── blocks                  # Example/reference blocks (com.acme.hello, com.acme.project, etc.)
├── agent-cli               # Rust — MCP bootstrap CLI (already exists)
└── docs                    # Shared docs, design specs (optional — can stay in `agent`)
```

---

## Starting point — the current `us` monorepo becomes `listo-ai/agent`

The existing monorepo at `/home/user/code/rust/us` is **not abandoned**. It is the future `listo-ai/agent` repo. Git history is preserved.

**Mechanics:**
1. For each extracted subdir (`clients/ts`, `clients/rs`, `frontend`, `blocks`, `crates/spi`, `crates/ui-ir`, `crates/blocks-sdk*`, etc.), use `git subtree split` to create a history-preserving branch, push that to the new repo, then delete the subdir from `us`.
2. Once all extractions are done, rename `us` → `agent` locally (`/home/user/code/listo-repos/agent`), change the `origin` remote to `https://github.com/listo-ai/agent`, and force-push to the new repo.
3. Package name sweeps inside the remaining `us`/`agent` tree: `@sys/*` → `@listo/*`, bare Rust crate names → `listo-*` for any that get published.

**What "stays" in `us`/`agent`:** everything in the "What stays in `listo-ai/agent`" table below — engine, graph, domain-*, dashboard-*, data-*, transport-*, auth, messaging, audit, observability, config, query, blocks-host, ai-runner, apps/agent. These never move out; they just get their path-dep references to extracted crates swapped for git/crates.io deps.

**What leaves `us`:** clients/ts, clients/rs, frontend, blocks, crates/spi, crates/ui-ir, crates/blocks-sdk, crates/blocks-sdk-macros, plus the new crates created during the refactor (block-client, block-domain) that are born in `agent-sdk`.

---

## Section 1 — Frontend

### Problem

Today `frontend/` is a single private package (`@sys/studio`) that mixes:
- Shadcn UI primitives (`components/ui/`)
- Reusable logic (auth provider, SDUI renderer, graph store, hooks)
- Application pages specific to Studio
- Block integration (MF loader, registry)
- Features (page-builder, AI chat)

Anyone building an alternative frontend (mobile app, a lighter admin panel, a CLI dashboard) must fork or copy-paste.

### Target split — 4 packages

#### 1. `@listo/ui-kit` → repo `listo-ai/ui-kit`

**What it contains:**
- All `components/ui/*.tsx` (Shadcn primitives: Button, Badge, Card, Dialog, Table, etc.)
- `main.css` design tokens (CSS custom properties, HSL convention)
- Tailwind preset/config for the token system
- `components.json` (shadcn config)

**What it does NOT contain:**
- Any React hooks
- Any business logic
- Any API calls
- Any store

**Package:** `@listo/ui-kit` on npm. Peer deps: React 19, tailwindcss 4.

**Why separate:** A team building a mobile-first admin UI, or a reporting dashboard, or a third-party integration screen can use the same visual language without pulling in the entire Studio.

---

#### 2. `@listo/ui-core` → repo `listo-ai/ui-core`

**What it contains (extracted from current `frontend/`):**

| Current location | Moves to | Package export |
|------------------|----------|----------------|
| `providers/auth.tsx` + `store/auth.ts` | `src/auth/` | `AuthProvider`, `useAuth`, `useAuthToken`, auth store |
| `sdui/*` (renderer, types, context, actions, subscriptions, patch, capability) | `src/sdui/` | `SduiRenderer`, `SduiProvider`, `useActionResponse`, `useSubscriptions`, `applyPatch`, types |
| `store/graph-store.ts` + `store/graph-hooks.ts` | `src/graph/` | `useGraphStore`, graph hooks |
| `providers/query.tsx` | `src/query/` | `QueryProvider` (React Query wrapper) |
| `store/flow.ts` | `src/flow/` | Flow state management |
| `store/blocks.ts` + `blocks/*` | `src/blocks/` | Block loader, registry, MF integration |
| `hooks/useAgent.ts` | `src/agent/` | `useAgent`, `useScopedAgent` |
| `lib/fleet/` | `src/fleet/` | `ScopeProvider`, `FleetScope`, fleet transport |
| `lib/agent/` | `src/agent/` | Agent connection helpers |
| `providers/theme.tsx` | `src/theme/` | `ThemeProvider`, `useTheme` |
| `store/presentation-store.ts` | `src/presentation/` | Presentation state |

**Package:** `@listo/ui-core` on npm. Deps: `@listo/agent-client` (the TS client), React 19, zustand, @tanstack/react-query. Peer dep on `@listo/ui-kit` (for SDUI renderer components that reference primitives).

**What it does NOT contain:**
- Page components
- Application routing
- Studio-specific features (page-builder, AI chat)
- Shadcn primitives (those are in ui-kit)

**Why separate:** This is the "brain" — any frontend (web, Tauri, Electron, even React Native with a different ui-kit) can use these hooks and providers to talk to the agent, render SDUI, manage auth, and subscribe to graph events.

---

#### 3. `@listo/block-ui-sdk` → repo `listo-ai/block-ui-sdk`

**What it contains:**
- `useAgentClient()` — hook returning pre-configured client from host context
- `useNode(path)`, `useSlot(path, slot)`, `useNodes(query)` — graph hooks
- `useAction(handler)` — SDUI action dispatch
- `useSubscription(subjects)` — SSE subscription + React Query invalidation
- `useNavigate()` — navigation within the host app (wraps react-router or equivalent)
- `useToast()` — display toast/notification from block code
- `useCapabilities()` — query host capabilities / permission gates
- `useI18n(namespace)` — block-scoped translation hook (delegates to host's Fluent bundle)
- `BlockShell` — layout wrapper for block panels (loading/error boundary built-in)
- `BlockErrorBoundary` — catch-all error boundary with structured fallback UI
- `registerBlockComponent(id, component)` — custom renderer registration
- `NodeLink`, `SlotBadge` — common block UI components
- Form helpers: `useFormValidation(schema)` — JSON Schema validation via the same `@rjsf` pipeline as SDUI forms

**Relationship to `@listo/ui-core`:** `block-ui-sdk` is a curated **re-export facade** over `ui-core` internals. Block authors import only from `@listo/block-ui-sdk`; they never reach into `@listo/ui-core` directly. If a block needs something not in this surface, that's a feature request on block-ui-sdk, not a reason to break layering.

**Package:** `@listo/block-ui-sdk` on npm. Deps: `@listo/ui-core` (bundled/re-exported subset), `@listo/agent-client`, React 19, @tanstack/react-query. Peer dep on `@listo/ui-kit`.

**Purpose:** Any block author (`com.acme.whatever`) depends on this to ship MF bundles. Thin, stable, versioned.

---

#### 4. `@listo/studio` → repo `listo-ai/studio`

**What remains — the application shell:**
- `App.tsx`, `router.tsx`, `bootstrap.tsx`
- `pages/*` (FlowsListPage, FlowsPage, BlocksPage, SettingsPage, PagesListPage)
- `features/*` (page-builder, global-ai-chat)
- `components/layout/` (Shell, Sidebar, Header)
- `components/AddChildNodeDialog.tsx`, `NodeAppearanceDialog.tsx`, etc.
- Rsbuild config, Tauri config, MF host config
- `src-tauri/` (Tauri Rust shell)

**Package:** `@listo/studio` (private, not published). Deps: `@listo/ui-kit`, `@listo/ui-core`, `@listo/agent-client`.

**Why separate:** Studio is *one* consumer of the core libraries. A customer could build `@acme/custom-dashboard` that imports `@listo/ui-core` and `@listo/ui-kit` and ignores Studio entirely.

---

### Frontend dependency graph

```
@listo/agent-client          ← standalone, zero React dep
    ↑
@listo/ui-kit                ← standalone, zero logic
    ↑
@listo/ui-core               ← imports agent-client + ui-kit
    ↑
@listo/block-ui-sdk          ← thin layer over ui-core for block authors
    ↑
@listo/studio                ← the app, imports everything
    ↑
blocks (MF remotes)         ← import block-ui-sdk only
```

---

## Section 2 — Blocks

### Current state

Blocks live at `blocks/` inside the monorepo. They're standalone Cargo crates (not workspace members) + optional MF UI bundles. Two examples exist: `com.acme.hello` and `com.acme.wasm-demo`.

### Target

#### `listo-ai/blocks` — reference blocks repo

A separate repo containing example/reference blocks. Each block is a directory:

```
listo-ai/blocks/
├── com.acme.hello/           # Existing — minimal UI-only block
├── com.acme.wasm-demo/       # Existing — Wasm-only block
├── com.acme.project/         # New — project management (from PROJECT-MANAGEMENT-BLOCK.md)
├── com.nube.modbus/          # Future — protocol block
├── com.nube.bacnet/          # Future — protocol block
└── README.md
```

**Each block has:**
- `block.yaml` (manifest)
- `Cargo.toml` (standalone, depends on published `@listo/blocks-sdk`)
- `kinds/*.yaml` (node kind manifests)
- `ui/` (optional MF bundle, depends on `@listo/block-ui-sdk`)
- `src/` (Rust handlers)

**Key change:** blocks depend on *published crates*, not path deps into the monorepo. This means:
- `blocks-sdk` is published to crates.io (or a private registry)
- `@listo/block-ui-sdk` is published to npm
- Block authors don't need the agent source code

Third-party blocks live in their own repos (e.g. `AcmeCorp/acme-blocks`) and follow the same structure.

---

## Section 3 — Rust

### Current state

Everything is one Cargo workspace with 43 crates. This works for development velocity but prevents:
- Publishing `blocks-sdk` to crates.io independently
- Publishing `agent-client` (Rust) independently
- Block authors depending on stable, versioned SDK crates without cloning the whole repo

### Target split

#### `listo-ai/contracts` — wire types & schemas (the dependency root)

The lowest-level repo. Everything depends on this; it depends on nothing internal.

```
listo-ai/contracts/
├── Cargo.toml (workspace)
├── spi/
│   ├── Cargo.toml          # publishable as `listo-spi`
│   └── src/                # Msg, KindManifest, NodeId, Facet, proto, JSON schemas
├── ui-ir/
│   ├── Cargo.toml          # publishable as `listo-ui-ir`
│   └── src/                # ComponentTree, Component enum, Action, IR_VERSION
└── codegen/
    ├── Cargo.toml          # internal build tool (not published)
    └── src/                # spi → TypeScript type generation (see M1 note below)
```

**Rule:** `spi` and `ui-ir` have **zero** deps on any other internal crate. Only third-party: `serde`, `schemars`, `prost`, `semver`. This makes them safe to publish and depend on from every direction.

**TS codegen:** `contracts/codegen/` generates TypeScript type definitions from `spi` types (Rust struct → Zod schema or TS interface). The generated output is committed to `listo-ai/agent-client-ts` as `src/generated/`. This eliminates hand-maintained type copies between Rust and TS — a single source of truth.

---

#### `listo-ai/agent` — the core platform (private/internal)

The main repo. Still a Cargo workspace, but smaller — only the crates needed to build and run the agent binary. Depends on `listo-spi` and `listo-ui-ir` as published crates (or git deps during development).

```
listo-ai/agent/
├── Cargo.toml (workspace)
├── crates/
│   ├── query/                  # RSQL → AST → SeaQuery
│   ├── auth/                   # JWT, RBAC
│   ├── messaging/              # NATS client, bus
│   ├── audit/                  # Audit events
│   ├── observability/          # Tracing, metrics
│   ├── config/                 # Config loading
│   ├── data-entities/          # Entity structs
│   ├── data-repos/             # Repo traits
│   ├── data-sqlite/            # SQLite impl
│   ├── data-postgres/          # Postgres impl
│   ├── data-tsdb/              # Time-series
│   ├── domain-*/               # All domain crates (9 crates)
│   ├── dashboard-*/            # Dashboard crates (3 crates)
│   ├── transport-*/            # All transports (6 crates)
│   ├── graph/                  # Core graph substrate
│   ├── engine/                 # Flow engine
│   ├── blocks-host/            # Block supervisor
│   ├── ai-runner/              # AI runner
│   └── apps/agent/             # The binary
├── dev/                        # Dev configs
└── Makefile
```

**What moves OUT:**
- `crates/spi/` → `listo-ai/contracts`
- `crates/ui-ir/` → `listo-ai/contracts`
- `crates/blocks-sdk/` → `listo-ai/agent-sdk`
- `crates/blocks-sdk-macros/` → `listo-ai/agent-sdk`
- `clients/rs/` → `listo-ai/agent-client-rs`
- `clients/ts/` → `listo-ai/agent-client-ts`
- `frontend/` → `listo-ai/studio` (+ extracted packages)
- `blocks/` → `listo-ai/blocks`

---

#### `listo-ai/agent-sdk` — block-author tools

Contains only the crates block authors use directly. Does **not** contain `spi` or `ui-ir` — those live in `listo-ai/contracts` (the single source of truth for wire types).

```
listo-ai/agent-sdk/
├── Cargo.toml (workspace)
├── blocks-sdk/
│   ├── Cargo.toml          # publishable to crates.io as `listo-blocks-sdk`
│   └── src/
├── blocks-sdk-macros/
│   ├── Cargo.toml          # publishable as `listo-blocks-sdk-macros`
│   └── src/
├── block-client/
│   ├── Cargo.toml          # publishable as `listo-block-client`
│   └── src/                # BlockContext, ActionResult, view builder, test harness
└── block-domain/
    ├── Cargo.toml          # publishable as `listo-block-domain`
    └── src/                # StateMachine, Prioritised, AssignmentSet, SlotHelpers
```

**Dependency direction:**
```
contracts (listo-spi, listo-ui-ir)           ← the root, owned by listo-ai/contracts
    ↑
agent-sdk (blocks-sdk, block-client, block-domain)  ← depends on published contracts
    ↑
blocks (com.acme.*)                        ← depends on published SDK + contracts
    ↓ loaded at runtime
agent                                      ← depends on published contracts
```

No duplication of `spi` or `ui-ir`. One home, one version, consumed everywhere as a crates.io/git dependency.

---

#### `listo-ai/agent-client-rs` — Rust HTTP client

```
listo-ai/agent-client-rs/
├── Cargo.toml              # publishable as `listo-agent-client`
├── src/
│   ├── lib.rs
│   ├── nodes.rs
│   ├── slots.rs
│   ├── flows.rs
│   ├── ui.rs
│   └── ...
└── tests/
```

Depends on `listo-spi` (for types like `Msg`, `NodeSnapshot`). Zero dependency on the agent's internals.

---

#### `listo-ai/agent-client-ts` — TypeScript HTTP client

```
listo-ai/agent-client-ts/
├── package.json            # publishable as `@listo/agent-client`
├── src/
│   ├── index.ts
│   ├── client.ts
│   ├── domain/
│   │   ├── nodes.ts
│   │   ├── slots.ts
│   │   ├── ui.ts
│   │   └── ...
│   ├── transport/
│   └── schemas/
└── tests/
```

Already well-structured. Move as-is, rename from `@sys/agent-client` to `@listo/agent-client`.

---

## Migration order

Not everything moves at once. Sequence optimised for "unblocks others first":

| Phase | What moves | Status |
|-------|-----------|--------|
| **M1** | `crates/spi/` + `crates/ui-ir/` → `listo-ai/contracts` | ✅ Done — pushed to GitHub |
| **M2** | `clients/ts/` → `listo-ai/agent-client-ts` | ✅ Done — pushed to GitHub |
| **M3** | `crates/blocks-sdk*` + scaffold `block-client` + `block-domain` → `listo-ai/agent-sdk` | ✅ Done — pushed to GitHub |
| **M4** | `clients/rs/` → `listo-ai/agent-client-rs` | ✅ Done — pushed to GitHub |
| **M5** | Extract `@listo/ui-kit` from `frontend/src/components/ui/` | ✅ Done — pushed to GitHub |
| **M6** | Extract `@listo/ui-core` from `frontend/src/{sdui,store,hooks,providers,lib}` | ✅ Done — pushed to GitHub |
| **M7** | Extract `@listo/block-ui-sdk` | ✅ Done — pushed to GitHub |
| **M8** | Move remaining `frontend/` → `listo-ai/studio` | ✅ Done — pushed to GitHub |
| **M9** | Move `blocks/` → `listo-ai/blocks` | ✅ Done — pushed to GitHub |
| **M10** | Rename `us` → `listo-ai/agent` (change remote, push to new GitHub repo) | ✅ Done — pushed to `listo-ai/agent` |
| **M11** | Commit uncommitted fixes in `us` (`folder.yaml`, `station.yaml`, `pnpm-lock.yaml`) | ✅ Done — committed + pushed |
| **M12** | Switch all Rust path deps → git deps (or crates.io) | ✅ Done — git deps in `us/Cargo.toml` |
| **M13** | Switch all TS `workspace:*` deps → npm version ranges | ✅ Done — `^0.1.0` in ui-core, block-ui-sdk, studio |
| **M14** | Push `ui-kit` tsc-alias build fix to GitHub | ✅ Done — pushed to `listo-ai/ui-kit` |
| **M15** | Fix `agent-sdk` back-reference to `us/crates/transport-grpc` | ✅ Done — git dep in `agent-sdk/Cargo.toml` |
| **M16** | Set up GitHub Actions CI on all repos | ✅ Done — CI on all 8 repos |
| **M17** | npm/crates.io publish prep (`prepublishOnly`, changelogs, `integration.lock`) | ❌ Not started |

---

## Current state (April 2026)

### What exists on GitHub under `listo-ai/`

| Repo | Package name | Pushed | Builds standalone |
|------|-------------|--------|------------------|
| `listo-ai/contracts` | `listo-spi`, `listo-ui-ir` | ✅ | ✅ (`cargo check`) |
| `listo-ai/agent-client-ts` | `@listo/agent-client` | ✅ | ✅ (`tsc`) |
| `listo-ai/agent-sdk` | `listo-blocks-sdk`, `listo-blocks-sdk-macros` | ✅ | ✅ path dep to contracts |
| `listo-ai/agent-client-rs` | `listo-agent-client` | ✅ | ✅ concrete versions |
| `listo-ai/ui-kit` | `@listo/ui-kit` | ✅ | ✅ (`tsc + tsc-alias`) |
| `listo-ai/ui-core` | `@listo/ui-core` | ✅ | ✅ (`tsc + tsc-alias`) |
| `listo-ai/block-ui-sdk` | `@listo/block-ui-sdk` | ✅ | ✅ (`tsc`) |
| `listo-ai/studio` | `@listo/studio` | ✅ | ✅ (Rsbuild dev server runs) |
| `listo-ai/blocks` | example blocks | ✅ | path dep to agent-sdk |
| `listo-ai/agent` | the backend | ❌ **empty** | — |

### What `us` (`NubeDev/us`) currently is

- All Rust backend crates remain in `us` with their original structure
- `frontend/`, `blocks/`, `clients/ts`, `clients/rs`, `crates/spi`, `crates/ui-ir`, `crates/blocks-sdk*` have been `git rm`'d
- `pnpm-workspace.yaml` wires `../../listo-repos/*` packages via `workspace:*` — **dev-only**, only works on this machine
- `Cargo.toml` wires extracted crates via `path = "../../listo-repos/..."` — **dev-only**, only works on this machine
- `crates/graph/manifests/folder.yaml`, `crates/graph/manifests/station.yaml`, `pnpm-lock.yaml` — committed and pushed (M11 ✅)
- Origin remote is now `https://github.com/listo-ai/agent` — history pushed ✅

### Known broken cross-repo references

| Location | Problem |
|----------|---------|
| `listo-ai/ui-core`, `block-ui-sdk`, `studio` `package.json` | `@listo/*` deps set to `^0.1.0` — packages not yet on npm; CI uses bootstrap-clone workaround until M17 |

---

## Pending work — prioritised

### P0 — finish the repo migration (this machine → GitHub)

~~M11~~, ~~M10~~, ~~M14~~ ✅ All three done — `us` is live on `listo-ai/agent`, `ui-kit` build fix is pushed, graph manifests committed.

### P1 — make deps portable

~~M12~~, ~~M13~~, ~~M15~~ ✅ All done — Rust path deps are now git deps; TS `workspace:*` replaced with `^0.1.0`; `agent-sdk` transport-grpc back-ref fixed.

**Note:** `@listo/*` npm packages are still unpublished. TS CI for `ui-core` and `block-ui-sdk` uses a bootstrap-clone workaround (clones sibling repos in CI) until M17.

### P2 — CI

~~M16~~ ✅ Done — GitHub Actions CI on all 8 repos (contracts, agent-sdk, agent-client-rs, agent, agent-client-ts, ui-kit, ui-core, block-ui-sdk).

### P1 — make deps portable (anyone can clone and build)

4. **M12** Switch Rust path deps in `us/Cargo.toml` to git deps:
   ```toml
   spi     = { git = "https://github.com/listo-ai/contracts", package = "listo-spi" }
   ui-ir   = { git = "https://github.com/listo-ai/contracts", package = "listo-ui-ir" }
   blocks-sdk = { git = "https://github.com/listo-ai/agent-sdk", package = "listo-blocks-sdk" }
   blocks-sdk-macros = { git = "https://github.com/listo-ai/agent-sdk", package = "listo-blocks-sdk-macros" }
   agent-client = { git = "https://github.com/listo-ai/agent-client-rs", package = "listo-agent-client" }
   ```

5. **M13** Switch TS `workspace:*` deps in extracted packages to npm version ranges. Each package's `package.json` needs:
   - `@listo/agent-client: "^0.1.0"` instead of `workspace:*`
   - Requires publishing to npm (or GitHub Packages) first, or using `file:` paths in a released tarball

6. **M15** Fix `agent-sdk/Cargo.toml` `transport-grpc` back-reference — either remove the `process` feature's hard dep or make it optional and document it.

### P2 — CI

7. **M16** Add GitHub Actions to each repo (start with `contracts` + `agent`):
   - `contracts`: `cargo check`, `cargo test`
   - `agent-sdk`: `cargo check --all-features`
   - `agent-client-ts`: `pnpm build`, `pnpm test`
   - `ui-kit`, `ui-core`, `block-ui-sdk`: `pnpm build`
   - `studio`: `pnpm build:web`
   - `agent`: `cargo check`, `cargo test`, `cargo nextest`

### P3 — publish to registries

8. **M17** Publishing prep:
   - Tag `v0.1.0` on `contracts`, `agent-sdk`, `agent-client-rs`, `agent-client-ts`
   - `cargo publish` for `listo-spi`, `listo-ui-ir`, `listo-blocks-sdk`, `listo-blocks-sdk-macros`, `listo-agent-client`
   - `npm publish` for `@listo/agent-client`, `@listo/ui-kit`, `@listo/ui-core`, `@listo/block-ui-sdk`
   - Switch all deps from git/workspace to published versions
   - Add `integration.lock` to `agent`

---

## Next-session prompt

```
Context: Rust+React monorepo multi-repo extraction into `github.com/listo-ai/`
is complete. All 17 migration steps M1–M16 are DONE. All repos are live on
GitHub, deps are portable (git deps / ^0.1.0 ranges), CI is in place.

What remains is M17 — publishing to registries (P3).

Repo layout on disk:
- `/home/user/code/rust/us` — the backend monorepo, origin = listo-ai/agent
- `/home/user/code/listo-repos/{contracts,agent-client-ts,agent-sdk,agent-client-rs,ui-kit,ui-core,block-ui-sdk,studio,blocks}` — all extracted repos, all on GitHub with CI

M17 tasks (P3 — publish to registries):
1. Tag `v0.1.0` on contracts, agent-sdk, agent-client-rs, agent-client-ts
2. `cargo publish` for listo-spi, listo-ui-ir, listo-blocks-sdk,
   listo-blocks-sdk-macros, listo-block-client, listo-block-domain, listo-agent-client
3. `npm publish` for @listo/agent-client, @listo/ui-kit, @listo/ui-core,
   @listo/block-ui-sdk
4. Once published, remove bootstrap-clone workaround from ui-core and
   block-ui-sdk CI workflows (replace with plain `pnpm install --frozen-lockfile`)
5. Switch Cargo git deps → crates.io version deps
6. Add `integration.lock` to listo-ai/agent (pinned version matrix)

Plan doc: `/home/user/code/rust/us/docs/sessions/REFACTOR-MULTI-REPO.md`
  (see P3 / M17 section for full task list)

Tech stack: Rust/Cargo workspaces, pnpm workspaces, React 19, Rsbuild, tsc,
tsc-alias, GitHub CLI (`gh`), cargo-release, npm publish.
Rust toolchain 1.90, Node v22, pnpm v10.
```

---

**Why M1 is contracts, not the TS client:** The TS client's types mirror `spi`. Extracting the TS client first creates a window where TS types are hand-maintained copies with guaranteed drift. By extracting contracts first and shipping a codegen step, M2 (TS client) gets generated types from day 1 — one source of truth, zero hand sync.

---

## Package naming

| Current name | New package name | Registry | Repo |
|--------------|-----------------|----------|------|
| `@sys/agent-client` | `@listo/agent-client` | npm | `agent-client-ts` |
| `@sys/studio` | `@listo/studio` (private) | — | `studio` |
| — (new) | `@listo/ui-kit` | npm | `ui-kit` |
| — (new) | `@listo/ui-core` | npm | `ui-core` |
| — (new) | `@listo/block-ui-sdk` | npm | `block-ui-sdk` |
| `spi` (path) | `listo-spi` | crates.io | `contracts` |
| `ui-ir` (path) | `listo-ui-ir` | crates.io | `contracts` |
| `blocks-sdk` (path) | `listo-blocks-sdk` | crates.io | `agent-sdk` |
| `blocks-sdk-macros` (path) | `listo-blocks-sdk-macros` | crates.io | `agent-sdk` |
| `agent-client` (path) | `listo-agent-client` | crates.io | `agent-client-rs` |
| — (new) | `listo-block-client` | crates.io | `agent-sdk` |
| — (new) | `listo-block-domain` | crates.io | `agent-sdk` |

---

## Frontend library detail

### `@listo/ui-kit` — what gets extracted

```
src/
  components/
    button.tsx
    badge.tsx
    card.tsx
    dialog.tsx
    dropdown-menu.tsx
    input.tsx
    label.tsx
    scroll-area.tsx
    select.tsx
    separator.tsx
    sheet.tsx
    skeleton.tsx
    table.tsx
    tabs.tsx
    tooltip.tsx
    context-menu.tsx
    popover.tsx
    index.ts              # barrel re-export
  tokens/
    main.css              # design tokens (HSL custom properties, dark mode)
  tailwind-preset.ts      # shared tailwind config
  index.ts                # package entry
```

**Zero logic. Zero hooks. Zero state. Just styled primitives + tokens.**

---

### `@listo/ui-core` — what gets extracted

```
src/
  auth/
    AuthProvider.tsx       # OIDC provider wrapping oidc-client-ts
    useAuth.ts            # hook: login, logout, token, user
    store.ts              # zustand auth slice
    index.ts
  sdui/
    Renderer.tsx          # Component switch dispatcher
    SduiProvider.tsx      # Context for SDUI state
    SduiPage.tsx          # Page-level host
    SduiRenderPage.tsx    # Render wrapper for kind views
    useActionResponse.ts  # Action dispatch hook
    useSubscriptions.ts   # SSE/NATS subscription + React Query invalidation
    applyPatch.ts         # Optimistic/server patch application
    capability.ts         # IR version handshake
    types.ts              # IR type definitions (mirrors ui-ir)
    context.tsx
    field.ts
    components/           # Built-in SDUI component implementations
      Page.tsx
      Row.tsx
      Table.tsx
      Form.tsx
      Chart.tsx
      ...
    index.ts
  graph/
    store.ts              # Graph zustand store
    hooks.ts              # useGraphStore, useNodeSnapshot, etc.
    index.ts
  flow/
    store.ts              # Flow state
    index.ts
  blocks/
    loader.ts             # MF dynamic import + shared singleton negotiation
    registry.ts           # Block component registry
    types.ts
    index.ts
  agent/
    useAgent.ts           # Hook returning configured AgentClient
    useScopedAgent.ts     # Remote-scope variant
    index.ts
  fleet/
    ScopeProvider.tsx     # Fleet scope context
    useScope.ts
    index.ts
  presentation/
    store.ts              # Presentation state (nav, UI preferences)
    index.ts
  theme/
    ThemeProvider.tsx
    useTheme.ts
    index.ts
  query/
    QueryProvider.tsx     # React Query client setup
    index.ts
  index.ts                # Package barrel
```

**This is the "portable brain" — any React app importing this gets full agent integration.**

---

### `@listo/block-ui-sdk` — surface area

```
src/
  hooks/
    useAgentClient.ts     # Returns client from host MF shared context
    useNode.ts            # Single node + live subscription
    useSlot.ts            # Single slot + live updates
    useNodes.ts           # Query/list nodes with pagination
    useAction.ts          # Fire SDUI action, handle response
    useSubscription.ts    # SSE subscription scoped to subjects
  components/
    BlockShell.tsx        # Panel layout wrapper
    NodeLink.tsx          # Clickable node reference
    SlotBadge.tsx         # Badge driven by slot value
  registration.ts         # registerBlockComponent(id, component)
  index.ts
```

**Stable, versioned, semver'd. Block authors depend on this and nothing else.**

---

## Rust library detail

### `listo-spi` — the public contract (lives in `listo-ai/contracts`)

Extracted from current `crates/spi/`. Contains:
- `Msg`, `MessageId` (wire envelope)
- `KindId`, `KindManifest`, `SlotSchema`, `SlotRole`, `Cardinality`
- `NodeId`, `NodePath`, `Facet`, `FacetSet`
- `CapabilityId`, `CapabilityVersion`, `SemverRange`
- `block.proto` (gRPC schema for process blocks)
- `flow.schema.json`, `node.schema.json`

**Rule:** Only types and schemas. No implementations. No runtime. Zero deps on any internal crate.

---

### `listo-ui-ir` — component tree types (lives in `listo-ai/contracts`)

Extracted from current `crates/ui-ir/`. Contains:
- `ComponentTree`, `Component` enum (25+ variants)
- `Action`, `TableSource`, `ChartSource`, etc.
- JSON Schema generation via `schemars`
- `IR_VERSION` constant

**Rule:** Types + schema + validation only. No resolver logic. Depends only on `listo-spi` (same repo).

---

### `listo-blocks-sdk` — block author SDK

Extracted from current `crates/blocks-sdk/`. Contains:
- `NodeBehavior` trait
- `NodeKind` trait + derive macro
- `WasmPlugin` trait + `export_plugin!` macro
- `NodeCtx`, `EmitSink`, `GraphAccess`
- Process-block runner (`run_process_plugin()`)

Depends on: `listo-spi`.

---

### `listo-block-client` — action handler helpers (NEW)

From [PROJECT-MANAGEMENT-BLOCK.md](PROJECT-MANAGEMENT-BLOCK.md) P0.2:
- `BlockContext` (env-driven config)
- `ActionResult` (Toast, Navigate, FullRender, FormErrors, Patch, Download, None)
- ComponentTree builder helpers
- Test harness (spin up agent, seed nodes, assert SDUI)

Depends on: `listo-agent-client`, `listo-spi`, `listo-ui-ir`.

---

### `listo-block-domain` — reusable domain patterns (NEW)

From [PROJECT-MANAGEMENT-BLOCK.md](PROJECT-MANAGEMENT-BLOCK.md) P0.3:
- `StateMachine<S>` (typed transitions with error reporting)
- `Prioritised<T>` (ranked items)
- `AssignmentSet` (multi-role node assignments)
- `TagFilter` (expression over tags)
- `SlotHelpers` (dot-path JSON walks on `Msg`)
- `Auditable` trait

Depends on: `listo-spi`, `listo-audit` (trait only — or inline a minimal trait).

---

### `listo-agent-client` — Rust HTTP client

Extracted from current `clients/rs/`. Full `AgentClient` facade:
- `.nodes()`, `.slots()`, `.flows()`, `.ui()`, `.blocks()`, `.auth()`, etc.
- Async, reqwest-based, rustls

Depends on: `listo-spi` (for request/response types).

---

## What stays in `listo-ai/agent`

Everything that is **platform-internal** — not consumed by block authors or frontend developers:

| Crate | Why it stays |
|-------|-------------|
| `graph` | Core substrate — internal, not a public API |
| `engine` | Flow execution — internal |
| `data-*` (entities, repos, sqlite, postgres, tsdb) | Persistence — internal |
| `domain-*` (all 9) | Business logic — internal |
| `dashboard-*` (nodes, runtime, transport) | SDUI resolver internals — internal |
| `transport-*` (rest, grpc, nats, zenoh, cli, mcp) | Wire surfaces — internal |
| `auth` | JWT verification — internal (different from `ui-core`'s OIDC client) |
| `messaging` | NATS client — internal |
| `audit`, `observability`, `config`, `query` | Infrastructure — internal |
| `blocks-host` | Block supervisor — internal |
| `ai-runner` | AI execution — internal |
| `apps/agent` | The binary — internal |

These crates consume the published contract types (`listo-spi`, `listo-ui-ir` from `listo-ai/contracts`) as crates.io or git dependencies.

---

## Cross-repo dependency graph

```
                    listo-ai/contracts
                   ┌────────────────────┐
                   │ listo-spi (crates.io)│
                   │ listo-ui-ir          │
                   └────────┬───────────┘
                            │
          ┌─────────────────┼──────────────────┐
          ↓                 ↓                  ↓
  listo-ai/agent-sdk    listo-ai/agent     listo-ai/agent-client-rs
  (blocks-sdk,         (graph, engine,   (listo-agent-client)
   block-client,        transports,             │
   block-domain)        domain-*, …)            │
          │                                     │
          ↓                                     ↓
  listo-ai/blocks ─── loaded at runtime by ── listo-ai/agent

─── TypeScript side ───

                    listo-ai/contracts
                   ┌────────────────────┐
                   │ codegen → TS types  │
                   └────────┬───────────┘
                            ↓
              listo-ai/agent-client-ts
              (@listo/agent-client, npm)
                            │
              ┌─────────────┼─────────────┐
              ↓             ↓             ↓
       @listo/ui-kit   @listo/ui-core   @listo/block-ui-sdk
              │             │             │
              └──────┬──────┘             │
                     ↓                    ↓
              listo-ai/studio        block MF bundles
```

---

## Versioning strategy

| Package | Versioning |
|---------|-----------|
| `listo-spi` | Semver, additive only within major. Proto fields add-only. |
| `listo-ui-ir` | Semver, tied to `IR_VERSION`. New components = minor bump. |
| `listo-blocks-sdk` | Semver, strict stability promise. Default impls on new trait methods. |
| `listo-block-client` | Semver, follows agent API version. |
| `listo-block-domain` | Semver, independent. |
| `@listo/agent-client` | Semver, tied to REST API version. |
| `@listo/ui-kit` | Semver, visual-only changes = patch. New components = minor. |
| `@listo/ui-core` | Semver, tracks IR version + agent API version. |
| `@listo/block-ui-sdk` | Semver, strict stability promise for block authors. |

**Rule:** `listo-spi` and `@listo/agent-client` drive the ecosystem. A breaking change there ripples everywhere — avoid except at major version boundaries.

---

## Immediate next steps

These don't require the full split. They can be done incrementally inside the current monorepo, making the eventual extraction trivial:

| # | Action | Effort | Unblocks |
|---|--------|--------|----------|
| 1 | Ensure `crates/spi/` and `crates/ui-ir/` have zero deps on other internal crates | 1 day | Independent publishing — prerequisite for everything else |
| 2 | Build `contracts/codegen` tool: Rust types → TS interfaces/Zod schemas | 3 days | Eliminates TS↔Rust type drift permanently |
| 3 | Move `frontend/src/components/ui/` into `clients/ui-kit/` package (pnpm workspace) | 1 day | UI reuse |
| 4 | Extract `@listo/ui-core` — untangle internal import cycles, define module boundaries, build as standalone | 1.5–2 weeks | Alternative frontends (see "Known risks" §4 below) |
| 5 | Create `clients/block-ui/` package with hooks extracted from `ui-core` (re-export facade) | 2 days | Block authors |
| 6 | Create `crates/block-client/` and `crates/block-domain/` | 2 days | Example block |
| 7 | Build `com.acme.project` example block using only the extracted packages | 3 days | Validates the split |

Once these are working as separate pnpm/cargo workspace members, the git-repo split is mechanical: `git filter-branch` / `git subtree split`, update `Cargo.toml`/`package.json` to point at published versions instead of `workspace:*` / `path = "..."`.

---

## Known risks & mitigations

### Risk 4 — ui-core circular imports

**Problem:** The modules proposed for `@listo/ui-core` (sdui, graph, blocks, agent, fleet, query, auth, presentation, theme) currently import each other cyclically inside `frontend/`. SDUI reads graph store; graph store uses agent client; blocks loader uses SDUI registry; auth wraps query. Extracting the whole bundle into a standalone package requires untangling these into a strict acyclic layer order.

**Impact:** This is not a 3-day task. Realistic estimate: **1.5–2 weeks** for a clean extraction that builds and passes tests in isolation.

**Mitigation plan:**
1. Map every cross-module import inside `frontend/src/` into a dependency DAG.
2. Identify cycles (likely: blocks ↔ sdui, graph ↔ agent).
3. Break cycles by introducing explicit dependency-injection points (context providers, callback props, or a thin `core-internal` barrel that owns the shared singleton setup).
4. Enforce the layer order with ESLint `import/no-restricted-paths` rules before extracting.
5. Extract only once the package builds with `tsc --noEmit` in isolation.

---

### Risk 5 — MF singleton version skew

**Problem:** Studio ships with `ui-core@X`. A block MF bundle built 6 months later uses `block-ui-sdk@Y`, which peer-deps `ui-kit@Z`. At MF runtime, if React, React Query, or zustand singletons don't align, you get two React copies, stale caches, or silent breakage.

**Mitigation — host-injected singletons contract:**

1. **Studio (the MF host) declares a `shared` singleton manifest** in its Rsbuild/rspack MF config. This is the authoritative list of packages that MUST be shared as singletons:
   ```js
   shared: {
     react: { singleton: true, requiredVersion: "^19.0.0" },
     "react-dom": { singleton: true, requiredVersion: "^19.0.0" },
     zustand: { singleton: true, requiredVersion: "^5.0.0" },
     "@tanstack/react-query": { singleton: true, requiredVersion: "^5.0.0" },
     "@listo/agent-client": { singleton: true },
     "@listo/ui-kit": { singleton: true },
   }
   ```
2. **`@listo/block-ui-sdk` documents a compatibility matrix** in its README and exports a `COMPAT` constant:
   ```ts
   export const COMPAT = {
     minHostVersion: "0.4.0",    // minimum @listo/ui-core version in host
     react: "^19.0.0",
     zustand: "^5.0.0",
     reactQuery: "^5.0.0",
   };
   ```
3. **Block MF bundles declare the same deps as `eager: false` singletons** — they never bundle their own React. If the host doesn't provide a compatible version, MF throws at import time (fail-fast, not silent).
4. **CI for `listo-ai/blocks`** builds each block's MF bundle against the latest published Studio shared manifest and fails if version ranges don't overlap.

---

### Risk 6 — Integration testing across 9 repos

**Problem:** "Integration tests run in a separate CI workflow that pulls all repos" is insufficient. With 9 repos, "which commit of each repo was tested together" becomes release-blocking.

**Solution — pinned integration manifest + coordinator:**

1. **`listo-ai/agent` owns an `integration.lock` file:**
   ```toml
   [pins]
   contracts = "v0.3.1"           # git tag
   agent-sdk = "v0.2.0"
   agent-client-rs = "v0.2.0"
   agent-client-ts = "0.4.2"     # npm version
   ui-kit = "0.3.0"
   ui-core = "0.4.1"
   block-ui-sdk = "0.2.0"
   studio = "main@abc1234"       # git ref
   blocks = "main@def5678"
   ```
2. **A nightly (or on-push) CI workflow** in `listo-ai/agent` checks out all repos at their pinned versions, builds, and runs the full integration suite (agent binary + blocks loaded + Studio E2E via Playwright).
3. **Renovate / release-please** bumps pins automatically when downstream repos tag a release. The integration CI gates the PR — if the combination breaks, the pin update is rejected.
4. **Any release of `listo-spi` or `@listo/agent-client`** triggers the integration workflow across all repos. These are the ecosystem's two critical contracts.

This gives a clear answer to "what was tested together" for every release.

---

### Risk 7 — block-ui-sdk surface completeness

**Problem:** The original 10-hook surface would force block authors to reach into `@listo/ui-core` for common needs (form validation, toasts, navigation, permissions, error boundaries, i18n). That breaks the clean layering.

**Resolution:** `@listo/block-ui-sdk` is explicitly a **curated re-export facade** over `@listo/ui-core`. Its surface (expanded above in Section 1 §3) covers the full set of needs a block MF bundle has. The rule: if a block needs to `import` from `@listo/ui-core` directly, that's a bug in `block-ui-sdk`'s surface — file an issue, add the re-export.

---

## FAQ

**Q: Won't many repos slow down development?**
A: During active development, use git submodules or a workspace override pointing at local checkouts. The multi-repo structure is for *consumers* (block authors, alternative frontend builders). Core developers can use a meta-repo or just clone side-by-side.

**Q: Should `docs/` stay in agent or get its own repo?**
A: Keep in `agent` for now. The design docs are platform-internal. If they grow into public developer docs, extract then.

**Q: What about CI?**
A: Each repo gets its own unit/build CI. Integration testing is coordinated via `integration.lock` in the agent repo (see Risk 6 above). A nightly workflow checks out all repos at pinned versions and runs the full E2E suite.

**Q: Can I do M1 (extract contracts) today?**
A: Yes — step 1 in "Immediate next steps" is confirming `spi` and `ui-ir` have zero internal deps. Once verified, create the repo, move the two crates, update the monorepo's `Cargo.toml` to point at a git dep, and all existing code keeps building.

---

## One-line summary

**Split the monorepo into 10 focused repos under listo-ai/ so block authors depend on published SDKs, frontend developers pick from composable UI libraries, and the core platform stays private — not production yet, break freely, stabilise the public contract surfaces first.**
