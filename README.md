# Full Stack — Complete Architecture

## Product shape

**A generic, extensible, flow-based integration platform.** Users author flows in a Studio; the Control Plane ships them to agents; agents execute them against pluggable extensions. Multi-tenant, fleet-native, one Rust codebase that runs on 512 MB ARM gateways and in the cloud alike.

The core bet is a **node/slot/flow** model plus a first-class **extension system**. Anything you want to integrate with — a protocol, an API, a database, a message queue, a service — is an extension that contributes nodes. Everything on the canvas, from compute nodes to device drivers to identity objects, shares one uniform shape.

Common applications include building automation, industrial IoT, home automation, service orchestration, ETL/data pipelines, and internal-tools glue — the platform is not specialised for any one of them. Where the docs use BACnet, Modbus, or HVAC scenarios, they're illustrative examples of what the extension system enables, not the platform's reason for existing.

---

## Engine — crossflow

The open-rmf reactive workflow engine. Apache-2.0, Rust, Bevy ECS. Hierarchical workflows with parallel branches, cycles, serializable diagrams. Compiles to Wasm for browser use. Built-in gRPC and zenoh middleware. Same engine crate in both edge and cloud deployments.

---

## Nodes and extensions — three-layer model

Every node on the canvas follows the same three-layer contract. Flow authors can't tell built-ins from extensions.

**Layer 1 — UI Node** (TypeScript/React). Palette icon, property panel, configuration form. Loaded into the Studio via Module Federation. Forms are schema-driven from the SPI's JSON Schema so extension authors don't hand-build forms.

**Layer 2 — Engine Node** (Rust). Thin adapter inside crossflow. Receives messages, calls out to the relevant extension process or executes inline, emits result messages. Maps between crossflow's message model and the extension.proto schema.

**Layer 3 — Extension process** (separate binary, when needed). Runs as a supervised child process with cgroup memory limits and crash-restart backoff. IPC is gRPC over Unix domain sockets using crossflow's built-in middleware.

Not every node needs Layer 3. A Function or Switch node runs entirely in Layer 2. Protocol extensions, AI-inference extensions, heavy integrations — those benefit from the separate process. Wasm plugin nodes run via Wasmtime inside Layer 2 (sandboxed, no separate process). Native Rust plugin nodes are statically linked into the agent.

Benefits: crash isolation, independent upgrade, license segregation, per-extension resource ceilings, independent language choice (extensions can be Rust, Go, Python — anything that speaks gRPC).

Extensions can target specific deployments. Some only make sense on edge (anything talking to local hardware or a LAN-only protocol); some only in the cloud (anything hitting third-party SaaS); many in both (MQTT, HTTP, databases, generic compute). The extension manifest declares which deployments it targets; the Control Plane refuses to schedule incompatible combinations.

---

## SPI contracts — the source of truth

Three files in `/packages/spi/` that every other package depends on:

- **`extension.proto`** — gRPC contract between engine and extension processes. `describe`, `discover`, `subscribe` (streaming), `invoke`, `health`. Semver, add-only.
- **`node.schema.json`** — JSON Schema defining node manifests. Drives palette rendering, property forms, validation.
- **`flow.schema.json`** — JSON Schema for the flow document format. `"schema_version": 1` field for future migrations.

All three are versioned independently. Rust traits and TypeScript interfaces are generated from these — never hand-maintained in parallel.

---

## Database — SeaORM, SQLite edge / Postgres cloud

Same repository traits, **two implementations** behind them. We don't cripple Postgres to match SQLite.

```yaml
database:
  url: "sqlite:///var/lib/yourapp/app.db"   # edge
  # url: "postgres://user:pass@db:5432/app" # cloud
```

- **Edge (SQLite):** simple schema, single-writer assumption, `TEXT`/`INTEGER` columns. Stores flow definitions, node configurations, device registrations, deployment state, audit log, local cache of org/user/RBAC data sourced from Zitadel, and the cloud-bound outbox.
- **Cloud (Postgres):** native `UUID`, `TIMESTAMPTZ`, `JSONB`, partial and GIN indexes, concurrent writers, partitioning for large tables. Row-level security scoped by tenant where appropriate.

The repository trait is the seam. Logical shape (columns, relationships, constraints) is shared; physical types, indexes, and partitioning diverge per backend. Queries that can't be expressed portably live in backend-specific impls.

**Telemetry and time-series do not live in this OLTP store.** See the telemetry backbone below.

---

## Auth — Zitadel (cloud) + JWT verification (edge)

**Cloud:** Zitadel is the identity provider. Single Go binary, ~100 MB RAM, PostgreSQL backend. Provides OIDC/OAuth2/SAML, native multi-tenancy (Instance → Organization → Project → Application), API-first administration over gRPC/REST, event-sourced audit trail. The Control Plane delegates all user management, login flows, MFA, and SSO to Zitadel.

