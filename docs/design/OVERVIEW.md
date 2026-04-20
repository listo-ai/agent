![Full Stack Architecture](../../full_stack_architecture.svg)



# Target Deployments

## Deployment profiles

One binary, one codebase. Role selected at startup via `--role` flag or config. These are the supported combinations.

| Profile | Role | Host | Typical hardware | What runs | Database | NATS |
|---|---|---|---|---|---|---|
| **Cloud — multi-tenant SaaS** | `cloud` | Linux containers, Kubernetes or VMs | Horizontally scaled, 2–8 GB per pod | Control Plane API, fleet orchestrator, cloud-side engine, cloud-only blocks | Postgres (managed, HA) | NATS cluster with JetStream |
| **Cloud — single-tenant** | `cloud` | Linux VM, Docker, or bare metal | 1 VM, 2–4 GB RAM | Same as above, single replica | Postgres (single instance) | NATS single-node with JetStream |
| **Edge — ARM gateway** | `edge` | Raspberry Pi, industrial gateway | aarch64, 512 MB RAM, 8+ GB storage | Engine, local blocks, NATS leaf, local SQLite | SQLite | NATS leaf (Core only; JetStream off) |
| **Edge — x86 gateway** | `edge` | Industrial PC, Intel NUC | x86_64, 1–4 GB RAM | Same as ARM edge; JetStream optional | SQLite | NATS leaf (JetStream opt-in) |
| **Edge — legacy ARM** | `edge` | Older hardware | armv7, 256–512 MB RAM | Stripped features, no JetStream, reduced outbox | SQLite | NATS leaf (Core only) |
| **Standalone appliance** | `standalone` | Single box on-prem | 2–4 GB RAM | Everything — Control Plane + engine + blocks | SQLite or embedded Postgres | NATS embedded single-node |
| **Developer laptop** | `standalone` | macOS / Windows / Linux | Dev machine | Everything in one process | SQLite | NATS embedded |
| **Studio — Windows desktop** | (client) | Windows 10/11 | Any modern PC | Tauri shell, local Studio UI | — | NATS WebSocket client |
| **Studio — macOS desktop** | (client) | macOS 12+ | Intel + Apple Silicon | Tauri shell, local Studio UI | — | NATS WebSocket client |
| **Studio — Linux desktop** | (client) | Ubuntu, Fedora, Arch | AppImage, .deb, .rpm | Tauri shell, local Studio UI | — | NATS WebSocket client |
| **Studio — browser** | (client) | Any modern browser | Desktop or mobile browser | Static SPA | — | NATS WebSocket client |
| **Studio — iOS** (future) | (client) | iOS 16+ | iPhone, iPad | Tauri mobile shell | — | NATS WebSocket client |
| **Studio — Android** (future) | (client) | Android 10+ | Phones, tablets | Tauri mobile shell | — | NATS WebSocket client |

## Build targets (Rust triples)

| Target | Triple | Used by | Cross-compile from |
|---|---|---|---|
| Edge ARM 64 | `aarch64-unknown-linux-gnu` | Edge agent on Pi 4/5, industrial gateways | Any, via `cross` or Docker |
| Edge ARM 64 (musl) | `aarch64-unknown-linux-musl` | Static edge binary, Alpine-based images | Any, via `cross` |
| Edge ARM 32 | `armv7-unknown-linux-gnueabihf` | Older ARM gateways | Any, via `cross` |
| Edge / cloud x86 | `x86_64-unknown-linux-gnu` | Cloud servers, x86 gateways | Native Linux or `cross` |
| Edge / cloud x86 (musl) | `x86_64-unknown-linux-musl` | Static binaries for Alpine / scratch containers | Native Linux or `cross` |
| Studio Windows | `x86_64-pc-windows-msvc` | Tauri desktop | Windows native, or Linux via `cargo-xwin` |
| Studio macOS Intel | `x86_64-apple-darwin` | Tauri desktop on Intel Macs | macOS native (notarization requires it) |
| Studio macOS ARM | `aarch64-apple-darwin` | Tauri desktop on Apple Silicon | macOS native |
| Studio Linux | `x86_64-unknown-linux-gnu` | Tauri desktop | Native Linux |
| Browser Studio | `wasm32-unknown-unknown` | Any Wasm crates needed in-browser | Any |
| Wasm blocks | `wasm32-wasip1` or `wasm32-unknown-unknown` | Block authors, not us | Any |

## Distribution artifacts

