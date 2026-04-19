# UI & Extension System

## How it fits together

The Studio is a shell. It ships with built-in node types, but the interesting capability comes from **extensions** — third-party bundles that contribute nodes, property panels, dashboards, or whole new views. One contract, loaded at runtime, same shape for first-party and third-party.

An extension is a single unit with up to three parts: a **UI bundle** (federated React module), an **engine node** (Rust, statically linked or Wasm), and optionally an **extension process** (separate binary speaking gRPC).

## Build targets for the Studio

One React codebase, multiple shells. Same source, different build config.

| Target | Shell | Command | Artifact | Distribution |
|---|---|---|---|---|
| Windows desktop | Tauri 2 | `tauri build --target x86_64-pc-windows-msvc` | `.msi` + `.exe` installer | MSI, winget, direct download, auto-update |
| macOS desktop (Intel) | Tauri 2 | `tauri build --target x86_64-apple-darwin` | `.dmg` + `.app` | Notarized DMG, Homebrew cask, auto-update |
| macOS desktop (Apple Silicon) | Tauri 2 | `tauri build --target aarch64-apple-darwin` | `.dmg` + `.app` | Universal binary or separate, auto-update |
| Linux desktop | Tauri 2 | `tauri build --target x86_64-unknown-linux-gnu` | `.AppImage`, `.deb`, `.rpm` | AppImage, Flatpak, apt/rpm repos |
| Browser (hosted) | Rsbuild web target | `rsbuild build --env web` | Static SPA in `/dist` | Served by Control Plane or CDN |
| Browser (embedded in edge) | Rsbuild web target | Same as above | Static SPA | Served by edge agent for local admin UI |
| iOS (future) | Tauri 2 mobile | `tauri ios build` | `.ipa` | TestFlight, App Store |
| Android (future) | Tauri 2 mobile | `tauri android build` | `.apk` / `.aab` | Play Store, direct APK |

## How the same code targets all of them

| Concern | Desktop (Tauri) | Browser | Mobile (Tauri) |
|---|---|---|---|
| Rendering | System WebView — WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux | Browser engine | WKWebView (iOS), system WebView (Android) |
| File system | Tauri FS APIs via `@tauri-apps/api/fs` | Virtual FS abstraction, no direct access | Tauri FS APIs (scoped) |
| Native dialogs | Tauri `dialog` plugin | HTML `<input type="file">` fallback | Tauri `dialog` plugin |
| Auto-update | Tauri updater plugin | N/A — always fresh | Store-managed |
| Deep links | Tauri `deep-link` plugin | `postMessage` + URL routing | Tauri `deep-link` plugin |
| System tray | Tauri `tray` plugin | — | — |
| Local agent discovery | Unix socket / named pipe via Tauri sidecar | WebSocket to `localhost` if edge agent exposes it | Network only |
| Push notifications | Tauri `notification` plugin | Web Notifications API | Native push |
| Offline | Full, local DB, local engine | Service worker cache, read-only when offline | Full, local DB subset |

## Feature detection in code

Instead of forking the codebase per platform, one runtime module resolves capabilities:

```ts
// /packages/studio/src/platform.ts
export const platform = {
  isTauri: '__TAURI_INTERNALS__' in window,
  isBrowser: !('__TAURI_INTERNALS__' in window),
  hasFileSystem: isTauri || 'showDirectoryPicker' in window,
  hasNativeNotifications: isTauri || 'Notification' in window,
  canRunLocalAgent: isTauri,
  canEmbedWebview: isTauri,
};
```

Components use `platform.hasFileSystem` to decide behavior. No build-time conditionals for most things — one bundle, runtime branching. Build-time conditionals reserved for heavy imports (Tauri plugins, native-only libs) gated by `@tauri-apps/api` dynamic imports.

## Build stack

