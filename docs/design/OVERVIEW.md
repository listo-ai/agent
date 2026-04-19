![Full Stack Architecture](../../full_stack_architecture.svg)



# Target Deployments

## Deployment profiles

One binary, one codebase. Role selected at startup via `--role` flag or config. These are the supported combinations.

| Profile | Role | Host | Typical hardware | What runs | Database | NATS |
|---|---|---|---|---|---|---|
| **Cloud — multi-tenant SaaS** | `cloud` | Linux containers, Kubernetes or VMs | Horizontally scaled, 2–8 GB per pod | Control Plane API, fleet orchestrator, cloud-side engine, cloud-only extensions | Postgres (managed, HA) | NATS cluster with JetStream |
| **Cloud — single-tenant** | `cloud` | Linux VM, Docker, or bare metal | 1 VM, 2–4 GB RAM | Same as above, single replica | Postgres (single instance) | NATS single-node with JetStream |
| **Edge — ARM gateway** | `edge` | Raspberry Pi, industrial gateway | aarch64, 512 MB RAM, 8+ GB storage | Engine, local extensions, NATS leaf, local SQLite | SQLite | NATS leaf (Core only; JetStream off) |
| **Edge — x86 gateway** | `edge` | Industrial PC, Intel NUC | x86_64, 1–4 GB RAM | Same as ARM edge; JetStream optional | SQLite | NATS leaf (JetStream opt-in) |
| **Edge — legacy ARM** | `edge` | Older hardware | armv7, 256–512 MB RAM | Stripped features, no JetStream, reduced outbox | SQLite | NATS leaf (Core only) |
| **Standalone appliance** | `standalone` | Single box on-prem | 2–4 GB RAM | Everything — Control Plane + engine + extensions | SQLite or embedded Postgres | NATS embedded single-node |
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
| Wasm extensions | `wasm32-wasip1` or `wasm32-unknown-unknown` | Extension authors, not us | Any |

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
| Native Rust extensions | ✓ (cloud-side) | ✓ (edge-side) | ✓ (edge-side) | ✓ | — | — |
| Wasm extensions | ✓ | ✓ | ✓ | ✓ | ✓ (preview only) | ✓ (preview only) |
| Protocol extensions (BACnet, Modbus) | — | ✓ | ✓ | ✓ | — | — |
| Cloud-API extensions (Salesforce, Slack) | ✓ | — | — | ✓ | — | — |
| MQTT / HTTP extensions | ✓ | ✓ | ✓ | ✓ | — | — |
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

## One-line summary

**One Rust binary cross-compiled to ARM64/ARMv7/x86 Linux for engine roles, plus a Tauri Studio built for Windows/macOS/Linux/iOS/Android and a browser SPA — deployment profile (`cloud`, `edge`, `standalone`) selected at runtime via config, not via separate builds.**
