# Module Federation shared singletons — work in progress, NOT FINISHED

> **READ FIRST, NEW SESSION:** the previous session declared this
> "done" twice without testing end-to-end in a real browser. Both
> times it was still broken. **DO NOT repeat that mistake.** The only
> acceptable definition of done is:
>
> 1. Studio loads in a real browser at http://localhost:3010
> 2. React actually mounts (`<div id="root">` is NOT empty)
> 3. Navigating to a block panel (mqtt-client) renders its UI
> 4. A block panel write (e.g. settings slot) hits agent :8082 and
>    returns success within 10 s
> 5. Console has ZERO errors (no `factory is undefined`, no
>    unhandled rejections)
> 6. `window.__FEDERATION__.__INSTANCES__[0].shareScopeMap.default`
>    shows ALL `@listo/*` entries with `loaded: true` AND `hasLib: true`
>
> Playwright is installed and a smoke script lives at
> [/home/user/code/workspace/studio-smoke.mjs](../../../studio-smoke.mjs).
> Use it. Static manifest inspection is NOT sufficient — it missed a
> factory-evaluation deadlock that only surfaces in-browser.

## Current status at handoff

**Partial working state.** The chain Studio → agent now works end-to-end
for Studio's own UI: React mounts, API calls hit :8082. **Block panel
loading has not been verified yet.**

**What I observed in the last Playwright run (before handoff interrupt):**

- Studio root mounts — React alive, making real HTTP requests to the
  agent
- Shared singletons show `loaded: true` for react, react-dom,
  react-router-dom, zustand, @tanstack/react-query, @listo/agent-client
- Agent was killed mid-session; last log showed `ERR_CONNECTION_REFUSED`
  for `/api/v1/events` — expected, just means restart the agent
- **NOT yet verified:** `@listo/ui-core` and `@listo/ui-kit` status
  after the fix. The earlier problem was they sat `loaded: false`
  forever. Need to re-run smoke with the agent alive to confirm.
- **NOT yet verified:** block panel mounts + writes succeed.

## The full story — what was wrong, in order of discovery

### Layer 1 — `requiredVersion: "^undefined"` in manifest

`@module-federation/enhanced@0.6.16` can't parse `workspace:*` pnpm
specifiers. Poisoned share-scope entries leave neighboring factories
uninitialised; surfaces as `factory is undefined` for unrelated
packages (react-router-dom was the usual suspect).

**Fixed by:**

1. Bumped `@module-federation/enhanced` `^0.6.0` → `^2.3.3` in:
   - `studio/package.json`
   - `ui-core/package.json`
   - `blocks/com.acme.hello/ui-src/package.json`
   - `blocks/com.listo.bacnet/ui-src/package.json`
   - `blocks/com.listo.mqtt-client/ui-src/package.json`

   **Gotcha:** forgot the first two blocks the first time. Old
   version's DTS worker spawned via `npx tsc` kept running as a
   zombie process (pid 2089197) spamming `dts-plugin@0.6.16` errors
   across dev sessions. If you see `0.6.16` anywhere in a log after
   bumping, `pgrep -af dts-plugin` and kill the zombie first.

2. Replaced the static `MF_SHARED_SINGLETONS` object with a factory
   `createSharedSingletons()` in [ui-core/src/mf.ts](../../../ui-core/src/mf.ts).
   The factory reads each workspace package's real semver from its own
   `package.json` via `createRequire(import.meta.url)` and hands MF a
   plain semver string — MF never sees a `workspace:*` specifier.
   Each `@listo/*` entry pins BOTH `version` AND `requiredVersion`.

3. `block-ui-sdk/src/mf.ts` re-exports `createSharedSingletons`
   (upstream API change).

### Layer 2 — DTS plugin noise + crash

MF DTS plugin spawns tsc via `npx` to generate federated type zips.
`npx` isn't on PATH in this environment (`which npx` → not found).
Spams `#TYPE-001` on every HMR rebuild; eventually the IPC channel
crashes with `ERR_IPC_CHANNEL_CLOSED` → `ELIFECYCLE 137` → dev server
dies silently.