**Edge:** agents don't run an IdP. They cache Zitadel's JWKS (public keys), verify JWT signatures and claims locally — no network round-trip per request, works through outages. Each edge agent has its own service-account token (long-lived, rotatable) for identifying itself to the Control Plane. RBAC enforced locally against claims in the verified JWT.

---

## Messaging backbone — NATS end-to-end

Edge agent runs a **NATS leaf node**. Cloud runs a **NATS cluster with JetStream**. Edge initiates outbound — long-lived TCP or WebSocket fallback — so home routers, mobile networks, and corporate firewalls work without port forwarding, VPNs, or STUN/TURN.

Local flow-to-flow messaging stays on the leaf (microseconds). Cloud-bound telemetry and commands forward through the tunnel. Link drop → local operation continues → reconverges on restore. JetStream provides durability, KV store, and object store in the cloud. At 512 MB the edge runs JetStream with a modest retention window for offline buffering.

Scatter-gather over subject wildcards is the fleet-query primitive: "firmware version on every gateway in Denver" is one `request()` call with a timeout and a wildcard subject.

**JetStream on edge is opt-in.** At 512 MB the default is NATS Core leaf only; the cloud-bound outbox lives in SQLite. JetStream-on-edge is available where disk and RAM allow (edge x86, standalone), with explicit retention caps.

**Telemetry / time-series path is separate from the OLTP code.** Edge buffers in rolling SQLite tables or on-disk segments; cloud uses **TimescaleDB** (a Postgres extension — hypertables, continuous aggregates, compression, retention policies). The cloud TSDB may share a Postgres instance with the OLTP database, but the *code* is separated behind its own repository trait so time-series access patterns don't bleed into OLTP crates.

**Three-tier communication model:**
- Public API → gRPC/REST at `/api/v1/...` (external tools, CLI, MCP, SDKs)
- Event fabric → NATS (telemetry, flow state, live monitoring, fleet commands). Studio connects here over WebSocket, so this is **tenant-facing**: NATS accounts partition per tenant, subject-level permissions derive from the user's Zitadel JWT claims, and tenant IDs are baked into subject names. Not an "internal" bus.
- Extension IPC → gRPC over Unix socket (engine ↔ one extension process, local)

---

## Wasm sandbox — Wasmtime

At 512 MB the edge runs Wasmtime. Extension authors get real performance, fuel metering for CPU limits, proper sandboxing, language choice (any language that compiles to Wasm).

**Two host-function tables, one ABI.** The host-function *signatures* (`get_input`, `set_output`, `log`, `call_extension`) are defined once, versioned, stable. Implementations differ: Wasmtime binds them to native Rust; the browser binds them via JS imports. WASI is not assumed — we do not rely on WASI-preview1/2 being available in the browser. A Wasm Provider trait abstracts the difference so module source code ports cleanly, but "same `.wasm` artifact runs unmodified in both" is only true when the module uses only our host-function ABI, not WASI directly.

## UI extension isolation — the honest picture

Module Federation loads third-party UI bundles into the Studio at runtime. **MF is a delivery mechanism, not a security boundary** — federated modules share a JS realm with the host, can see `window`, React internals, and tokens in memory. Treat it accordingly:

