# Blocks — Naming & Terminology

Replaces the term "plugin" (and the informal alias "extension") across all layers of the product.

---

## The three Block types

A **Block** is the installable unit — a directory, a manifest (`block.yaml`), the thing you ship, discover, and install. Every block has a type that describes what it gives the user.

| Type | Technical layer | User value | Name |
|---|---|---|---|
| Frontend / UI | React + Module Federation | New screens, dashboards, and widgets | **View Block** |
| Wasm / Logic | `wasm32-wasip1` module | Custom rules and processing logic | **App Block** |
| Rust / Native | Native `.so` or process binary | Device, protocol, and API connections | **Process Block** |

A single block directory may contribute more than one type (e.g. a Modbus Process Block that also ships a View Block for its configuration panel). The `block.yaml` manifest declares all contributions.

---

## View Block — The Surface

> New screens, dashboards, and widgets.

Contributes UI to the Studio — sidebar panels, dashboard widgets, property panels, full-page views. Delivered as a Module Federation remote bundle (`ui/remoteEntry.js`). Runs entirely in the browser or Tauri shell; zero agent-side code.

MIT UI Software Architecture guidelines describe these as **visual containment trees** — composable UI subtrees that plug into the host shell's layout.

**Examples:** a custom floor-plan dashboard, a live camera feed panel, a sensor history chart widget, a device configuration form.

---

## App Block — The Brain

> Custom rules and processing logic.

Contributes processing logic to the engine — custom node kinds that run inside the flow graph. Packaged as `.wasm` modules compiled to `wasm32-wasip1`. Sandboxed, portable, runs on cloud, edge, and standalone without recompilation.

This handles the **software behavior** that users can customize: what happens to data, how alerts fire, what transformations run.

**Examples:** a PID controller node, a custom alerting rule, an ML inference node, a unit-conversion utility, a data normalizer.

---

## Process Block — The Engine

> Native performance and connectivity.

Contributes connectivity and heavy lifting — native Rust code that bridges external devices, protocols, or cloud APIs into the platform. Packaged as a native shared library (`.so` / `.dll` / `.dylib`) or a standalone process binary. Handles **high-speed data** and **physical device bridges**.

**Examples:** a BACnet/IP driver, a Modbus RTU driver, a Salesforce sync adapter, an MQTT bridge, a serial-port reader.

---

## Vocabulary rules

| Old term | New term |
|---|---|
| plugin | block |
| plugin.yaml | block.yaml |
| plugins/ directory | blocks/ directory |
| Plugin Manager (UI) | Block Manager |
| frontend plugin / UI extension | View Block |
| Wasm plugin / Wasm extension | App Block |
| native / process plugin | Process Block |
| extension (informal) | block (user-facing) |

The word **extension** is retained as internal Rust crate naming convention only. In all user-facing copy, docs, and API surface, use **block**.

---

## How to talk about blocks

- "Install the Modbus **Process Block**" — not "install the Modbus plugin"
- "Build an **App Block** that runs a PID algorithm" — not "write a Wasm extension"
- "The **View Block** adds a live floor-plan panel to your dashboard"
- "This block contributes both a **Process Block** and a **View Block**" — when a single package ships both

---

## Developer mapping

| Concept | Internal crate / field |
|---|---|
| Block manifest | `block.yaml` → `BlockManifest` struct |
| Block registry / scanner | `BlockRegistry` |
| View Block contribution | `contributes.ui` in `block.yaml` |
| App Block contribution | `contributes.wasm_modules` in `block.yaml` |
| Process Block contribution | `contributes.native_lib` / `contributes.process_bin` in `block.yaml` |
| Host that loads all blocks | `crates/blocks-host` |
| SDK for block authors | `crates/blocks-sdk` |
| Domain model | `crates/domain-blocks` |