| Layer | Tech | Role |
|---|---|---|
| Shell (desktop/mobile) | Tauri 2 | Native window, system APIs, auto-update, IPC to Rust sidecar |
| Shell (browser) | None — served as static SPA | No native wrapper |
| Bundler | Rsbuild + Rspack | Dev server, production builds, per-target config |
| Module Federation | Rspack's native MF support | Runtime plugin loading, same mechanism on every target |
| UI kit | Shadcn + Tailwind | Components and styling |
| Canvas | React Flow | Flow editor |
| Forms | `@rjsf/core` driven by JSON Schema, with support for multi-variant settings (see [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) — Modbus serial vs TCP style) | Extension property panels |
| State — local | Zustand | UI state |
| State — server | TanStack Query | API caching |
| API transport | `@connectrpc/connect-web` | gRPC-Web to Control Plane |
| Live transport | `nats.ws` | NATS over WebSocket |
| Auth | `oidc-client-ts` + PKCE | Zitadel integration |
| Plugin host | Module Federation + custom registry | Extension loading |
| Service registry | ~50 LOC over React Context | DI without InversifyJS |

## Rsbuild target configuration

Single `rsbuild.config.ts` with per-target overrides:

```ts
// conceptually
export default defineConfig({
  source: { entry: { index: './src/index.tsx' } },
  environments: {
    tauri: {
      output: { target: 'web', distPath: { root: 'dist-tauri' } },
      tools: { rspack: { /* Tauri-specific */ } },
    },
    web: {
      output: { target: 'web', distPath: { root: 'dist-web' } },
      tools: { rspack: { /* no Tauri plugins */ } },
    },
  },
});
```

`tauri build` invokes `rsbuild build --env tauri`. Hosting the browser app runs `rsbuild build --env web`. Same source, different bundles.

## The extension contract

| Concept | What it is | Where it lives |
|---|---|---|
| `manifest.json` | Extension metadata: id, version, targets, permissions, contributions | Root of extension folder |
| `node.schema.json` | JSON Schema for each node type this extension contributes | Referenced from manifest |
| `extension.proto` | gRPC contract (if the extension has a separate process) | `/packages/spi/` |
| UI bundle | Federated React module exposing panels, custom nodes, views | `/ui/` subdirectory |
| Engine node | Rust crate implementing runtime behavior | `/engine/` subdirectory |
| Extension process | Optional separate binary (Rust, Go, Python — anything with gRPC) | `/process/` subdirectory |
| Host functions | `get_input`, `set_output`, `log`, `call_extension`, `kv_get`, `kv_set` | Provided by engine to Wasm nodes |
| Capabilities | Declared permissions (network, filesystem, KV, extension calls) | `manifest.json` permissions block |

## Extension flavors

| Flavor | UI | Runtime | Process | When to use |
|---|---|---|---|---|
| UI-only | ✓ | — | — | Custom dashboards, property panels, theme packs |
| Wasm node | ✓ | Wasm via Wasmtime | — | Sandboxed compute — math, parsing, transforms. Language-agnostic |
| Native Rust node | ✓ | Statically linked | — | Performance-critical, trusted, first-party |
| Protocol / integration | ✓ | Rust adapter node | ✓ separate binary | BACnet, Modbus, Salesforce, Slack, ML inference |
| Logic script | — | QuickJS inside Function node | — | User-written JS inside a flow (not a distributed extension) |

## What an extension can contribute to the UI

| Contribution | Declared in manifest | Rendered where |
|---|---|---|
| Node types | `contributions.nodes[]` | Flow canvas palette |
| Property panel | `contributions.panels[]` with `target: "node-config"` | Right panel when a node is selected |
| Custom view | `contributions.views[]` with `location: "sidebar" \| "main"` | Left sidebar or main area route |
| Dashboard widget | `contributions.widgets[]` | Dashboard pages users compose |
| Command / action | `contributions.commands[]` | Command palette, menus, keybindings |
| Settings page | `contributions.settings[]` | Settings area, scoped to extension |
| Theme | `contributions.themes[]` | Global theme picker |

## How extensions behave per target

| Capability | Desktop (Tauri) | Browser | Mobile (Tauri) |
|---|---|---|---|
| UI bundle loading | ✓ | ✓ | ✓ |
| Wasm node execution | ✓ (Wasmtime on edge, browser engine if running Studio-local) | ✓ (browser Wasm engine) | ✓ |
| Native engine node | ✓ (runs on edge, not in UI) | — (remote only, via gRPC-Web to Control Plane) | — (remote only) |
| Extension process | ✓ (runs on edge) | — (remote only) | — (remote only) |
| Protocol extensions (BACnet, Modbus) | ✓ | — (can configure/monitor remotely via Control Plane) | — (same) |
| File-based extensions | ✓ | — (virtual FS only) | Scoped FS |

