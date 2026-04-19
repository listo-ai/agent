# Plugins

How plugins (a.k.a. extensions) are packaged, discovered, loaded, and served. Long-term vision here; see [STEPS.md](../sessions/STEPS.md) for the staged landing.

**One rule:** every later capability — signing, kind migrations, Wasm, process plugins, fleet push — only *adds fields* to `plugin.yaml` and *adds branches* in `PluginRegistry::scan`. A plugin written today never has to be re-packaged.

Companion docs:
- [NODE-AUTHORING.md](NODE-AUTHORING.md) — how a single node kind is authored (the SDK, Msg, manifests).
- [NODE-SCOPE.md](../sessions/NODE-SCOPE.md) — the three execution flavours (native, Wasm, process).
- [VERSIONING.md](VERSIONING.md) — capability manifests and install-time matching.
- [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) — why plugin lifecycle lives in the graph, not a parallel registry.
- [OVERVIEW.md](OVERVIEW.md) — deployment profiles that pick the plugins dir.

## What a plugin IS

A plugin is a **directory**, not a file. Directory name = plugin id, reverse-DNS (e.g. `com.acme.hello`). The directory name is **authoritative**; the `id:` field in `plugin.yaml` must match or the plugin is `Failed` at scan time. One plugin owns one directory:

```
plugins/
  com.acme.hello/
    plugin.yaml            REQUIRED — single source of truth
    ui/                    optional — MF remote bundle
      remoteEntry.js
      static/...chunks
    kinds/                 optional — YAML node manifests
      hello_panel.yaml
    native/                optional — native .so/.dll/.dylib (Stage 3c)
    wasm/                  optional — .wasm modules (Stage 3b)
    bin/                   optional — process-plugin binary (Stage 3c)
    signature              optional today, REQUIRED at Stage 10 — detached Ed25519
```

### `plugin.yaml`

The contract. Declares what the plugin contributes and what it needs. `deny_unknown_fields` — typos are parse errors, not silent defaults.

```yaml
id: com.acme.hello
version: 0.1.0
display_name: "Hello plugin"
description: "Reference plugin — UI panel only."

contributes:
  ui:
    entry: ui/remoteEntry.js
    exposes:
      - name: Panel
        module: "./Panel"
        contributes_to: sidebar        # sidebar | property-panel | node:<kind_id>
  kinds: []                            # YAML files under kinds/ registered via spi::KindManifest
  native_lib: null                     # e.g. native/libhello.so (Stage 3c)
  wasm_modules: []                     # (Stage 3b)
  process_bin: null                    # e.g. bin/hello-driver (Stage 3c)

requires:
  - { id: spi.msg,                       range: "^1" }
  - { id: feature.module_federation_host, range: "^1" }
```

One manifest, one format, regardless of flavour mix. Every later stage adds fields here; none replace it.

### Namespace ownership

A plugin **owns its id as a namespace prefix**. Every kind id a plugin contributes (via `kinds/*.yaml` or later via Wasm/native/process) must be equal to the plugin id or a dotted descendant of it:

