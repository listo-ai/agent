#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Plugin discovery + lifecycle — reads `plugins/<id>/plugin.yaml`,
//! validates `requires` against the host capability set, registers
//! contributed kinds with [`graph::KindRegistry`], and exposes a
//! read-model for the REST surface.
//!
//! See `docs/design/PLUGINS.md` for the design. This crate owns Layer A
//! ("discovery + state"); the HTTP surface lives in `transport-rest`.
//!
//! Stage-3a-bonus first landing: UI bundles + YAML-declared kinds only.
//! Wasm / native / process plugins get `plugin.yaml` fields today and
//! their loaders in later stages — the manifest never has to change.

mod manifest;
mod registry;
pub mod supervisor;

pub use manifest::{
    Contributes, NativeLibContribution, PluginId, PluginManifest, ProcessBinContribution,
    UiContribution, UiExpose, WasmContribution,
};
pub use registry::{
    LoadedPlugin, LoadedPluginSummary, PluginError, PluginLifecycle, PluginRegistry,
};
pub use supervisor::{ProcessSupervisor, SupervisorError, SOCKET_ENV};
