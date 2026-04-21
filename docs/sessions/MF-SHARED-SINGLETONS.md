# Module Federation shared singletons — unresolved

Status: **broken in dev**. Studio + block MF bundles cannot agree on shared
modules (`react-router-dom`, `@listo/ui-core`, `@listo/block-ui-sdk`).
Runtime fails with `factory is undefined` on load.

## Summary

Listo ships one MF host (Studio) and N MF remotes (one per block). Both
need to share at least React, react-router, zustand, react-query, and
the Listo packages that hold singleton state (most importantly
`@listo/ui-core`, which owns the module-level `agentPromise`). Without
sharing, every block ships its own copy of ui-core → its own
`AgentClient` instance → its own baseUrl → every REST call from a block
panel hangs on the wrong port.

**The current state does not work.** Browser console shows:

```
undefined factory webpack/sharing/consume/default/react-router-dom/react-router-dom
Error: RuntimeError: factory is undefined (webpack/sharing/consume/default/react-router-dom/react-router-dom)
    at __webpack_require__
    at ../ui-core/dist/lib/fleet/ScopeContext.js
    at ../ui-core/dist/lib/fleet/index.js
    at ../ui-core/dist/index.js
    at ./src/providers/index.tsx   ← Studio itself
```

It breaks inside Studio's *own* bootstrap, not just the block.

## Where the shared config lives

- Authoritative list: [ui-core/src/mf.ts](../../../ui-core/src/mf.ts) →
  `export const MF_SHARED_SINGLETONS`.
- Studio's [rsbuild.config.ts](../../../studio/rsbuild.config.ts) imports
  it and passes to `ModuleFederationPlugin({ shared: MF_SHARED_SINGLETONS })`.
- Each block's rsbuild.config.ts imports the same list via
  `@listo/block-ui-sdk/mf` (a re-export) and uses it identically.
- block-ui-sdk's [src/mf.ts](../../../block-ui-sdk/src/mf.ts) is a
  one-line re-export from ui-core: `export { MF_SHARED_SINGLETONS } from "@listo/ui-core/mf";`

## The original symptom (which is what we were trying to fix)

Block panel writes time out after 10 s with `AbortSignal.timeout`:

```
Error: signal timed out
{"path":"/iot/broker","slot":"settings","value":{...}}
```

Root cause: block bundles its own `@listo/ui-core`; ui-core's
`agentPromise` reads `PUBLIC_AGENT_URL` at module load; the block was
built without that env var, so its `AgentClient` defaults to
`http://localhost:8080`. Edge agent is on :8082 — nothing on :8080 —
fetch hangs → timeout.

Two fixes, either one works on its own:

1. **Share `@listo/ui-core` as an MF singleton.** The block defers to
   Studio's instance, which was built with `PUBLIC_AGENT_URL=:8082`.
2. **Bake `PUBLIC_AGENT_URL` into the block at build time.** The
   block's own AgentClient now points at the right URL. No sharing
   needed.

We attempted (1) first, which broke things; then (2) which works for
the block's Panel **but Studio itself still crashes** on the new
shared config because HMR state doesn't cleanly re-initialise the
share scope. Restarting the dev server SHOULD recover, but in our
sessions it didn't reliably — the browser cached bundles reference
the old share scope and the new ones reference the modified one.

## Chronology of attempts (all shipped in this branch — revert if needed)

1. Added `@listo/agent-client`, `@listo/ui-core`, `@listo/ui-kit`,
   `@listo/block-ui-sdk` to `MF_SHARED_SINGLETONS` with `singleton: true`
   only.
   - **Failure mode:** the block's MF manifest ends up with
     `requiredVersion: "^undefined"` because rspack's plugin infers
     `requiredVersion` from the *consuming* package's `package.json`
     dependencies — and the block's package.json doesn't list these
     packages directly (only `@listo/block-ui-sdk`).
   - Verification command:
     ```
     curl -s http://localhost:8082/blocks/com.listo.mqtt-client/mf-manifest.json \
       | python3 -c "import sys,json;[print(s['name'],s['requiredVersion']) for s in json.load(sys.stdin)['shared']]"
     ```

2. Added the transitive listo packages to the block's `devDependencies`
   and `dependencies` (with `workspace:^0.1.0` specifier).
   - **Failure mode:** still emitted `^undefined`. rspack likely can't
     parse the `workspace:*` protocol, or it reads `package.json`
     subpath access (`@listo/ui-core/package.json`) which is blocked
     by ESM `exports` on newer packages.