| Plugin id | Contributable kind ids |
|---|---|
| `com.acme.hello` | `com.acme.hello`, `com.acme.hello.panel`, `com.acme.hello.anything.deeper` |
| `com.acme.hello` | ❌ `acme.core.folder` (not under the plugin's namespace — refused) |
| `com.acme.hello` | ❌ `com.acme.other` (sibling vendor namespace — refused) |

This is enforced in `PluginRegistry::scan` before kinds are registered with `graph::KindRegistry`. Without it, a third-party plugin could shadow a first-party kind (`acme.core.folder`) or squat another vendor's namespace. Violations → plugin `Failed`, structured reason, nothing registered.

### Path-segment encoding for event subjects

Plugin ids contain dots. NATS subjects use dots as token separators ([EVERYTHING-AS-NODE.md § "The event model"](EVERYTHING-AS-NODE.md#the-event-model)). Published verbatim, a plugin node at `/agent/plugins/com.acme.hello` would produce `graph.<tenant>.agent.plugins.com.acme.hello.slot.lifecycle.changed` — the plugin id occupies *four* subject tokens, and wildcard subscriptions like `graph.*.agent.plugins.*.slot.lifecycle.changed` silently fail to match.

Rule: when a graph path segment is converted into a NATS subject token, dots are escaped to `_`. The segment `com.acme.hello` becomes the single token `com_acme_hello`. The escape is applied once, at the event-publisher boundary; internal APIs (REST, CLI, graph store) keep the dotted form. Same rule will apply to any future kind id or path segment that contains dots.

## Where plugins live

Role-aware defaults via the [`config` crate](../../crates/config/) precedence (CLI > env > file > default).

| Role | Default plugins dir | Why |
|---|---|---|
| `standalone` (dev laptop) | `<config_dir>/plugins/` (e.g. `~/.config/<app>/plugins/` on Linux, XDG-resolved) | Stable across working directories; cwd is too brittle — launching from a different shell silently changes what loads. Use `--plugins-dir .` for in-tree dev. |
| `edge` | `/var/lib/<app>/plugins/` | systemd unit, writable by agent user |
| `cloud` | `/opt/<app>/plugins/` (read-only, image-baked) + optional `/var/lib/<app>/plugins/` for fleet-delivered | Containers: one layer immutable, one writable |

Knobs:
- CLI: `--plugins-dir <PATH>`
- Env: `AGENT_PLUGINS_DIR`
- File: `plugins_dir: ...` under `agent:` in `agent.yaml`

Default resolution lives in `Role::default_plugins_dir()` — same pattern as `Role::default_db_path()` already does.

## Loader architecture

Two layers, both in [`crates/extensions-host`](../../crates/extensions-host/) (today a stub — this doc is what fills it in).

### Layer A — `PluginRegistry` (discovery + state)

```rust
pub struct PluginRegistry { /* RwLock<HashMap<PluginId, LoadedPlugin>> */ }

pub struct LoadedPlugin {
    pub id: PluginId,
    pub manifest: PluginManifest,
    pub root: PathBuf,                   // plugins/<id>/
    pub lifecycle: PluginLifecycle,      // Discovered → Validated → Enabled → Disabled → Failed
    pub load_errors: Vec<String>,
}

impl PluginRegistry {
    pub fn scan(dir: &Path, host_caps: &CapabilityManifest) -> Result<Self>;
    pub fn list(&self) -> Vec<LoadedPluginSummary>;
    pub fn get(&self, id: &PluginId) -> Option<LoadedPluginSummary>;
    pub fn enable(&self, id: &PluginId)  -> Result<()>;
    pub fn disable(&self, id: &PluginId) -> Result<()>;
    pub fn reload(&self, id: &PluginId)  -> Result<()>;   // dev ergonomics
}
```

`scan` runs in two phases — **validate everything, then commit** — so a partial failure can't leave the shared `graph::KindRegistry` half-populated.

**Phase 1 — validate all plugin dirs (no shared state touched):**

1. **Parse** `plugin.yaml`. Missing/malformed → status `Failed`, reason recorded.
2. **Check directory name == manifest `id`**. Mismatch → `Failed`.
3. **Validate** `requires ⊆ host_caps` using the matcher in [`crates/spi/src/capabilities.rs`](../../crates/spi/src/capabilities.rs). Missing caps → `Failed` with structured reasons. Mismatches are a **hard fail from day one** — there's no install base to protect, and flipping from warn-to-fail later would be a breaking change we can skip by being strict now.
4. **Parse `kinds/*.yaml`** through `spi::KindManifest` into an in-memory staging set. Validate every kind id is equal to the plugin id or a dotted descendant (see [Namespace ownership](#namespace-ownership)). Violations → `Failed`.

**Phase 2 — commit registrations atomically:**

5. **Register contributions** for every plugin that passed Phase 1 — UI bundle path recorded, staged kinds registered with `graph::KindRegistry` in one batch, process-plugin binary noted for the supervisor.
6. **Transition to `Enabled`** (or `Disabled` if the plugin node's `config.enabled` slot is `false` — see [Plugin state IS a graph node](#plugin-state-is-a-graph-node)).

### Plugin state IS a graph node

Per [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) — no parallel state. Kind: `acme.agent.plugin` (registered in `domain-extensions`). Status slots: `lifecycle`, `version`, `last_error`. Config slots: `enabled` — this is the authoritative store for enable/disable state; there is no parallel table. One node per loaded plugin, under `/agent/plugins/`.

Consequence: Studio subscribes to plugin state changes through the same event bus it uses for everything else. Given the dot-to-underscore escape above, the subject for plugin `com.acme.hello` is `graph.<tenant>.agent.plugins.com_acme_hello.slot.lifecycle.changed`, and a fleet-wide subscription uses `graph.*.agent.plugins.*.slot.lifecycle.changed` — a single `*` per plugin, as intended. No second channel, no second schema.

### Layer B — HTTP surface

In [`crates/transport-rest`](../../crates/transport-rest/). Thin — just reflects registry state and serves bytes.

```
GET  /api/v1/plugins                → [{id, version, lifecycle, contributes}]
GET  /api/v1/plugins/:id            → PluginDetail (full manifest + load errors)
POST /api/v1/plugins/:id/enable     → 204
POST /api/v1/plugins/:id/disable    → 204
POST /api/v1/plugins/reload         → 204   (rescan dir; dev-only; gated by role)

GET  /plugins/:id/*path             → ServeDir(plugins/<id>/ui/) with correct MIME
                                      // Serves MF remoteEntry.js + chunks
```

CORS is already permissive (`CorsLayer::permissive()` in [routes.rs](../../crates/transport-rest/src/routes.rs)), so MF cross-origin `import()` works out of the box.

## Frontend contract (what Studio / host page does)

1. `fetch("/api/v1/plugins")` → list of plugins with `ui.contributes_to` hints.
2. For each `Enabled` plugin, dynamic `import("/plugins/<id>/ui/remoteEntry.js")`.
3. MF negotiates shared singletons (React, React DOM, zustand, tanstack-query — versions pinned in host + all plugin `rsbuild.config.ts`).
4. Mount exposed modules at their declared slot (`sidebar`, `property-panel`, `node:<kind_id>`).

This is the contract the frontend codes against **once** and never has to change — local, fleet-delivered, signed, unsigned, it's all the same wire shape.

## Transport matrix — who uses what wire

One plugin directory can mix flavours; each flavour uses the transport that fits it. **gRPC is used by exactly one flavour — out-of-process plugins.** UI plugins and in-process flavours don't touch it.

| Flavour | Transport | Rationale | Stage |
|---|---|---|---|
| **UI** (MF remote) | Plain HTTP via `ServeDir` at `/plugins/:id/*` + REST for API calls | Browsers can't speak gRPC natively; MF needs static hosting | First landing |
| **Kind YAML only** (manifest, no code) | None — nothing runs | Manifest is registered with `graph::KindRegistry`; behaviour is in-tree | First landing |
| **Native Rust** | Direct trait calls (in-process) | Same binary as the agent; adding gRPC here is ceremony with zero benefit | Stage 3a (SDK exists) |
| **Wasm** | Wasmtime host-function ABI (in-process) | Wasmtime's own call convention, fuel + memory caps enforced at the ABI | Stage 3b (deferred) |
| **Process plugin** | **gRPC over Unix-domain socket** — [`crates/spi/proto/extension.proto`](../../crates/spi/proto/extension.proto) | Out-of-process isolation (cgroup limits, crash containment), polyglot plugin authors (Rust / Go / Python / TS), streaming for `Discover`/`Subscribe`, typed contract versioned per [VERSIONING.md § proto](VERSIONING.md) | Stage 3c (deferred) |

### Process-plugin wire in detail

The contract is fixed in [`extension.proto`](../../crates/spi/proto/extension.proto) — five RPCs, engine is client, plugin is server, Unix-domain socket:

| RPC | Direction | Streaming | Purpose |
|---|---|---|---|
| `Describe` | req/resp | — | Identity, version, declared kinds, capabilities |
| `Discover` | server-stream | ✓ | Enumerate instances (e.g. BACnet Who-Is) |
| `Subscribe` | server-stream | ✓ | Slot change events on nodes the plugin owns |
| `Invoke` | req/resp | — | Operation on a node the plugin owns (e.g. write a point) |
| `Health` | req/resp | — | Liveness + readiness, pinged on a cadence |

Proto is **add-only within a major version** per [VERSIONING.md § Semver rules](VERSIONING.md) — CI diffs the `.proto` against the last release and fails on removed/renamed fields. This is the exact guarantee that lets process plugins written today keep working in two years.

### Plugin authors never hand-write gRPC

The `extensions-sdk` crate exposes a one-liner: `extensions_sdk::run_process_plugin()`. Plugin authors write `NodeBehavior` impls (the same trait in-process kinds use) and the SDK multiplexes them behind the proto. Non-Rust authors get generated `tonic`-equivalent clients for Go / Python / TS from the same `.proto` — the SDK surface is language-idiomatic, the wire is uniform.

### What plugin.yaml declares

Process plugins populate `contributes.process_bin: bin/<name>` — nothing else. The supervisor:
1. Spawns the binary with a fresh UDS path passed via env.
2. Awaits the socket, opens a gRPC client.
3. Calls `Describe` — kinds and capabilities match the manifest's declared set or the plugin is refused (`Failed`).
4. Subscribes to graph events the plugin needs, wires responses through `Invoke`.
5. Pings `Health` on a cadence; restarts with exponential backoff on failure.

All of this is **Stage 3c**. First landing ships only the parse path (recognise `process_bin:` in the manifest, log a warning that the supervisor isn't wired yet).

### MCP contributions

A plugin can contribute MCP tools, resources, and prompts through a `contributes.mcp` block. Full rules (parity, RBAC, off-switches, tier gating) live in [MCP.md § Plugin- and node-contributed tools](MCP.md#plugin--and-node-contributed-tools) — the short version here is the manifest shape and the two invariants plugin authors must know.

```yaml
contributes:
  mcp:
    tools:
      - id: discover_v1                         # registered as "<plugin-last-segment>.discover_v1"
        title: "Discover BACnet devices"
        description_md: "docs/mcp/discover.md"  # static file, included in the signed bundle
        input_schema:  schemas/discover_in.json
        output_schema: schemas/discover_out.json
        tier: read                               # read | write | destructive
        dispatch:
          kind: rest_proxy                       # rest_proxy | node_action — nothing else
          method: POST
          path: /api/v1/plugins/com.acme.bacnet/rpc/discover
    resources:
      - uri_pattern: "bacnet://{device_id}"
        backing: node
        kind_filter: acme.driver.bacnet.device
    prompts:
      - id: investigate_bacnet_fault
        template: prompts/investigate.md
```

Node kinds contribute tools automatically via an `mcp.actions` block in the kind manifest (see [MCP.md](MCP.md#node-kind-manifest--automatic-tool-contribution)) — no separate registration.

Two invariants for plugin authors:

1. **Every MCP tool must have a REST twin.** `dispatch.kind: rest_proxy` points at an existing REST route the plugin exposes; `node_action` dispatches via the uniform `POST /api/v1/nodes/:id/actions/:action` surface. If the REST route is absent at load, the manifest fails to parse. Parity is non-negotiable.
2. **Descriptions are static files in the signed bundle.** `description_md` is never templated at runtime. This is the prompt-injection defence — plugin authors cannot smuggle live data into tool descriptions, whether by accident or design.

Lands in Stage 10 (parse + register) + Stage 14 (MCP server exposes them). First landing does **not** parse `contributes.mcp` — the block is reserved and strict-validated against `deny_unknown_fields` only when the Stage 10 work begins.

### Possible v2 refinement (flagged, not committed)

Once [Stage 7](../sessions/STEPS.md) lands NATS end-to-end, streaming RPCs (`Discover`, `Subscribe`) may migrate to NATS subjects on the existing leaf node — gRPC retains `Describe` / `Invoke` / `Health`. This is a forward-compatible split (the proto doesn't shrink) and lets process plugins reuse the transport the agent is already running. Decision deferred until Stage 7 ships and we measure whether the dual-transport split is worth it; plugin authors depending on streaming gRPC keep working either way.

## Evolution path

| Now (first landing) | Stage 3b / 3c | Stage 10 (Extension lifecycle) |
|---|---|---|
| Discovery by dir scan | + Wasm module loading (Wasmtime, fuel + mem caps) | + Install/upgrade/rollback via `yourapp ext …` CLI |
| UI bundle served via `/plugins/*` | + Process-plugin supervisor (gRPC over UDS, cgroups) | + Signed plugins (Ed25519 `signature` verified on load; plugins with a `signature` file that fails verification are refused, not silently accepted) |
| `requires` mismatch = hard fail from day one | (no change — already strict) | + Registry sync from Control Plane (fleet push) |
| Manual enable/disable via REST | + Process health / restart with exponential backoff | + Kind migration runner on load |
| Plugin lifecycle as `acme.agent.plugin` nodes | + `acme.agent.plugin.process`, `.wasm`, `.native` children | + Per-plugin permission grants (capability-based RBAC) |

**Not in the first landing, to stay honest:**

- ❌ Running Rust code from the plugin dir. That's Stage 3c (process) or compile-time link (native). Until then, "plugin" means **UI bundles + kind manifests only**. A plugin can declare kinds whose behaviour is implemented by an in-tree crate (like `domain-compute`); it cannot ship its own behaviour binary yet.
- ❌ Hot-reload of running flows when a plugin is disabled. Stage 10.
- ❌ Signature verification — `signature` file is read but not verified today. Stage 10 flips the switch *and* refuses plugins whose signature file fails verification (no "silently accept because verification is off" footgun once the switch is on).
- ❌ Plugin-contributed *capabilities* (plugins declaring new `spi.*` surfaces). Not a v1 concept; capabilities flow host → plugins, not the reverse.

## First-landing scope (~400 LOC, one PR)

1. **`crates/extensions-host`** fleshed out:
   - `manifest.rs` — `PluginManifest`, `Contributes`, `UiContribution` (serde, `deny_unknown_fields`).
   - `registry.rs` — `PluginRegistry`, `LoadedPlugin`, `PluginLifecycle`, `scan()`.
   - Unit tests: good manifest round-trip, missing field rejected, unknown field rejected, missing-capability → `Failed` with reason.
2. **`crates/config`** adds `plugins_dir` to `AgentConfig` + overlay + `Role::default_plugins_dir()`.
3. **`crates/transport-rest`** adds the 5 routes above. `ServeDir` for `/plugins/*`; `fs` feature enabled in workspace `Cargo.toml` tower-http entry.
4. **`apps/agent`** threads plugins-dir config through, calls `PluginRegistry::scan` at startup, logs `{id, lifecycle, contributes}` per plugin, passes the registry to `AppState`.
5. **Register `acme.agent.plugin` kind** in `domain-extensions` (currently a stub) and seed one node per loaded plugin — Studio subscribes via the same graph events it uses for everything else.
6. **Migrate `examples/plugin-hello`** — add `plugin.yaml`, keep the existing rsbuild build. Makefile target builds + copies `dist/` → `plugins/com.acme.hello/ui/`.
7. **Update `crates/transport-rest/static/index.html`** with a minimal MF host loader (~30 LOC of vanilla JS; Studio-proper in Stage 4 replaces this, but proves the wire end-to-end now).

Deferred seams — each gets a one-line TODO comment pointing at the stage that lands it:
- Process supervisor (Stage 3c) — `registry.rs`: `match contributes.process_bin { Some(_) => warn!("process plugins need Stage 3c"), None => ok }`.
- Signature verification (Stage 10) — `scan()` reads `signature` if present; no-op verify.
- CLI (`yourapp ext …`) — [transport-cli/src/lib.rs](../../crates/transport-cli/src/lib.rs) gets a single stub command that calls the REST API.

## Decisions locked

1. **Plugin id = reverse-DNS directory name** (e.g. `com.acme.hello`). Flat strings are not ids. Directory name is authoritative; manifest `id` must match.
2. **Plugins own their namespace.** Contributed kind ids must be equal to the plugin id or a dotted descendant. No squatting first-party or sibling-vendor namespaces.
3. **Dots in path segments are escaped to `_` when building NATS subject tokens.** Keeps wildcard subscriptions meaningful without forcing plugin ids to avoid dots.
4. **One manifest, forever.** `plugin.yaml` is additive across every stage; no second manifest format will appear.
5. **Plugin lifecycle is a graph node** (`acme.agent.plugin`), not a parallel registry surface. Enable/disable lives on the node's `config.enabled` slot.
6. **`requires ⊆ host_caps` is a hard fail at load time**, from day one. No warn-now-fail-later transition.
7. **`scan` is two-phase (validate all, then commit).** Partial failures never leave the shared kind registry half-populated.
8. **UI bundles are the only plugin-supplied code that runs in the first landing.** Plugin-contributed node kinds are allowed via YAML; their behaviour must be in-tree until Stage 3b/3c.
9. **gRPC is the wire for out-of-process plugins only.** UI (HTTP/MF), native (in-process traits), Wasm (Wasmtime ABI), and kind-YAML contributions do not use gRPC. The contract lives in [`spi/proto/extension.proto`](../../crates/spi/proto/extension.proto); `extensions-sdk` gives plugin authors a one-liner so they never hand-write the service.

## One-line summary

**A plugin is a reverse-DNS-named directory holding one `plugin.yaml` plus optional UI / kinds / Wasm / native / process artefacts; the host scans the plugins dir on boot, validates required capabilities, registers contributions, and exposes both a REST surface and a `ServeDir` so the frontend loads UI bundles via Module Federation — the same contract works today for UI-only plugins and tomorrow for signed, fleet-delivered, process-isolated extensions.**
