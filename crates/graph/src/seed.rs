//! Seed kinds registered by every agent at startup.
//!
//! A handful of first-party kinds prove the substrate:
//!
//! * `acme.core.station` — the root container (one-per-graph)
//! * `acme.core.folder` — free container
//! * `acme.compute.math.add` — free leaf (native compute, placeholder
//!   until Stage 3a-2 ships `acme.compute.count`)
//! * `acme.driver.demo`, `.device`, `.point` — a demo bound-kind trio
//!   that proves placement rules work end-to-end.
//!
//! Every kind here is defined by a YAML manifest under `manifests/` in
//! this crate, wired via `#[derive(NodeKind)]` from `extensions-sdk`.
//! The YAML is the single source of truth — placement rules, facets,
//! slot schemas all live in the file. See
//! `docs/sessions/NODE-SCOPE.md` for the broader picture.

use extensions_sdk::NodeKind;

use crate::kind::KindRegistry;

/// Register the built-in kinds on the given registry.
pub fn register_builtins(kinds: &KindRegistry) {
    kinds.register(<Station as NodeKind>::manifest());
    kinds.register(<Folder as NodeKind>::manifest());
    kinds.register(<MathAdd as NodeKind>::manifest());
    kinds.register(<DriverDemo as NodeKind>::manifest());
    kinds.register(<DriverDemoDevice as NodeKind>::manifest());
    kinds.register(<DriverDemoPoint as NodeKind>::manifest());
}

#[derive(extensions_sdk::NodeKind)]
#[node(
    kind = "acme.core.station",
    manifest = "manifests/station.yaml",
    behavior = "none"
)]
pub struct Station;

#[derive(extensions_sdk::NodeKind)]
#[node(
    kind = "acme.core.folder",
    manifest = "manifests/folder.yaml",
    behavior = "none"
)]
pub struct Folder;

#[derive(extensions_sdk::NodeKind)]
#[node(
    kind = "acme.compute.math.add",
    manifest = "manifests/math_add.yaml",
    behavior = "none"
)]
pub struct MathAdd;

#[derive(extensions_sdk::NodeKind)]
#[node(
    kind = "acme.driver.demo",
    manifest = "manifests/driver_demo.yaml",
    behavior = "none"
)]
pub struct DriverDemo;

#[derive(extensions_sdk::NodeKind)]
#[node(
    kind = "acme.driver.demo.device",
    manifest = "manifests/driver_demo_device.yaml",
    behavior = "none"
)]
pub struct DriverDemoDevice;

#[derive(extensions_sdk::NodeKind)]
#[node(
    kind = "acme.driver.demo.point",
    manifest = "manifests/driver_demo_point.yaml",
    behavior = "none"
)]
pub struct DriverDemoPoint;
