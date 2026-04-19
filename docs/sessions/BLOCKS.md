# Blocks — Naming & Terminology

Replaces the term "plugin" (and the informal alias "extension") across all layers of the product.

## The three Block types

A **Block** is the installable unit — a directory, a manifest (`block.yaml`), the thing you ship, discover, and install. Every block has a type that describes what it gives the user.

| Type | User value | Name |
|---|---|---|
| Frontend / UI | New screens, panels, dashboards | **View Block** |
| Wasm | New logic, rules, processing | **Flow Block** |
| Rust / native | New device, protocol, or API connections | **Connect Block** |

A single block may contribute more than one type (e.g. a Modbus Connect Block that also ships a View Block for its configuration panel). The `block.yaml` manifest declares all contributions.

---

## View Block

Contributes UI to the Studio — sidebar panels, dashboard widgets, property panels, full-page views. Delivered as a Module Federation remote bundle (`ui/remoteEntry.js`). Runs entirely in the browser or Tauri shell; no agent-side code.

**Examples:** a custom dashboard for a building floor plan, a camera feed panel, a chart widget for sensor history.

---

## Flow Block

Contributes processing logic to the engine — custom node kinds that run inside the flow graph. Packaged as `.wasm` modules compiled to `wasm32-wasip1`. Sandboxed, portable, runs on cloud, edge, and standalone.

**Examples:** a PID controller node, a custom alerting rule, an ML inference node, a unit-conversion utility.

---

## Connect Block

Contributes connectivity — native Rust code that bridges external devices, protocols, or cloud APIs into the platform. Packaged as a native shared library (`.so` / `.dll` / `.dylib`) or a standalone process binary. Runs inside or alongside the agent.

**Examples:** a BACnet/IP driver, a Modbus RTU driver, a Salesforce sync adapter, an MQTT bridge.

---

## Vocabulary rules

| Old term | New term |
|---|---|
| plugin | block |
| plugin.yaml | block.yaml |
| plugins/ directory | blocks/ directory |
| Plugin Manager (UI) | Block Manager |
| frontend plugin | View Block |
| Wasm plugin | Flow Block |
| native / process plugin | Connect Block |
| extension (informal) | block (preferred) / extension (internal code only) |

The word **extension** is kept in internal code layer names (`extensions-host`, `extensions-sdk`, `domain-extensions`) because those crates predate this terminology and renaming them is a separate mechanical task. In all user-facing copy, docs, and API surface use **block**.

---

## How to talk about blocks

- "Install the Modbus **Connect Block**" — not "install the Modbus plugin"
- "Build a **Flow Block** that runs a PID algorithm" — not "write a Wasm extension"
- "The **View Block** adds a live floor-plan panel to your dashboard"
- "This block contributes both a **Connect Block** and a **View Block**" — when a single package ships both

---

## Developer mapping

| Concept | Internal crate / field |
|---|---|
| Block manifest | `block.yaml` → `BlockManifest` struct |
| Block registry / scanner | `BlockRegistry` (was `PluginRegistry`) |
| View Block contribution | `contributes.ui` in `block.yaml` |
| Flow Block contribution | `contributes.wasm_modules` in `block.yaml` |
| Connect Block contribution | `contributes.native_lib` / `contributes.process_bin` in `block.yaml` |
| Host that loads all blocks | `crates/extensions-host` (name unchanged) |
| SDK for block authors | `crates/extensions-sdk` (name unchanged) |
