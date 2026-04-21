# Blocks

How blocks (a.k.a. blocks) are packaged, discovered, loaded, and served. Long-term vision here; see [STEPS.md](../sessions/STEPS.md) for the staged landing.

**One rule:** every later capability — signing, kind migrations, Wasm, process blocks, fleet push — only *adds fields* to `block.yaml` and *adds branches* in `BlockRegistry::scan`. A block written today never has to be re-packaged.

Companion docs:
- [NODE-AUTHORING.md](NODE-AUTHORING.md) — how a single node kind is authored (the SDK, Msg, manifests).
- [NODE-SCOPE.md](../sessions/NODE-SCOPE.md) — the three execution flavours (native, Wasm, process).
- [VERSIONING.md](VERSIONING.md) — capability manifests and install-time matching.
- [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) — why block lifecycle lives in the graph, not a parallel registry.
- [OVERVIEW.md](OVERVIEW.md) — deployment profiles that pick the blocks dir.

## What a block IS

A block is a **directory**, not a file. Directory name = block id, reverse-DNS (e.g. `com.acme.hello`). The directory name is **authoritative**; the `id:` field in `block.yaml` must match or the block is `Failed` at scan time. One block owns one directory:

```
blocks/
  com.acme.hello/
    block.yaml            REQUIRED — single source of truth
    ui/                    optional — MF remote bundle
      remoteEntry.js
      static/...chunks
    kinds/                 optional — YAML node manifests
      hello_panel.yaml
    native/                optional — native .so/.dll/.dylib (Stage 3c)
    wasm/                  optional — .wasm modules (Stage 3b)
    bin/                   optional — process-block binary (Stage 3c)
    signature              optional today, REQUIRED at Stage 10 — detached Ed25519
```

### `block.yaml`

The contract. Declares what the block contributes and what it needs. `deny_unknown_fields` — typos are parse errors, not silent defaults.

```yaml
id: com.acme.hello
version: 0.1.0
display_name: "Hello block"
description: "Reference block — UI panel only."

contributes:
  ui:
    entry: ui/remoteEntry.js
    exposes:
      - name: Panel
        module: "./Panel"
        contributes_to: sidebar        # sidebar | property-panel | node:<kind_id>
  kinds: []                            # YAML files under kinds/ registered via spi::KindManifest
  builtin: false                       # true ⇒ behaviour is linked into the agent binary (first-party only)
  native_lib: null                     # e.g. native/libhello.so (Stage 3c)
  wasm_modules: []                     # (Stage 3b)
  process_bin: null                    # e.g. bin/hello-driver (Stage 3c)

requires:
  - { id: spi.msg,                       range: "^1" }
  - { id: feature.module_federation_host, range: "^1" }
```

One manifest, one format, regardless of flavour mix. Every later stage adds fields here; none replace it.

### Namespace ownership

A block **owns its id as a namespace prefix**. Every kind id a block contributes (via `kinds/*.yaml` or later via Wasm/native/process) must be equal to the block id or a dotted descendant of it:

