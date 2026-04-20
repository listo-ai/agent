#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Block discovery + lifecycle — reads `blocks/<id>/block.yaml`,
//! validates `requires` against the host capability set, registers
//! contributed kinds with [`graph::KindRegistry`], and exposes a
//! read-model for the REST surface.
//!
//! See `docs/design/PLUGINS.md` for the design. This crate owns Layer A
//! ("discovery + state"); the HTTP surface lives in `transport-rest`.
//!
//! Stage-3a-bonus first landing: UI bundles + YAML-declared kinds only.
//! Wasm / native / process blocks get `block.yaml` fields today and
//! their loaders in later stages — the manifest never has to change.

pub mod host;
mod manifest;
mod registry;
pub mod supervisor;
pub mod wasm;

pub use host::{HostError, HostPolicy, BlockHost, PluginRuntimeState};
pub use wasm::{WasmError, WasmLimits, WasmSupervisor};

pub use manifest::{
    Contributes, NativeLibContribution, BlockId, BlockManifest, ProcessBinContribution,
    UiContribution, UiExpose, WasmContribution,
};
pub use registry::{
    LoadedPlugin, LoadedPluginSummary, BlockError, PluginLifecycle, BlockRegistry,
};
pub use supervisor::{ProcessSupervisor, SupervisorError, SOCKET_ENV};