| Artifact | Format | Target | Distribution channel |
|---|---|---|---|
| Edge agent — ARM 64 | Single static binary + systemd unit | `aarch64-unknown-linux-gnu` | Debian repo, OCI image, direct download, OTA via Control Plane |
| Edge agent — x86 | Single static binary + systemd unit | `x86_64-unknown-linux-gnu` | Same |
| Edge agent — Docker | OCI image | Multi-arch manifest (ARM 64, x86) | Docker Hub, GHCR, private registry |
| Cloud — OCI image | Docker image | Multi-arch | Private registry, Kubernetes |
| Cloud — Helm chart | Helm package | Kubernetes | Helm repo |
| Standalone appliance | Single binary or OCI image | x86 or ARM | Direct download, Docker |
| Studio Windows | `.msi` + auto-updating `.exe` | Windows | Direct download, winget, auto-update |
| Studio macOS | Notarized `.dmg` (universal) | macOS | Direct download, Homebrew cask, auto-update |
| Studio Linux | `.AppImage`, `.deb`, `.rpm`, Flatpak | Linux | Direct, apt/rpm repos, Flathub |
| Studio browser | Static SPA | Any browser | Hosted by Control Plane, or CDN, or bundled with edge agent for local admin |
| Studio iOS (future) | `.ipa` | iOS | App Store, TestFlight |
| Studio Android (future) | `.apk` / `.aab` | Android | Play Store, direct APK |

## What runs where — capability matrix

| Capability | Cloud | Edge ARM | Edge x86 | Standalone | Studio desktop | Studio browser |
|---|---|---|---|---|---|---|
| Control Plane API | ✓ | — | — | ✓ | — | — |
| Flow engine | ✓ | ✓ | ✓ | ✓ | — | — |
| Native Rust blocks | ✓ (cloud-side) | ✓ (edge-side) | ✓ (edge-side) | ✓ | — | — |
| Wasm blocks | ✓ | ✓ | ✓ | ✓ | ✓ (preview only) | ✓ (preview only) |
| Protocol blocks (BACnet, Modbus) | — | ✓ | ✓ | ✓ | — | — |
| Cloud-API blocks (Salesforce, Slack) | ✓ | — | — | ✓ | — | — |
| MQTT / HTTP blocks | ✓ | ✓ | ✓ | ✓ | — | — |
| NATS cluster (JetStream) | ✓ | — | — | embedded | — | — |
| NATS leaf | — | ✓ | ✓ | n/a | — | — |
| NATS client over WebSocket | — | — | — | — | ✓ | ✓ |
| SQLite | — | ✓ | ✓ | ✓ | — | — |
| Postgres | ✓ | — | — | optional | — | — |
| Zitadel | ✓ (external) | — | — | ✓ (bundled) | — | — |
| File system access | full | full | full | full | scoped | virtual only |
| Local agent discovery | — | — | — | — | ✓ (Unix socket) | ✓ (WebSocket if exposed) |
| Offline operation | partial | full | full | full | full when paired with local agent | read-only cache |

## Memory budgets

| Deployment | Target RSS | Notes |
|---|---|---|
| Edge ARM 256 MB legacy | ≤ 180 MB | Stripped features, no JetStream, fewer concurrent flows |
| Edge ARM/x86 512 MB | ≤ 350 MB | Full feature set, JetStream with small retention window |
| Standalone 2 GB | ≤ 1.2 GB | Everything in one process, moderate tenant count |
| Cloud pod | 2–8 GB | Horizontally scaled, set per workload |
| Studio desktop | 150–400 MB | Tauri shell + React app + WebView |
| Studio browser | 80–200 MB | No native shell overhead |

## Multi-tenancy

- **Cloud** is multi-tenant; tenant == Zitadel Organization. Postgres row-level security + NATS accounts partition data and subjects per tenant.
- **Edge agent is single-tenant.** One gateway belongs to one tenant. Multi-tenant edge (e.g. MSP hosting multiple customers on one box) is **not supported in v1** — the 350 MB memory budget and shared-process model don't give real isolation, and claiming it would be dishonest. Customers needing hard isolation per tenant on-site get one agent per tenant.
- **Standalone** appliances are single-tenant by definition (one org owns the box).

## Key libraries

One library per concern. Don't introduce alternatives — if a crate is listed here, use it. If you think you need something not on this list, discuss before adding.