| Block id | Contributable kind ids |
|---|---|
| `com.acme.hello` | `com.acme.hello`, `com.acme.hello.panel`, `com.acme.hello.anything.deeper` |
| `com.acme.hello` | ❌ `sys.core.folder` (not under the block's namespace — refused) |
| `com.acme.hello` | ❌ `com.acme.other` (sibling vendor namespace — refused) |

This is enforced in `BlockRegistry::scan` before kinds are registered with `graph::KindRegistry`. Without it, a third-party block could shadow a first-party kind (`sys.core.folder`) or squat another vendor's namespace. Violations → block `Failed`, structured reason, nothing registered.

### Path-segment encoding for event subjects

Block ids contain dots. NATS subjects use dots as token separators ([EVERYTHING-AS-NODE.md § "The event model"](EVERYTHING-AS-NODE.md#the-event-model)). Published verbatim, a block node at `/agent/blocks/com.acme.hello` would produce `graph.<tenant>.agent.blocks.com.acme.hello.slot.lifecycle.changed` — the block id occupies *four* subject tokens, and wildcard subscriptions like `graph.*.agent.blocks.*.slot.lifecycle.changed` silently fail to match.

Rule: when a graph path segment is converted into a NATS subject token, dots are escaped to `_`. The segment `com.acme.hello` becomes the single token `com_acme_hello`. The escape is applied once, at the event-publisher boundary; internal APIs (REST, CLI, graph store) keep the dotted form. Same rule will apply to any future kind id or path segment that contains dots.

## Where blocks live

Role-aware defaults via the [`config` crate](../../crates/config/) precedence (CLI > env > file > default).

| Role | Default blocks dir | Why |
|---|---|---|
| `standalone` (dev laptop) | `<config_dir>/blocks/` (e.g. `~/.config/<app>/blocks/` on Linux, XDG-resolved) | Stable across working directories; cwd is too brittle — launching from a different shell silently changes what loads. Use `--blocks-dir .` for in-tree dev. |
| `edge` | `/var/lib/<app>/blocks/` | systemd unit, writable by agent user |
| `cloud` | `/opt/<app>/blocks/` (read-only, image-baked) + optional `/var/lib/<app>/blocks/` for fleet-delivered | Containers: one layer immutable, one writable |

Knobs:
- CLI: `--blocks-dir <PATH>`
- Env: `AGENT_BLOCKS_DIR`
- File: `blocks_dir: ...` under `agent:` in `agent.yaml`

Default resolution lives in `Role::default_plugins_dir()` — same pattern as `Role::default_db_path()` already does.

## Loader architecture

Two layers, both in [`crates/blocks-host`](../../crates/blocks-host/) (today a stub — this doc is what fills it in).

### Layer A — `BlockRegistry` (discovery + state)

```rust
pub struct BlockRegistry { /* RwLock<HashMap<BlockId, LoadedPlugin>> */ }

pub struct LoadedPlugin {
    pub id: BlockId,
    pub manifest: BlockManifest,
    pub root: PathBuf,                   // blocks/<id>/
    pub lifecycle: PluginLifecycle,      // Discovered → Validated → Enabled → Disabled → Failed
    pub load_errors: Vec<String>,
}

impl BlockRegistry {
    pub fn scan(dir: &Path, host_caps: &CapabilityManifest) -> Result<Self>;
    pub fn list(&self) -> Vec<LoadedPluginSummary>;
    pub fn get(&self, id: &BlockId) -> Option<LoadedPluginSummary>;
    pub fn enable(&self, id: &BlockId)  -> Result<()>;
    pub fn disable(&self, id: &BlockId) -> Result<()>;
    pub fn reload(&self, id: &BlockId)  -> Result<()>;   // dev ergonomics
}
```

`scan` runs in two phases — **validate everything, then commit** — so a partial failure can't leave the shared `graph::KindRegistry` half-populated.

**Phase 1 — validate all block dirs (no shared state touched):**

1. **Parse** `block.yaml`. Missing/malformed → status `Failed`, reason recorded.
2. **Check directory name == manifest `id`**. Mismatch → `Failed`.
3. **Validate** `requires ⊆ host_caps` using the matcher in [`crates/spi/src/capabilities.rs`](../../crates/spi/src/capabilities.rs). Missing caps → `Failed` with structured reasons. Mismatches are a **hard fail from day one** — there's no install base to protect, and flipping from warn-to-fail later would be a breaking change we can skip by being strict now.
4. **Parse `kinds/*.yaml`** through `spi::KindManifest` into an in-memory staging set. Validate every kind id is equal to the block id or a dotted descendant (see [Namespace ownership](#namespace-ownership)). Violations → `Failed`.

**Phase 2 — commit registrations atomically:**

5. **Register contributions** for every block that passed Phase 1 — UI bundle path recorded, staged kinds registered with `graph::KindRegistry` in one batch, process-block binary noted for the supervisor.
6. **Transition to `Enabled`** (or `Disabled` if the block node's `config.enabled` slot is `false` — see [Block state IS a graph node](#block-state-is-a-graph-node)).

### Block state IS a graph node

Per [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) — no parallel state. Kind: `sys.agent.block` (registered in `domain-blocks`). Status slots: `lifecycle`, `version`, `last_error`. Config slots: `enabled` — this is the authoritative store for enable/disable state; there is no parallel table. One node per loaded block, under `/agent/blocks/`.

Consequence: Studio subscribes to block state changes through the same event bus it uses for everything else. Given the dot-to-underscore escape above, the subject for block `com.acme.hello` is `graph.<tenant>.agent.blocks.com_acme_hello.slot.lifecycle.changed`, and a fleet-wide subscription uses `graph.*.agent.blocks.*.slot.lifecycle.changed` — a single `*` per block, as intended. No second channel, no second schema.

### Layer B — HTTP surface

In [`crates/transport-rest`](../../crates/transport-rest/). Thin — just reflects registry state and serves bytes.

```
GET  /api/v1/blocks                → [{id, version, lifecycle, contributes}]
GET  /api/v1/blocks/:id            → PluginDetail (full manifest + load errors)
POST /api/v1/blocks/:id/enable     → 204
POST /api/v1/blocks/:id/disable    → 204
POST /api/v1/blocks/reload         → 204   (rescan dir; dev-only; gated by role)

GET  /blocks/:id/*path             → ServeDir(blocks/<id>/ui/) with correct MIME
                                      // Serves MF remoteEntry.js + chunks
```

CORS is already permissive (`CorsLayer::permissive()` in [routes.rs](../../crates/transport-rest/src/routes.rs)), so MF cross-origin `import()` works out of the box.

## Frontend contract (what Studio / host page does)

1. `fetch("/api/v1/blocks")` → list of blocks with `ui.contributes_to` hints.
2. For each `Enabled` block, dynamic `import("/blocks/<id>/ui/remoteEntry.js")`.
3. MF negotiates shared singletons (React, React DOM, zustand, tanstack-query — versions pinned in host + all block `rsbuild.config.ts`).
4. Mount exposed modules at their declared slot (`sidebar`, `property-panel`, `node:<kind_id>`).

This is the contract the frontend codes against **once** and never has to change — local, fleet-delivered, signed, unsigned, it's all the same wire shape.

## Transport matrix — who uses what wire

One block directory can mix flavours; each flavour uses the transport that fits it. **gRPC is used by exactly one flavour — out-of-process blocks.** UI blocks and in-process flavours don't touch it.

| Flavour | Transport | Rationale | Stage |
|---|---|---|---|
| **UI** (MF remote) | Plain HTTP via `ServeDir` at `/blocks/:id/*` + REST for API calls | Browsers can't speak gRPC natively; MF needs static hosting | First landing |
| **Kind YAML only** (manifest, no code) | None — nothing runs | Manifest is registered with `graph::KindRegistry`; behaviour is in-tree | First landing |
| **Built-in Rust** (compiled into the agent) | Direct trait calls (in-process); no transport | First-party block crate linked into the agent binary at build time. Same pattern as [`domain-function`](../../crates/domain-function/) — the block exports `register_kinds()` + `behavior()`, the composition root wires them into `KindRegistry` + `BehaviorRegistry`. No UDS, no gRPC, no supervisor, no process spawn. | First landing |
| **Native Rust** (dynamic `.so`/`.dll`/`.dylib`) | Direct trait calls (in-process) | Same binary ABI as the agent; adding gRPC here is ceremony with zero benefit | Stage 3a (SDK exists) |
| **Wasm** | Wasmtime host-function ABI (in-process) | Wasmtime's own call convention, fuel + memory caps enforced at the ABI | Stage 3b (deferred) |
| **Process block** | **gRPC over Unix-domain socket** — [`crates/spi/proto/block.proto`](../../crates/spi/proto/block.proto) | Out-of-process isolation (cgroup limits, crash containment), polyglot block authors (Rust / Go / Python / TS), streaming for `Discover`/`Subscribe`, typed contract versioned per [VERSIONING.md § proto](VERSIONING.md) | Stage 3c (deferred) |

### Built-in vs process: when to pick which

Built-in blocks trade isolation for simplicity and latency. Both flavours implement the same `NodeKind` + `NodeBehavior` traits — the engine's dispatcher doesn't care which one it's calling. The choice is packaging, not contract.

| Concern | Built-in | Process |
|---|---|---|
| Deployment | One binary | Agent + block binaries |
| Crash isolation | Block crash takes down the agent | Supervisor restarts the block; agent keeps running |
| Capability sandbox | Inherits full agent perms | Per-block capability grant (Stage 10) |
| Latency on `on_message` | Direct call | UDS gRPC round-trip |
| Hot-reload | Requires agent restart | Supervisor can re-spawn |
| Authoring language | Rust only | Rust / Go / Python / TS |
| Agent binary size | Grows with each built-in | Unchanged |

**Rule of thumb:** built-in for first-party, trusted, high-frequency kinds (e.g. `domain-function`, `mqtt-client`, `bacnet`, `modbus`). Process for third-party, untrusted, polyglot, or crash-prone kinds where isolation matters.

### Built-in block shape

A built-in block is a workspace crate under `agent/crates/blocks-<name>/` that exports two functions — no `#[tokio::main]`, no `run_process_plugin`, no `BlockIdentity`:

```rust
pub fn register_kinds(kinds: &KindRegistry);
pub fn behavior_for(kind_id: &KindId) -> Option<Arc<dyn DynBehavior>>;
// or one fn per kind if the block exports multiple:
//   pub fn client_behavior()    -> Arc<dyn DynBehavior>;
//   pub fn publish_behavior()   -> Arc<dyn DynBehavior>;
//   pub fn subscribe_behavior() -> Arc<dyn DynBehavior>;
```

Composition root ([`apps/agent/src/main.rs`](../../crates/apps/agent/src/main.rs)) wires it once, alongside `domain_function::register_kinds`:

```rust
blocks_mqtt::register_kinds(&kinds);
engine.behaviors().register(
    <blocks_mqtt::Client as NodeKind>::kind_id(),
    blocks_mqtt::client_behavior(),
)?;
// repeat for each kind the block contributes
```

The block still ships a `block.yaml` and `kinds/*.yaml` in the blocks dir so Studio sees the same manifest shape as every other block flavour; `BlockRegistry::scan` recognises the block as built-in (see below) and skips process-spawn. Kind manifests are the contract with Studio; the Rust crate is the contract with the engine.

**Manifest marker.** `contributes.builtin: true` tells the loader the block's behaviour is linked into the agent binary — no `process_bin`, no `native_lib`, no `wasm_modules` required (and setting any of them alongside `builtin: true` is a manifest error). At scan time, the loader verifies every kind the manifest declares has a behaviour registered in `BehaviorRegistry`; a declared-but-unregistered kind → `Failed` with a clear reason (catches "forgot to wire it in the composition root" at boot, not at first message).

**Optional Cargo feature.** Gate each built-in crate with a feature (`features = ["builtin-mqtt"]`) so deployments that don't need the kind don't pay for its dependencies. First-party defaults on; strip-down builds turn it off.

**Shared runtime.** Built-in blocks reuse the agent's tokio multi-thread runtime. Background tasks (MQTT event loop, BACnet poll loop, etc.) use `tokio::spawn` against the same runtime — no second `#[tokio::main]`, no runtime nesting.

**No slot-event bus needed.** Process blocks emit via the slot-event bus because `GraphAccess` across the UDS boundary is currently a stub. Built-in blocks call `ctx.write_slot()` / `ctx.emit()` directly through the `NodeCtx` graph handle — one less indirection.

### Process-block wire in detail

The contract is fixed in [`block.proto`](../../crates/spi/proto/block.proto) — five RPCs, engine is client, block is server, Unix-domain socket:

| RPC | Direction | Streaming | Purpose |
|---|---|---|---|
| `Describe` | req/resp | — | Identity, version, declared kinds, capabilities |
| `Discover` | server-stream | ✓ | Enumerate instances (e.g. BACnet Who-Is) |
| `Subscribe` | server-stream | ✓ | Slot change events on nodes the block owns |
| `Invoke` | req/resp | — | Operation on a node the block owns (e.g. write a point) |
| `Health` | req/resp | — | Liveness + readiness, pinged on a cadence |

Proto is **add-only within a major version** per [VERSIONING.md § Semver rules](VERSIONING.md) — CI diffs the `.proto` against the last release and fails on removed/renamed fields. This is the exact guarantee that lets process blocks written today keep working in two years.

### Block authors never hand-write gRPC

The `blocks-sdk` crate exposes a one-liner: `extensions_sdk::run_process_plugin()`. Block authors write `NodeBehavior` impls (the same trait in-process kinds use) and the SDK multiplexes them behind the proto. Non-Rust authors get generated `tonic`-equivalent clients for Go / Python / TS from the same `.proto` — the SDK surface is language-idiomatic, the wire is uniform.

### What block.yaml declares

Process blocks populate `contributes.process_bin: bin/<name>` — nothing else. The supervisor:
1. Spawns the binary with a fresh UDS path passed via env.
2. Awaits the socket, opens a gRPC client.
3. Calls `Describe` — kinds and capabilities match the manifest's declared set or the block is refused (`Failed`).
4. Subscribes to graph events the block needs, wires responses through `Invoke`.
5. Pings `Health` on a cadence; restarts with exponential backoff on failure.

All of this is **Stage 3c**. First landing ships only the parse path (recognise `process_bin:` in the manifest, log a warning that the supervisor isn't wired yet).

### MCP contributions

A block can contribute MCP tools, resources, and prompts through a `contributes.mcp` block. Full rules (parity, RBAC, off-switches, tier gating) live in [MCP.md § Block- and node-contributed tools](MCP.md#block--and-node-contributed-tools) — the short version here is the manifest shape and the two invariants block authors must know.

```yaml
contributes:
  mcp:
    tools:
      - id: discover_v1                         # registered as "<block-last-segment>.discover_v1"
        title: "Discover BACnet devices"
        description_md: "docs/mcp/discover.md"  # static file, included in the signed bundle
        input_schema:  schemas/discover_in.json
        output_schema: schemas/discover_out.json
        tier: read                               # read | write | destructive
        dispatch:
          kind: rest_proxy                       # rest_proxy | node_action — nothing else
          method: POST
          path: /api/v1/blocks/com.acme.bacnet/rpc/discover
    resources:
      - uri_pattern: "bacnet://{device_id}"
        backing: node
        kind_filter: sys.driver.bacnet.device
    prompts:
      - id: investigate_bacnet_fault
        template: prompts/investigate.md
```

Node kinds contribute tools automatically via an `mcp.actions` block in the kind manifest (see [MCP.md](MCP.md#node-kind-manifest--automatic-tool-contribution)) — no separate registration.

Two invariants for block authors:

1. **Every MCP tool must have a REST twin.** `dispatch.kind: rest_proxy` points at an existing REST route the block exposes; `node_action` dispatches via the uniform `POST /api/v1/nodes/:id/actions/:action` surface. If the REST route is absent at load, the manifest fails to parse. Parity is non-negotiable.
2. **Descriptions are static files in the signed bundle.** `description_md` is never templated at runtime. This is the prompt-injection defence — block authors cannot smuggle live data into tool descriptions, whether by accident or design.

Lands in Stage 10 (parse + register) + Stage 14 (MCP server exposes them). First landing does **not** parse `contributes.mcp` — the block is reserved and strict-validated against `deny_unknown_fields` only when the Stage 10 work begins.

### Possible v2 refinement (flagged, not committed)

Once [Stage 7](../sessions/STEPS.md) lands NATS end-to-end, streaming RPCs (`Discover`, `Subscribe`) may migrate to NATS subjects on the existing leaf node — gRPC retains `Describe` / `Invoke` / `Health`. This is a forward-compatible split (the proto doesn't shrink) and lets process blocks reuse the transport the agent is already running. Decision deferred until Stage 7 ships and we measure whether the dual-transport split is worth it; block authors depending on streaming gRPC keep working either way.

## Evolution path

| Now (first landing) | Stage 3b / 3c | Stage 10 (Block lifecycle) |
|---|---|---|
| Discovery by dir scan | + Wasm module loading (Wasmtime, fuel + mem caps) | + Install/upgrade/rollback via `yourapp ext …` CLI |
| UI bundle served via `/blocks/*` | + Process-block supervisor (gRPC over UDS, cgroups) | + Signed blocks (Ed25519 `signature` verified on load; blocks with a `signature` file that fails verification are refused, not silently accepted) |
| `requires` mismatch = hard fail from day one | (no change — already strict) | + Registry sync from Control Plane (fleet push) |
| Manual enable/disable via REST | + Process health / restart with exponential backoff | + Kind migration runner on load |
| Block lifecycle as `sys.agent.block` nodes | + `sys.agent.block.process`, `.wasm`, `.native` children | + Per-block permission grants (capability-based RBAC) |

**Not in the first landing, to stay honest:**

- ❌ Running Rust code *from the block dir*. That's Stage 3c (process) or Stage 3a (dynamic native). **Built-in blocks are different** — the Rust code lives in the agent's workspace and is linked at build time, so it's always available. Until Stage 3a/3c, "block" means **UI bundles + kind manifests + optional built-in behaviour linked at build time**. A block can declare kinds whose behaviour is implemented by an in-tree crate (built-in or `domain-*`); it cannot ship its own behaviour binary loaded from the blocks dir yet.
- ❌ Hot-reload of running flows when a block is disabled. Stage 10.
- ❌ Signature verification — `signature` file is read but not verified today. Stage 10 flips the switch *and* refuses blocks whose signature file fails verification (no "silently accept because verification is off" footgun once the switch is on).
- ❌ Block-contributed *capabilities* (blocks declaring new `spi.*` surfaces). Not a v1 concept; capabilities flow host → blocks, not the reverse.

## First-landing scope (~400 LOC, one PR)

1. **`crates/blocks-host`** fleshed out:
   - `manifest.rs` — `BlockManifest`, `Contributes`, `UiContribution` (serde, `deny_unknown_fields`).
   - `registry.rs` — `BlockRegistry`, `LoadedPlugin`, `PluginLifecycle`, `scan()`.
   - Unit tests: good manifest round-trip, missing field rejected, unknown field rejected, missing-capability → `Failed` with reason.
2. **`crates/config`** adds `blocks_dir` to `AgentConfig` + overlay + `Role::default_plugins_dir()`.
3. **`crates/transport-rest`** adds the 5 routes above. `ServeDir` for `/blocks/*`; `fs` feature enabled in workspace `Cargo.toml` tower-http entry.
4. **`apps/agent`** threads blocks-dir config through, calls `BlockRegistry::scan` at startup, logs `{id, lifecycle, contributes}` per block, passes the registry to `AppState`.
5. **Register `sys.agent.block` kind** in `domain-blocks` (currently a stub) and seed one node per loaded block — Studio subscribes via the same graph events it uses for everything else.
6. **Migrate `examples/block-hello`** — add `block.yaml`, keep the existing rsbuild build. Makefile target builds + copies `dist/` → `blocks/com.acme.hello/ui/`.
7. **Update `crates/transport-rest/static/index.html`** with a minimal MF host loader (~30 LOC of vanilla JS; Studio-proper in Stage 4 replaces this, but proves the wire end-to-end now).

Deferred seams — each gets a one-line TODO comment pointing at the stage that lands it:
- Process supervisor (Stage 3c) — `registry.rs`: `match contributes.process_bin { Some(_) => warn!("process blocks need Stage 3c"), None => ok }`.
- Signature verification (Stage 10) — `scan()` reads `signature` if present; no-op verify.
- CLI (`yourapp ext …`) — [transport-cli/src/lib.rs](../../crates/transport-cli/src/lib.rs) gets a single stub command that calls the REST API.

## Decisions locked

1. **Block id = reverse-DNS directory name** (e.g. `com.acme.hello`). Flat strings are not ids. Directory name is authoritative; manifest `id` must match.
2. **Blocks own their namespace.** Contributed kind ids must be equal to the block id or a dotted descendant. No squatting first-party or sibling-vendor namespaces.
3. **Dots in path segments are escaped to `_` when building NATS subject tokens.** Keeps wildcard subscriptions meaningful without forcing block ids to avoid dots.
4. **One manifest, forever.** `block.yaml` is additive across every stage; no second manifest format will appear.
5. **Block lifecycle is a graph node** (`sys.agent.block`), not a parallel registry surface. Enable/disable lives on the node's `config.enabled` slot.
6. **`requires ⊆ host_caps` is a hard fail at load time**, from day one. No warn-now-fail-later transition.
7. **`scan` is two-phase (validate all, then commit).** Partial failures never leave the shared kind registry half-populated.
8. **UI bundles are the only block-supplied code that runs in the first landing.** Block-contributed node kinds are allowed via YAML; their behaviour must be in-tree until Stage 3b/3c.
9. **gRPC is the wire for out-of-process blocks only.** UI (HTTP/MF), built-in (compiled-in), native (dynamic in-process), Wasm (Wasmtime ABI), and kind-YAML contributions do not use gRPC. The contract lives in [`spi/proto/block.proto`](../../crates/spi/proto/block.proto); `blocks-sdk` gives block authors a one-liner so they never hand-write the service.
10. **Built-in blocks are a first-class packaging flavour**, not a shortcut. Same `block.yaml`, same `NodeKind`/`NodeBehavior` traits, same manifest ownership rules — only difference is `contributes.builtin: true` and the crate is linked into the agent at build time. Reserved for first-party, trusted kinds; third-party blocks must use process (or eventually Wasm) for isolation.

## One-line summary

**A block is a reverse-DNS-named directory holding one `block.yaml` plus optional UI / kinds / built-in-linkage / Wasm / native / process artefacts; the host scans the blocks dir on boot, validates required capabilities, registers contributions (including built-in behaviour linked at compile time), and exposes both a REST surface and a `ServeDir` so the frontend loads UI bundles via Module Federation — the same contract works today for UI-only and built-in blocks and tomorrow for signed, fleet-delivered, process-isolated blocks.**