Extensions with native/process parts only *run* on deployments where the engine is local (desktop connected to local edge agent, or edge agent standalone). From a browser, you can still author and monitor those flows — the UI bundle loads fine, but execution happens wherever the engine is deployed.

## Extension lifecycle

| Stage | What happens |
|---|---|
| Author develops | Local dev server, hot reload, extension loaded via local manifest |
| Publish | `yourapp ext publish` — signs, bundles, uploads UI + engine + process to Control Plane registry |
| Install | Admin enables extension for an org; Control Plane resolves version, distributes assets |
| Edge fetch | Edge agent pulls engine binary + extension process on next sync; UI bundle cached by Studio |
| Studio load | On flow open, Studio fetches required UI bundles via Module Federation, verifies signatures |
| Runtime | Flow executes — engine node calls into Wasm or extension process; UI panels render live state |
| Update | New version published; rollout policy decides canary vs fleet-wide; old version retained for rollback |
| Uninstall | Flow validation blocks removal if nodes still in use; forced removal disables those nodes cleanly |

## Security and trust boundaries

**Module Federation is a delivery mechanism, not a sandbox.** Federated modules share a JS realm with the host; they can see `window`, React internals, Zustand stores, and any token in memory. Signature verification proves provenance, not isolation. The UI isolation model reflects this:

| Trust tier | Loading mode | When |
|---|---|---|
| First-party, in-tree | Direct MF load into host realm | Built by us, shipped in-repo |
| Signed + vetted third-party | Direct MF load into host realm | Extensions audited and approved for the registry |
| Untrusted third-party | **iframe** with `postMessage` bridge (or Web Worker for headless compute) | Anything installed outside the vetted registry; default for user-installed third-party |

Property-panel forms for untrusted extensions are schema-driven (`@rjsf/core` over `node.schema.json`) so the common case needs no custom React from untrusted sources at all.

| Concern | Mechanism |
|---|---|
| Capability-scoped permissions | Manifest declares what it needs; user approves on install |
| Wasm sandbox | Wasmtime fuel metering, memory caps, host-function allowlist |
| Extension process isolation | Separate binary, cgroup memory limit, restart-on-crash |
| UI isolation (trusted) | MF shared-deps pinning; extension bundled with host realm — trust model assumes signing + review |
| UI isolation (untrusted) | iframe / Web Worker with postMessage channel; no shared globals |
| Signature verification | All published extensions signed; edge and Studio verify before loading |
| RBAC | Extension actions flow through the same JWT + role check as built-in actions |
| Audit | Every install, config change, and call logged to the audit stream |

## Cross-MF gotcha: React Context and the service registry

Our service registry is a `Map<string, Service>` exposed via React Context ([OVERVIEW.md service wiring](../../README.md)). React Context across Module Federation boundaries is a known footgun: if the host and a federated module each have their own copy of React, they have separate Context namespaces and the registry won't be visible.

Mitigation: React is a **required singleton shared module** in the MF config (host version wins, no duplicate trees). Validated in Stage 3 with a real federated module from a separate build, not a co-located one.

## Repo layout for an extension

```
/extensions/bacnet/
  manifest.json
  README.md
  /ui/
    src/index.tsx          # federated module exports
    rsbuild.config.ts
    node.schema.json       # property forms for each node type
  /engine/
    Cargo.toml             # Rust crate, built into edge-agent
    src/lib.rs             # maps crossflow messages ↔ gRPC calls
  /process/
    Cargo.toml             # separate binary
    src/main.rs            # BACnet stack, gRPC server
    extension.proto        # symlinked from /packages/spi/
```

Same shape for every extension. A theme pack has only `/ui/`. A Wasm compute node has `/ui/` + a `/wasm/` directory. A protocol integration has all three.

## One-line summary

**One React codebase, built with Rsbuild into Tauri shells for Windows/macOS/Linux (and iOS/Android later) or a static SPA for browsers — same Module Federation plugin system and same extension contract across every target, with runtime feature detection branching where platform capabilities genuinely differ.**