- **First-party / signed-and-vetted extensions** run directly in the host realm (fast path, full UI richness).
- **Third-party / untrusted extensions** that contribute custom views or widgets are loaded in an **iframe** (or Web Worker for headless compute) with `postMessage` bridging to the host. The extension contract defines the message schema.
- **Property-panel forms** for untrusted extensions are schema-driven (`@rjsf/core` over the extension's `node.schema.json`) — no custom React from untrusted sources required for the common case.

Signature verification proves provenance, not isolation. Both are needed.

---

## Public API — gRPC + REST, `/api/v1/`

URI versioning (`/api/v1/...`, `/api/v2/...`). Add-only within a major version; two supported majors at a time; 12-month deprecation window with `Deprecation` and `Sunset` headers.

**OpenAPI is the source of truth.** Generated from Rust code using `utoipa`. From that single spec we generate the TypeScript SDK, Python SDK, CLI help text, and documentation site. Hand-written OpenAPI is banned.

Versioning scope:
- REST/gRPC API → `/api/v1/`
- Extension SPI → proto package version (`extension.v1`)
- Flow document format → `schema_version` field with migrations
- MCP tool names → include version (`deploy_flow_v1`)

Not versioned: NATS subjects (internal), DB schema (migrations), internal Rust traits.

---

## CLI — clap, same binary as the agent

The edge agent binary *is* the CLI. `./agent run` starts the daemon; `./agent flow list` runs a command.

- **`clap` v4** for the command tree, **`comfy-table`** for tables, **`indicatif`** for progress.
- Every command supports `--output json|yaml|table` for scripting.
- Targets `--local` (local agent over Unix socket) or `--remote <url>` (Control Plane over API). Same commands, either way.
- Config precedence: flags > env (`YOURAPP_*`) > config file (`~/.config/yourapp/config.toml`) > defaults.
- Shell completions generated from clap for bash, zsh, fish, powershell.

---

## MCP server — optional, off by default

Thin adapter over the public API. `./agent mcp serve --stdio` or `--http :3000`. Built with the `rmcp` Rust SDK.

**Three-layer off-switch:**
1. **Build-time feature flag** — customers in locked-down environments get a binary with no MCP code at all.
2. **Config disable** — `mcp: { enabled: false }` in YAML. **Default is off.** Must be explicitly enabled.
3. **Runtime toggle** — `./agent mcp disable` flips live, no restart.

**Security defaults:** binds to `127.0.0.1` only unless configured otherwise; requires an auth token from Zitadel; per-tool permissions tied to the user's RBAC role; every MCP call logged to audit with session ID.

Exposes: flows, devices, extensions, recent telemetry, logs (resources); deploy flow, query device, run dry-run, fetch logs, query time-series (tools); debug/explain/generate prompts (prompts).

---

## Studio — Tauri + Rsbuild + Shadcn

**Shell:** Tauri 2 for Windows/macOS/Linux desktop. Same React app builds to a web target for the browser version.

**Bundler:** Rsbuild + Rspack. Rust-based, fast, Module Federation for runtime plugin loading.

**UI components:** Shadcn + Tailwind. Themeable, accessible, source owned in-repo.

**Plugin UI loading:** Module Federation. Extensions bundle a federated React module; Studio loads it at runtime; extension registers its panels/nodes via the SPI.

**Service wiring:** plain service registry over React Context. No InversifyJS — `reflect-metadata` and Module Federation don't play well together. A `Map<string, Service>` exposed via context is 50 lines and works.

**Flow canvas:** React Flow (or equivalent). Schema-driven property panels generated from the node manifest's JSON Schema.

**State:** Zustand for local UI state. TanStack Query against the Control Plane API for server state.

**Transport:** NATS WebSocket client to the Control Plane for live data; gRPC-Web for API calls. One auth token (from Zitadel) covers both.

---

## Control Plane — cloud service

Same Rust stack as the edge agent, deployed as a horizontally scaled service behind a load balancer. Runs against Postgres instead of SQLite (YAML swap). Provides:

- **Flow authoring backend** — Studio talks here for flow CRUD, validation, versioning
- **Fleet orchestration** — deployment jobs, rollouts, rollbacks across edge agents
- **Telemetry ingest** — time-series from edge agents lands here
- **API surface** — versioned public API per the OpenAPI spec
- **NATS cluster** — messaging backbone terminates here
- **Zitadel integration** — delegates all auth to Zitadel; mirrors org/user data into its own DB for reference

---

## Monorepo layout

See [docs/design/CODE-LAYOUT.md](docs/design/CODE-LAYOUT.md) for the full crate breakdown with layer rules. Summary:

```
/crates                      # Rust — Cargo workspace
  /spi                       # extension.proto + JSON schemas + trait signatures
  /query /auth /messaging /audit /observability /config   # cross-cutting libs
  /data/{entities,repos,sqlite,postgres,tsdb}             # data layer — OLTP split per backend, separate TSDB
  /domain/{flows,devices,extensions,fleet}                # pure business logic
  /transport/{rest,grpc,nats,cli,mcp}                     # external surfaces
  /engine                    # crossflow integration, node runtime
  /extensions-{sdk,host}     # extension SDK + supervisor
  /apps/agent                # the single binary — role selected at runtime, features per target

/sdks
  /sdk-ts                    # generated TypeScript SDK for the public API
  /sdk-python                # generated Python SDK for the public API

/studio                      # Tauri + React app (separate pnpm workspace)

/extensions                  # first-party extensions
  /bacnet /modbus /mqtt /opcua /mbus
  /slack                     # example cloud-side integration
  /ml-inference              # example compute extension
```

Single pnpm workspace for JS (Studio + SDKs), single Cargo workspace for Rust. Edge vs cloud vs standalone is one `apps/agent` binary with Cargo features, not separate packages.

---

## Build targets

- `aarch64-unknown-linux-gnu` — edge (primary)
- `armv7-unknown-linux-gnueabihf` — older edge hardware if needed
- `x86_64-unknown-linux-gnu` — cloud
- `x86_64-pc-windows-msvc` — Studio
- `x86_64-apple-darwin`, `aarch64-apple-darwin` — Studio
- `wasm32-unknown-unknown` — browser Studio build + Wasm extensions

Cross-compilation via `cross` or Docker. Conditional compilation via Cargo features to gate native-only code out of the browser build.

---

## One-sentence summary

**crossflow engine + three-layer node model with gRPC-over-UDS extension processes + NATS leaf-to-cloud messaging + SeaORM on SQLite/Postgres + Zitadel for auth + Tauri/Rsbuild/Shadcn Studio with Module Federation + Wasmtime sandbox + clap CLI + optional off-by-default MCP server + URI-versioned OpenAPI-first public API — same Rust codebase, YAML-switchable between edge and cloud.**