**Fixed by:** `dts: false` in Studio's and the mqtt-client block's
`rsbuild.config.ts`. Monorepo = types flow through workspace symlinks,
not through MF type zips. MF DTS is pointless here.

**TODO (new session):** apply `dts: false` to the other two blocks
too. They still have the old `dts: { generateTypes: ... }` config:

- `blocks/com.acme.hello/ui-src/rsbuild.config.ts`
- `blocks/com.listo.bacnet/ui-src/rsbuild.config.ts`

I didn't touch these because they weren't part of the active test
loop, but they will break the same way the moment you `pnpm dev`
against them.

### Layer 3 — `dev.lazyCompilation` default is MF-incompatible

Rsbuild 1.7 defaults to `dev: { lazyCompilation: { imports: true,
entries: false } }`. This wraps every dynamic import in a
compile-on-demand proxy. The proxy uses an XHR callback to trigger
actual module evaluation.

**The proxy is incompatible with MF shared consume.** When a chunk is
fetched for a shared factory, the proxy fires the XHR but the MF
runtime doesn't wait for it. Chunks return 200, factories NEVER run.
`@listo/ui-core` and `@listo/ui-kit` show `loaded: false` forever, React
never mounts, and there is ZERO console output to indicate the
problem. Silent hang.

**Fixed by:** `dev: { lazyCompilation: false }` in
[studio/rsbuild.config.ts](../../../studio/rsbuild.config.ts).
Block configs use `rsbuild build` not `rsbuild dev`, so they don't
need this.

### Layer 4 — ui-core and ui-kit self-import by package name

The hidden killer. Files inside `ui-core/src/` import from
`"@listo/ui-core"` (barrel path), not relative paths. 29 files in
ui-core, 2 in ui-kit.

When ui-core is compiled to `ui-core/dist/*.js`, those imports stay as
literal `from "@listo/ui-core"` strings. When Studio then bundles
ui-core's dist AND registers ui-core as an MF shared singleton, rspack's
ConsumeSharedPlugin rewrites every `"@listo/ui-core"` import inside
the shared module **into a consume of itself** — share-scope asks for
ui-core, which needs the factory, which needs the consumer. Deadlock.

This is a **documented MF footgun:** a shared module MUST NOT import
itself by package name. All internal imports must be relative.

**Fix (applied, needs verification):** use `tsc-alias` to rewrite
self-imports to relative paths at build time, without touching 31
source files.

- `ui-core/tsconfig.json` `compilerOptions.paths` now includes:
  `"@listo/ui-core": ["src/index.ts"]`
- `ui-kit/tsconfig.json` `compilerOptions.paths` now includes:
  `"@listo/ui-kit": ["src/index.ts"]`

`tsc-alias` is already in the build pipeline (`tsc --project &&
tsc-alias -p tsconfig.json`), so it rewrites both `@/*` and
`@listo/<self>` imports in the emitted `dist/*.js` to relative paths.
Verified the output no longer contains self-imports:
`grep '@listo/ui-core' ui-core/dist/` only shows comments and the
intentional reference in `dist/mf.js`.

**Alternative considered, rejected:** refactoring the 31 source files
to use relative imports. More invasive for the same outcome; the path
mapping gets us there without touching product code.

### Layer 5 — agent module-load throw

Earlier session made `ui-core/src/lib/agent/index.ts` throw on module
load if `PUBLIC_AGENT_URL` wasn't baked. That turned out to be
dangerous because every MF remote *compiles in* a copy of ui-core
even though only the host's copy is evaluated. A module-load throw
inside a shared consumer leaves share-scope init in a weird state.

**Fixed by:** `agentPromise` is now constructed lazily via
`Promise.resolve().then(...)`. Module init always succeeds; rejection
surfaces only when a caller `await`s the promise and the URL is
missing.

## Current repo state (files touched this session)

