//! Host capability manifest — what this agent provides.
//!
//! Per `docs/design/VERSIONING.md` § "Host-provided capability
//! manifest", every running agent exposes its capability set so
//! extensions can install-match against it. The IDs themselves are
//! defined in [`spi::capabilities::platform`] so every crate refers to
//! them by symbol, not string.
//!
//! Stage 3a-bonus: this list is hand-maintained here. Stage 0 of
//! VERSIONING explicitly defers host-side registration to
//! `extensions-host::capability_registry`; once that lands, this
//! function delegates to it instead of holding the list itself. Until
//! then, **adding a new capability requires editing this file** —
//! that's the forcing function so new contracts get a version
//! decision rather than slipping in unannounced.

use semver::Version;
use serde::Serialize;
use spi::capabilities::{platform, Capability};

/// API version exposed under the `/api/v1/` prefix. Bumping requires a
/// `Deprecation`/`Sunset` window per VERSIONING § "Public API".
pub const REST_API_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityManifest {
    pub platform: PlatformInfo,
    pub api: ApiInfo,
    pub capabilities: Vec<Capability>,
    /// SDUI component IR version. Clients use this to refuse
    /// incompatible trees before rendering.
    pub ir_version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformInfo {
    pub version: String,
    pub flow_schema: u32,
    pub node_schema: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiInfo {
    pub rest: String,
}

pub fn host_capabilities() -> CapabilityManifest {
    CapabilityManifest {
        platform: PlatformInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            flow_schema: spi::FLOW_SCHEMA_VERSION,
            node_schema: spi::NODE_SCHEMA_VERSION,
        },
        api: ApiInfo {
            rest: REST_API_VERSION.to_string(),
        },
        capabilities: vec![
            // Contract surfaces shipped by this agent.
            Capability::new(platform::spi_extension_proto(), Version::new(1, 0, 0)),
            Capability::new(platform::spi_msg(), Version::new(1, 0, 0)),
            Capability::new(platform::spi_node_schema(), Version::new(1, 0, 0)),
            Capability::new(platform::spi_flow_schema(), Version::new(1, 0, 0)),
            // Runtime features. Wasm + process plugin land in 3b/3c —
            // intentionally absent so an extension that requires them
            // refuses to install today instead of failing at runtime.
            Capability::new(platform::data_sqlite(), Version::new(3, 45, 0)),
        ],
        ir_version: ui_ir::IR_VERSION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::capabilities::{match_requirements, Requirement, SemverRange};

    #[test]
    fn host_provides_spi_msg_v1() {
        let host = host_capabilities().capabilities;
        let req = vec![Requirement::required(
            platform::spi_msg(),
            SemverRange::caret("1").unwrap(),
        )];
        assert!(match_requirements(&host, &req).is_ok());
    }

    #[test]
    fn host_does_not_provide_wasm_runtime_yet() {
        let host = host_capabilities().capabilities;
        let req = vec![Requirement::required(
            platform::runtime_wasmtime(),
            SemverRange::any(),
        )];
        let err = match_requirements(&host, &req).unwrap_err();
        assert_eq!(
            err.len(),
            1,
            "wasm runtime missing should be the only mismatch"
        );
    }
}