| Concern | Crate(s) | Notes |
|---|---|---|
| **Async runtime** | [`tokio`](https://crates.io/crates/tokio) | Multi-thread scheduler everywhere. `#[tokio::main]` only in `apps/agent`. |
| **HTTP server** | [`axum`](https://crates.io/crates/axum) + [`tower-http`](https://crates.io/crates/tower-http) | Routing in `transport-rest`; CORS, tracing, and static-file middleware via `tower-http`. |
| **HTTP client** | [`reqwest`](https://crates.io/crates/reqwest) | `rustls-tls`, no OpenSSL dep. Shared via workspace. |
| **CLI** | [`clap`](https://crates.io/crates/clap) | Derive + env features. Binary-only — domain crates never depend on it. |
| **Serialization** | [`serde`](https://crates.io/crates/serde) + [`serde_json`](https://crates.io/crates/serde_json) + [`serde_yml`](https://crates.io/crates/serde_yml) | All wire types derive `Serialize`/`Deserialize`. JSON on the wire; YAML for manifests and config files. |
| **JSON Schema** | [`schemars`](https://crates.io/crates/schemars) | Derives `JsonSchema` on all settings and slot-schema types for Studio property-panel generation. |
| **Error handling** | [`thiserror`](https://crates.io/crates/thiserror) / [`anyhow`](https://crates.io/crates/anyhow) | `thiserror` in every library crate (typed errors, part of the API). `anyhow` only in `apps/agent` (binary entry point). Never use `anyhow` in a library crate. |
| **Logging / tracing** | [`tracing`](https://crates.io/crates/tracing) + [`tracing-subscriber`](https://crates.io/crates/tracing-subscriber) | Structured spans and events everywhere. `tracing` in libs; subscriber initialised once in `apps/agent`. See [LOGGING.md](LOGGING.md). |
| **Async utilities** | [`async-trait`](https://crates.io/crates/async-trait) + [`async-stream`](https://crates.io/crates/async-stream) + [`tokio-stream`](https://crates.io/crates/tokio-stream) + [`futures-util`](https://crates.io/crates/futures-util) | `async-trait` for object-safe async traits; `async-stream` / `tokio-stream` for `Stream` impls; `futures-util` for combinators. |
| **UUIDs** | [`uuid`](https://crates.io/crates/uuid) | v4 random; serde feature always on. Node IDs, tenant IDs, correlation IDs. |
| **Semver** | [`semver`](https://crates.io/crates/semver) | Block capability manifests and kind versioning. See [VERSIONING.md](VERSIONING.md). |
| **Proc-macro helpers** | [`syn`](https://crates.io/crates/syn) + [`quote`](https://crates.io/crates/quote) + [`proc-macro2`](https://crates.io/crates/proc-macro2) | Used only in `blocks-sdk-macros`. Do not add proc-macro crates elsewhere. |
| **SQLite** | [`rusqlite`](https://crates.io/crates/rusqlite) (bundled feature) | Edge and standalone persistence. Bundled so the binary has zero system-lib dependencies. Only `data-sqlite` depends on it. |
| **PostgreSQL** | [`sqlx`](https://crates.io/crates/sqlx) or [`sea-query`](https://crates.io/crates/sea-query) *(planned)* | Cloud persistence. Only `data-postgres` will depend on it — no SQL in domain crates, ever. |
| **NATS** | [`async-nats`](https://crates.io/crates/async-nats) *(planned)* | JetStream for cloud; leaf-node for edge. Only `transport-nats` and `messaging` depend on it. |
| **Zenoh** | [`zenoh`](https://crates.io/crates/zenoh) | Fleet transport for edge-to-edge and edge-to-cloud in constrained environments. Only `transport-fleet-zenoh`. |
| **Wasm host** | [`wasmtime`](https://crates.io/crates/wasmtime) | Sandboxed Wasm block execution. Only `blocks-host` depends on it. Component model + WASI. |
| **Datetime / timezone** | [`jiff`](https://crates.io/crates/jiff) | UTC storage, IANA tz conversion, Rust 2024 idioms. No `chrono`/`time` — use `jiff` for all time math. |
| **Locale / formatting** | [`icu_locale`](https://crates.io/crates/icu_locale) + [`icu_datetime`](https://crates.io/crates/icu_datetime) + [`icu_decimal`](https://crates.io/crates/icu_decimal) (ICU4X) | BCP-47 locale parsing, locale-aware date/number/currency formatting on the presentation edge. |
| **Unit conversion** | [`uom`](https://crates.io/crates/uom) | Type-safe SI units; compile-time dimensional analysis. Used inside `UnitRegistry` — never hand-write conversion factors. |
| **Translations** | [`fluent`](https://crates.io/crates/fluent) + [`fluent-bundle`](https://crates.io/crates/fluent-bundle) | Mozilla Fluent for UI strings and backend message codes. `.ftl` bundles ship with Studio. |
| **ISO 4217 currencies** | [`iso_currency`](https://crates.io/crates/iso_currency) | Static code table. No FX logic — storage and display only. |
| **Testing** | [`tempfile`](https://crates.io/crates/tempfile) | Temporary directories/files in tests. See [TESTS.md](TESTS.md) for the full test pattern library. |

> **Guiding principle per domain:** `tokio` for async, `axum` for HTTP, `serde` for wire, `thiserror` in libs / `anyhow` in binary, `tracing` for observability, `jiff` for time, ICU4X for presentation formatting, `uom` for units, Fluent for translations. Don't mix, don't wrap unnecessarily, don't write your own.

## One-line summary

**One Rust binary cross-compiled to ARM64/ARMv7/x86 Linux for engine roles, plus a Tauri Studio built for Windows/macOS/Linux/iOS/Android and a browser SPA — deployment profile (`cloud`, `edge`, `standalone`) selected at runtime via config, not via separate builds.**