```
studio/rsbuild.config.ts                              dts: false, lazyCompilation: false, createSharedSingletons()
studio/package.json                                   enhanced ^2.3.3
studio/src/components/layout/SiteHeader.tsx          breadcrumb <li> nesting fix (unrelated)
ui-core/tsconfig.json                                 path mapping @listo/ui-core → src/index.ts
ui-core/package.json                                  enhanced ^2.3.3, @types/node dev dep
ui-core/src/mf.ts                                     createSharedSingletons() factory
ui-core/src/lib/agent/index.ts                        lazy agentPromise
ui-kit/tsconfig.json                                  path mapping @listo/ui-kit → src/index.ts
block-ui-sdk/src/mf.ts                                re-export createSharedSingletons
blocks/com.acme.hello/ui-src/package.json             enhanced ^2.3.3
blocks/com.listo.bacnet/ui-src/package.json           enhanced ^2.3.3
blocks/com.listo.mqtt-client/ui-src/package.json      enhanced ^2.3.3, comment updated
blocks/com.listo.mqtt-client/ui-src/rsbuild.config.ts dts: false, createSharedSingletons(), no PUBLIC_AGENT_URL plumbing
blocks/com.listo.mqtt-client/Makefile                 no PUBLIC_AGENT_URL plumbing
<root>/package.json                                   playwright ^1.59.1 dev dep
<root>/studio-smoke.mjs                               Playwright smoke test (see below)
```

## End-to-end test recipe (DO THIS — do not skip)

### Step 0 — sanity

```bash
cd /home/user/code/workspace
pgrep -af "rsbuild\|dts-plugin\|start-broker"   # should be empty
```

If any zombies, `kill -9 <pid>` them. Zombies from the 0.6.16 era
keep spewing stale `factory is undefined` and `dts-plugin@0.6.16`
errors into logs and will make you think the fix didn't land.

### Step 1 — rebuild workspace packages

```bash
rm -rf ui-core/dist ui-kit/dist block-ui-sdk/dist agent-client-ts/dist
pnpm install
pnpm --filter @listo/agent-client --filter @listo/ui-kit \
     --filter @listo/ui-core --filter @listo/block-ui-sdk run build
```

Verify self-imports are gone from dist:

```bash
grep -rn '@listo/ui-core' ui-core/dist/ | grep -v sourceMap | grep -v '\.map' | grep -v '\.d\.ts' | grep -v 'dist/mf.js'
# should be EMPTY
```

### Step 2 — rebuild block UI

```bash
rm -rf blocks/com.listo.mqtt-client/ui
cd blocks/com.listo.mqtt-client && make edge
cd -
```

### Step 3 — launch dev-edge fresh

```bash
mani run kill-dev --projects agent
rm -rf studio/dist-web studio/node_modules/.federation studio/node_modules/.cache
rm -f /tmp/dev-edge.log
nohup mani run dev-edge --projects agent > /tmp/dev-edge.log 2>&1 &
disown
until ss -tln | grep -qE ':(3010|8082)'; do sleep 2; done
```

### Step 4 — STATIC manifest check (quick sanity, NOT sufficient)

```bash
curl -s http://localhost:3010/mf-manifest.json | python3 -c "
import sys, json
d = json.load(sys.stdin)
for s in d['shared']:
    v = s.get('version', '?'); r = s.get('requiredVersion', '?')
    print(f\"{s['name']:32s} v={v:12s} req={r:14s}\")"

curl -s http://localhost:8082/blocks/com.listo.mqtt-client/mf-manifest.json | python3 -c "
import sys, json
d = json.load(sys.stdin)
for s in d['shared']:
    v = s.get('version', '?'); r = s.get('requiredVersion', '?')
    print(f\"{s['name']:32s} v={v:12s} req={r:14s}\")"
```

Expected: every row shows real semver (no `^undefined`), all listo
packages at `0.1.0`.

### Step 5 — ACTUAL end-to-end test in a real browser

```bash
cd /home/user/code/workspace
node ./studio-smoke.mjs
```

Expected:
- `rootKids > 0`
- `scopeSummary` shows ALL `@listo/*` with `loaded: true, hasLib: true`
- `=== logs ===` has ZERO `[pageerror]` lines
- ZERO `factory is undefined` occurrences

If `@listo/ui-core` or `@listo/ui-kit` still shows `loaded: false`,
the self-import deadlock is back — check that `dist/` doesn't contain
`@listo/ui-core` / `@listo/ui-kit` string literals in its JS output.
`tsc-alias` may not have run; rebuild ui-core/ui-kit.

### Step 6 — navigate to a block panel

