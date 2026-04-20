# Studio UI — Migration Scope from `bizzy/frontend`

Source: `/home/user/code/go/bizzy/frontend` (Vite + React 19 + Tailwind v4 + Shadcn + `@json-render/*`).
Target: new Studio app under `/apps/studio/` in this repo, per [UI.md](UI.md).
Existing TS client: [`/clients/ts`](../../clients/ts) — `@sys/agent-client`, the headless REST client; the Studio consumes it, does **not** replace it.

This is a **selective port**, not a lift-and-shift. The Vite build is incompatible with our Module Federation requirement, so we rebuild the shell and harvest components.

---

## Goal #1 — Prove Module Federation actually works (everything else is secondary)

The whole block model in [UI.md](UI.md) is bet on **Rspack native Module Federation**: one host realm, third-party blocks loaded at runtime, **shared singletons for `react`, `react-dom`, and our service-registry context** so blocks don't ship duplicate React trees or lose Context across the boundary ([UI.md:182-186](UI.md#L182-L186)).

This is historically painful. Known failure modes we must reproduce and defeat before committing to the rest of the migration:

| Known MF footgun | What breaks | How we'll prove it's fixed |
|---|---|---|
| Duplicate React in host + remote | Invalid hook calls, Context isolation, silent perf cliffs | Single `React` instance assertion at runtime; `bundle-stats` check in CI |
| React Context across MF boundary | Service registry / theme / query client invisible in remote | End-to-end test: remote consumes host's service registry via `useContext` |
| Version drift on shared deps | Two Radix / Tailwind runtimes, style collisions, memory bloat | Strict `singleton: true, requiredVersion: 'x.y.z'` with failing build on mismatch |
| Eager vs lazy loading races | Remote loads before host shares initialized | Explicit `__webpack_init_sharing__` / `__webpack_share_scopes__` bootstrap in host entry |
| TanStack Query / Zustand / Router each having their own store | Two caches, two routers, two stores | Each added to `shared` as singleton; verified by a remote that reads host state |
| Types across federation | `@module-federation/typescript` drift, `any` creep | Remote publishes `.d.ts` bundle; host consumes via path alias; tsc in CI |
| Tailwind v4 + MF | Remote ships its own CSS, host theme tokens invisible | Single Tailwind build in host; remote only uses utility classes + CSS vars |
| Dev-mode HMR across MF | Works in prod, explodes in dev | Both `rsbuild dev` for host and remote, run concurrently, HMR proven |
| React 19 + MF maturity | Rspack MF examples lag React 19; edge cases in `use`/actions | Pin versions; upstream issue links tracked in [tests](../../crates/engine/tests) note |

**If Milestone 0 (below) fails, we stop the migration and rethink the block model.** No point porting design system work onto a foundation that doesn't hold.

## Goal #2 — Harvest reusable UI from bizzy

Shadcn primitives, Tailwind v4 tokens, layout patterns (allotment), generic hooks. Low-risk lift once the shell exists.

## Goal #3 — Wire the Studio to real backends

Consume [`clients/ts`](../../clients/ts) for REST, add `@connectrpc/connect-web` for gRPC-Web, `nats.ws` for live events, `oidc-client-ts` for Zitadel. None of this matters until Goal #1 is proven.

## Non-goals

- Preserving bizzy's routes, page structure, or domain model.
- Porting anything coupled to the Go backend API (`/my/apps`, `/my/tools`, `/users/me`).
- Supporting the old app in parallel.
- Replacing `@sys/agent-client` — the Studio imports it.

---

## Client packages we already have

[`/clients/ts`](../../clients/ts) (`@sys/agent-client`) is the headless REST client. Policy ([clients/README.md](../../clients/README.md)):

- Three version numbers (package, `REST_API_VERSION`, `REQUIRED_CAPABILITIES`).
- Must pass `/contracts/fixtures/` round-trip tests.
- No business logic in clients.

Studio implications:

1. Studio depends on `@sys/agent-client` as a workspace package; never re-implements REST calls inline.
2. The client is **not** federated — it's a normal dep. Federation is only for **blocks**, not for internal packages.
3. If we need Connect-RPC in addition to REST, add it alongside the TS client (new `/clients/ts-rpc` or a subpath export), not inside the Studio.
4. Bizzy's `src/lib/api.ts` is not ported — `@sys/agent-client` replaces it.

---

## Compatibility verdict