3. Added `"./package.json": "./package.json"` to the `exports` field
   of all four listo packages so rspack can resolve the version via
   `require.resolve('@listo/ui-core/package.json')`.
   - **Failure mode:** no change — still `^undefined`. The MF plugin
     probably isn't using `require.resolve` for this.

4. Tried `requiredVersion: false` + `strictVersion: false` on each
   shared entry.
   - **Failure mode:** rspack **overrides** these in the emitted
     manifest. For `react` (which IS in package.json), the explicit
     `^19.0.0` we supplied was replaced with `^19.2.5` (installed
     version). For listo packages not in package.json, still
     `^undefined`.

5. Tried `eager: true` + explicit `version: "0.1.0"`.
   - **Failure mode:** bundle size doubled (3 MB → 5.4 MB) because
     eager means "inline all deps"; `requiredVersion` still
     `^undefined`.

6. **Switched strategy — build-time injection of `PUBLIC_AGENT_URL`.**
   Removed the listo packages from shared entirely. Updated
   [ui-core/src/lib/agent/index.ts](../../../ui-core/src/lib/agent/index.ts)
   to **throw** on module load if the URL wasn't baked in.
   [blocks/com.listo.mqtt-client/ui-src/rsbuild.config.ts](../../../blocks/com.listo.mqtt-client/ui-src/rsbuild.config.ts)
   requires the env var and fails the build without it.
   - **Partial win:** the block's panel now builds with the right URL.
     But Studio also needs `PUBLIC_AGENT_URL` at dev-server start (fine
     — mani's `dev-edge` already sets it).
   - **New failure:** restarting rsbuild during an HMR cycle leaves the
     browser holding a mix of old and new chunks → `react-router-dom`
     factory-undefined panic in Studio's own bootstrap.

## Where the code currently is

- `MF_SHARED_SINGLETONS` contains: `react`, `react-dom`, `react-router-dom`,
  `zustand`, `@tanstack/react-query`. NO listo packages.
- [ui-core/src/lib/agent/index.ts](../../../ui-core/src/lib/agent/index.ts)
  throws if `PUBLIC_AGENT_URL` isn't baked in. This is a **no-fallback**
  rule per user preference — no silent `?? "http://localhost:8080"`.
- [blocks/com.listo.mqtt-client/ui-src/rsbuild.config.ts](../../../blocks/com.listo.mqtt-client/ui-src/rsbuild.config.ts)
  `require`'s `PUBLIC_AGENT_URL` via `requireEnv()`. Throws at build if
  missing.
- [blocks/com.listo.mqtt-client/Makefile](../../../blocks/com.listo.mqtt-client/Makefile)
  targets `edge` / `cloud` pass `PUBLIC_AGENT_URL` explicitly. `make ui`
  on its own fails by design.
- `./package.json` was added to the `exports` field of `ui-core`,
  `ui-kit`, `block-ui-sdk`, `agent-client-ts` — harmless but didn't
  help what it was supposed to.
- The block's `ui-src/package.json` still carries the listo packages
  as explicit deps with `workspace:^0.1.0`. Revert if we abandon option
  (1) entirely; keep if we later revisit shared-singleton sharing with
  the version resolution fixed.

## What the next session needs to decide

### Option A — abandon MF singleton sharing of listo packages entirely

**Accept:** every block ships its own `@listo/ui-core` bundle (~3 MB).
**Rely on:** `PUBLIC_AGENT_URL` baked at build time.

Tasks:
1. Revert the per-block `@listo/*` deps in `ui-src/package.json`
   (we added them while debugging the shared route).
2. Remove the MF shared entries that were experimental (done for
   listo packages, but double-check).
3. Debug the `react-router-dom` factory-undefined crash. This is
   either HMR cache stale, or rsbuild dev-server state pollution.
   Reproduce from a clean tree:
   ```
   pkill -f rsbuild
   rm -rf studio/dist studio/.rsbuild blocks/*/ui studio/node_modules/.federation
   cd studio && PUBLIC_AGENT_URL=http://localhost:8082 pnpm dev --port 3010
   # then in another terminal:
   cd blocks/com.listo.mqtt-client && make edge
   ```
   Open **private browsing** window at http://localhost:3010 to
   bypass HMR caching.
4. If the crash persists, suspect the `ui-core/dist/lib/fleet/ScopeContext.js`
   chain specifically — pin down whether it's a Studio-local bug or
   an MF runtime ordering issue.

### Option B — fix MF shared singletons properly

**Accept:** learn rspack's `@module-federation/enhanced` plugin internals.
**Benefit:** one copy of ui-core, shared agent client, proper MF
model matching the original design intent.

The question is: **why does rspack's shared plugin refuse to honour
our explicit `requiredVersion`?** Answers to find:
1. Check `@module-federation/enhanced` source for where it computes
   `requiredVersion`. File: `node_modules/.pnpm/@module-federation+enhanced@0.6.16/.../dist/...`
2. Look for an option like `automaticRequiredVersion: false` or similar.
3. Try `import: false` on the shared entry — this tells MF "this
   is a consume-only share" and may disable some heuristics.
4. Try switching to a newer `@module-federation/enhanced` (0.6.16 is
   old; 0.9+ may have fixed this).
5. If all else fails, post-process the mf-manifest.json in the block's
   Makefile to rewrite `^undefined` → `*`.

### Option C — neither. Use runtime sharing via window globals.

Studio sets `window.__LISTO_AGENT_CLIENT__ = agentClient` before loading
any block. Blocks read from there. Ugly but bypasses MF entirely for
the singleton concern. Keep MF only for React/router (which ARE sharing
correctly today — the failure is only for the listo packages).

## Tools / commands the next session should know

### Inspect an MF remote's shared manifest

```bash
curl -s http://localhost:8082/blocks/com.listo.mqtt-client/mf-manifest.json \
  | python3 -c "import sys,json
d=json.load(sys.stdin)
for s in d['shared']:
    print(f\"{s['name']:40s} v={s.get('version','?'):10s} req={s.get('requiredVersion','?'):15s} singleton={s.get('singleton')}\")
"
```

If any line shows `req=^undefined`, rspack couldn't infer the version →
shared runtime will fail.

### Inspect Studio's providing manifest

```bash
curl -s http://localhost:3010/mf-manifest.json | python3 -m json.tool | less
```

The `shared[]` list is what Studio PROVIDES. Missing entries here mean
Studio won't satisfy a block's consume side.

### Which URL is baked into a bundle

```bash
grep -rEho '"http://localhost:808[0-9]"' blocks/com.listo.mqtt-client/ui/static/js/ | sort -u
```

Should show exactly one URL — the one matching the `PUBLIC_AGENT_URL`
you built with.

### Clean restart (nuke all dev-server caches)

```bash
pkill -f rsbuild
rm -rf \
  studio/dist studio/.rsbuild \
  studio/node_modules/.federation \
  studio/node_modules/.cache \
  blocks/*/ui \
  blocks/*/ui-src/.rsbuild
```

### Browser-side

- Always **hard refresh** (Cmd/Ctrl+Shift+R) — regular refresh keeps
  MF chunks cached.
- Better: open the Network tab with "Disable cache" checked, or use a
  private window.
- The MF runtime registers shared modules in `window.__FEDERATION__`.
  Inspect at runtime:
  ```js
  window.__FEDERATION__.__INSTANCES__[0].shareScopeMap
  ```

## Files changed in this branch (for revert reference)

```
ui-core/src/mf.ts                            — singleton list
ui-core/src/lib/agent/index.ts               — throws if PUBLIC_AGENT_URL unset
ui-core/package.json                         — added ./package.json export
ui-kit/package.json                          — added ./package.json export
block-ui-sdk/package.json                    — added ./package.json export
block-ui-sdk/src/index.ts                    — re-exported useKinds + useNodeSettings
block-ui-sdk/src/mf.ts                       — re-export of ui-core's mf
agent-client-ts/package.json                 — added ./package.json export
blocks/com.listo.mqtt-client/ui-src/package.json — added @listo/* deps
blocks/com.listo.mqtt-client/ui-src/rsbuild.config.ts — requireEnv(PUBLIC_AGENT_URL)
blocks/com.listo.mqtt-client/Makefile         — edge/cloud pass env
```

## References

- Module Federation docs: https://module-federation.io/configure/shared.html
- The rspack plugin source: `node_modules/.pnpm/@module-federation+enhanced@0.6.16/node_modules/@module-federation/enhanced/`
- Relevant upstream issue (search): "rspack shared requiredVersion undefined workspace"

## One-line summary

**MF shared-singleton sharing of workspace packages is broken in
`@module-federation/enhanced@0.6.16` because the plugin can't resolve
versions from `workspace:*` specs; until that's fixed, use build-time
`PUBLIC_AGENT_URL` injection as the source of truth for each bundle's
agent URL, and don't try to share `@listo/*` via MF.**