The smoke script only tests Studio's own root. Extend it (or manually
via Playwright) to:

1. `page.goto(STUDIO + "/blocks")` — block list renders
2. Click into mqtt-client — panel loads
3. Trigger a settings write — verify `POST /api/v1/nodes/.../settings`
   returns 200 within 10 s (the old bug timed out after 10 s)

This is the scenario that originally surfaced the entire saga. If
this doesn't work, this doc is still not done.

## Debug tooling

### window.__FEDERATION__ inspection

```js
const inst = window.__FEDERATION__.__INSTANCES__[0];
Object.fromEntries(
  Object.entries(inst.shareScopeMap.default).map(([name, versions]) => [
    name,
    Object.fromEntries(
      Object.entries(versions).map(([v, slot]) => [v, {
        loaded: !!slot.loaded, hasLib: !!slot.lib, from: slot.from
      }])
    )
  ])
)
```

`loaded: true` AND `hasLib: true` means the factory ran and a real
module object exists. `loaded: false` means the factory never
completed — that's the deadlock state.

### Zombie hunting

```bash
pgrep -af "dts-plugin\|rsbuild\|start-broker\|@module-federation"
```

The 0.6.16 DTS broker has specifically shown up as
`start-broker.js` from the old path. If you see it, kill it. If you
see any `0.6.16` path in a log, a zombie is still alive.

### Orphan pnpm folders

After bumping MF, pnpm may leave `.pnpm/@module-federation+*@0.6.16*`
directories. They're not linked from anywhere, but scripts and logs
sometimes still reference paths inside them (zombies, cached
`NODE_PATH`, etc). Nuke:

```bash
rm -rf node_modules/.pnpm/*0.6.16*
pnpm install
```

### Check baked PUBLIC_AGENT_URL

```bash
curl -s http://localhost:3010/static/js/async/ui-core_dist_index_js.js \
  | grep -oE '"http://localhost:80[0-9]+"' | sort -u
```

Should show only the URL you started dev-edge with (`http://localhost:8082`).

## What the next session MUST NOT do

- **Must not say "done" based on static manifest checks alone.** The
  manifest can be perfect while the app fails to mount (that is
  literally what happened twice this session).
- **Must not ignore "no console errors" as proof of success.** The
  lazyCompilation deadlock produced zero errors; it just silently
  hung. Visual confirmation of the mounted UI is mandatory.
- **Must not paper over DTS errors.** If `#TYPE-001` shows up, dev
  server is unstable. Disable DTS (`dts: false`) everywhere it's on.
- **Must not leave zombie processes.** Always grep for
  `dts-plugin@0.6.16` in a fresh log — its presence proves a stale
  process is feeding you old output.

## Remaining work

1. **Run the full end-to-end test recipe above.** If any step fails,
   stop and fix the root cause before claiming success.
2. **Apply `dts: false` to the two other blocks'
   `rsbuild.config.ts`** (com.acme.hello, com.listo.bacnet).
3. **Extend `studio-smoke.mjs` to exercise a block panel write** —
   Step 6 above. Block panel writes were the original bug; without
   verifying that path, the fix is speculative.
4. **Remove the `@types/node` devDep workaround** if a cleaner place
   exists — right now it's in `ui-core/package.json` because
   `src/mf.ts` uses `node:module`, which TS needs types for. Moving
   mf.ts into a separate build target (e.g. `ui-core/build-tools/`)
   would be cleaner.
5. **Consider moving `createSharedSingletons` out of the runtime
   package entirely.** It's only consumed by build config files; a
   separate `@listo/mf-config` workspace package would prevent
   accidental browser-side import.

## References

- Upstream fix for the version-inference bug:
  [module-federation/core#4614](https://github.com/module-federation/core/pull/4614)
- Related issues: [#3101](https://github.com/module-federation/core/issues/3101),
  [#1310](https://github.com/module-federation/core/issues/1310)
- MF shared docs: https://module-federation.io/configure/shared.html
- The self-import rule (well-known footgun, not well-documented):
  the shared-module bundle must resolve its internal imports without
  going through the consume plugin. The usual workaround is relative
  imports; our `tsc-alias` mapping is equivalent.