| Concern | bizzy/frontend | Studio requirement | Action |
|---|---|---|---|
| Bundler | Vite 8 | **Rsbuild + Rspack** (MF native support) | **Replace** |
| Block system | None | **Module Federation w/ singletons** | **Add — Goal #1** |
| Desktop shell | None | **Tauri 2** | **Add** |
| React | 19.2 | 19.x | Keep, pin as MF singleton |
| Styling | Tailwind v4 | Tailwind v4 | Keep config, host-only build |
| Components | Radix + Shadcn | Shadcn + Tailwind | Keep, pin Radix as singleton |
| Forms | `@json-render/*` | `@rjsf/core` ([UI.md:66](UI.md#L66)) | **Decision needed — see §Open questions** |
| Local state | Ad hoc (`use-store.ts`) | **Zustand** | **Add / rewrite stores**, singleton |
| Server state | TanStack Query 5 | TanStack Query | Keep, singleton |
| Router | react-router-dom 7 | Not pinned | Keep, singleton |
| REST transport | `fetch` via `lib/api.ts` | **`@sys/agent-client`** | **Replace with existing client** |
| RPC transport | None | `@connectrpc/connect-web` | **Add** |
| Live transport | None | `nats.ws` | **Add** |
| Auth | Custom (`use-auth.tsx`) | `oidc-client-ts` + PKCE | **Replace** |
| Canvas | None | **React Flow** | **Add (new work)** |
| Layout | `allotment` | Not specified | Keep |
| Theming | `use-theme.tsx` | Shadcn themes + block themes | Keep + extend |
| TypeScript | `~6.0.2` (suspicious) | Current 5.x (matches `/clients/ts`) | **Verify & pin to 5.x** |
| Lucide | `^1.8.0` (suspicious) | Current | **Verify & pin** |

---

## Port in three layers

### Layer A — Keep & port (low risk, high reuse)

| From bizzy | To Studio | Notes |
|---|---|---|
| `src/components/ui/*` (Shadcn primitives) | `apps/studio/src/components/ui/` | Straight copy; retune `components.json` paths |
| `index.css` + Tailwind v4 config | `apps/studio/src/index.css` | Host-only, design tokens preserved |
| `components/layout/*` | `apps/studio/src/layout/` | Rename to match Studio IA |
| `components/shared/*` | `apps/studio/src/components/shared/` | Triage — keep generic, drop app-specific |
| `hooks/use-mobile.ts`, `use-theme.tsx`, `use-command-picker.ts` | `apps/studio/src/hooks/` | Generic; port as-is |
| `lib/utils.ts` (`cn` etc.) | `apps/studio/src/lib/utils.ts` | Standard Shadcn util |
| `allotment` panel patterns | Studio workbench | Copy usage, not files |

### Layer B — Rewrite (must change)

| From bizzy | Why it can't be ported | Replacement |
|---|---|---|
| `vite.config.ts`, entry, HTML | Bundler change | New `rsbuild.config.ts` with `tauri` + `web` envs, MF block, shared singletons |
| `lib/api.ts` | REST against Go backend | `@sys/agent-client` from `/clients/ts` |
| `hooks/use-auth.tsx` | Bespoke | `oidc-client-ts` PKCE to Zitadel |
| `pages/*` | Domain-specific (apps store, workshop, chat) | New Studio pages: Flows, Dashboards, Blocks, Settings |
| `components/app-builder/*`, `workshop/*`, `live-preview/*`, `chat/*`, `store/*` | App-domain UI | Not ported |
| `hooks/use-my-apps.ts`, `use-blocks.ts`, `use-agent-chat.ts`, `use-revisions.ts`, `use-qa-wizard.ts`, `use-test-tool.ts` | Bound to old backend | Not ported |
| `lib/json-render-registry.ts`, `output-to-spec.ts`, `tool-naming.ts` | Tied to old app model | Not ported |
| Local state store | No Zustand | New Zustand stores (`flow`, `selection`, `ui`, `blocks`, `auth`) |

### Layer C — New build (no source to port)

- Rsbuild + Rspack config with Module Federation (host + 1+ remote packages in the monorepo for testing).
- Strict `shared` config: `react`, `react-dom`, `zustand`, `@tanstack/react-query`, `react-router-dom`, and our service-registry package — all `singleton: true` with pinned `requiredVersion`.
- Tauri 2 shell: `src-tauri/` scaffold, IPC sidecar to local edge agent.
- Platform capability module ([UI.md:42-52](UI.md#L42-L52)).
- Service registry over React Context (~50 LOC), published as a shared MF package.
- Block loader: manifest fetch, signature verification hook, MF dynamic import, iframe sandbox path for untrusted.
- React Flow canvas + node palette + property panel frame.
- Connect clients generated from `/packages/spi/*.proto`.
- `nats.ws` connection manager + subject-scoped subscribe hooks.
- OIDC flow (Zitadel).

---

## Milestones

**Milestone 0 is the gate.** Do not start Milestone 1 until M0 passes all exit checks.

| # | Milestone | Exit criteria |
|---|---|---|
| **0** | **MF proof-of-concept** | In a **separate minimal repo or `/experiments/mf-poc/`**: (a) Rsbuild host + Rsbuild remote built as independent packages; (b) remote exposes one React component that uses React Context from host; (c) host's Zustand store is read and written by remote; (d) host's TanStack Query cache is shared — remote triggers a query, host re-renders; (e) only one React instance in the combined runtime (verified via `React.version` identity check); (f) strict `singleton` enforcement — build fails if remote bumps React; (g) HMR works in dev for both host and remote concurrently; (h) production build verified with `bundle-stats` showing no duplicate shared deps; (i) React 19 specifically (not 18). **If any of (a)–(i) fails, migration is blocked pending resolution.** |
| 1 | New Studio app scaffold | `apps/studio/` builds with Rsbuild for `tauri` and `web` envs; empty shell renders; CI green; MF config copied from M0 POC |
| 2 | Design system port | Shadcn primitives, Tailwind v4, theme toggle, layout shell (sidebar + topbar + allotment) working on new scaffold |
| 3 | First real in-tree remote | A real Studio block (e.g. a trivial "Hello Node" contribution) built as a separate MF remote in the monorepo; loaded by Studio at runtime; consumes host service registry, Zustand, Query; no singleton warnings |
| 4 | Auth + transport | Zitadel OIDC login; `@sys/agent-client` calling real Control Plane with token; `nats.ws` subscription receiving a heartbeat |
| 5 | Flow canvas skeleton | React Flow canvas, node palette from registry, property panel frame — no real nodes yet |
| 6 | First block end-to-end | One in-tree UI-only block contributes a node type, property panel (`@rjsf/core` or chosen lib), renders in canvas |
| 7 | Tauri shell wired | Desktop build launches, IPC to local edge agent, file dialog works, auto-update block configured |
| 8 | Cutover | Old bizzy frontend archived; Studio is the only UI in docs and CI |

M0 → M1 → M3 are the risky path. M2, M4–M8 are mostly mechanical once the foundation holds.

---

## Risks & mitigations

- **MF + React 19 + Rspack maturity.** M0 is specifically designed to smoke this out before any investment. If it fails, fallbacks in order of preference: (a) drop to React 18 for Studio; (b) move to Webpack 5 MF (config largely portable); (c) reconsider the block model — trusted in-tree only, untrusted via iframe exclusively.
- **Singleton drift over time.** A future block author bumps React and the host silently accepts a duplicate. **Mitigation:** CI check that fails on any MF shared-dep mismatch, plus an `block lint` command that inspects remote `package.json` vs host-pinned versions.
- **Context loss with React 19 concurrent features.** `use()` hook, actions, and transitions have surfaced MF edge cases upstream. Track and pin workarounds.
- **Tailwind v4 + Rsbuild integration.** Verify in M0 — remote must render host-themed, not ship its own Tailwind runtime.
- **`@json-render` vs `@rjsf/core` divergence.** Picking wrong costs a rewrite of every block panel. Decide before M6.
- **Suspicious bizzy pins** (`typescript ~6.0.2`, `lucide-react ^1.8.0`). Don't copy blindly; match `/clients/ts` TypeScript 5.x.
- **Temptation to port `lib/api.ts`.** It's tied to the Go backend. Reject; use `@sys/agent-client`.

---

## Open questions

1. **Forms library.** [UI.md:66](UI.md#L66) specifies `@rjsf/core`, bizzy uses `@json-render/*`. Pick before M6.
2. **Router.** Keep `react-router-dom` (existing in bizzy) or TanStack Router for better loaders? Whichever we pick becomes an MF singleton.
3. **Icons.** Lucide is used; design doesn't pin. Confirm and pin.
4. **Storybook / component workshop from day one**, or later?
5. **RPC alongside REST.** Do we need `@connectrpc/connect-web` in v1, or does `@sys/agent-client` cover it? If both, does the RPC client live at `/clients/ts-rpc` or as a subpath export of the existing client?
6. **Service registry as its own package.** To be an MF shared singleton it probably needs to be a published package (e.g. `@sys/studio-registry`), not just a file in `apps/studio`. Decide in M0.

---

## Out of scope for this doc

- Per-block porting plans (none of bizzy's domain blocks apply).
- Backend / Control Plane work.
- Installer, signing, notarization, auto-update infrastructure.
- Other-language client packages (`/clients/rust|go|python` are future work per [clients/README.md](../../clients/README.md)).
