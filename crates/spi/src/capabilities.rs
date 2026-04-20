//! Capability registry types — the spine of long-term block compatibility.
//!
//! See `docs/design/VERSIONING.md`. The host publishes what it provides;
//! extensions declare what they need; installation is a set-match.
//!
//! Stage 0 ships only the types and the matcher. Host-side registration
//! lives in `blocks-host::capability_registry` in a later stage.
//!
//! # Example
//!
//! ```
//! use spi::capabilities::{
//!     Capability, Requirement, SemverRange, match_requirements, platform,
//! };
//! use semver::Version;
//!
//! let host = vec![
//!     Capability::new(platform::spi_extension_proto(), Version::new(1, 3, 0)),
//!     Capability::new(platform::spi_msg(), Version::new(1, 0, 0)),
//! ];
//! let extension_needs = vec![
//!     Requirement::required(platform::spi_extension_proto(), SemverRange::caret("1.2").unwrap()),
//! ];
//! assert!(match_requirements(&host, &extension_needs).is_ok());
//! ```

use std::fmt;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

/// Opaque, reverse-DNS-shaped capability identifier.
///
/// String-wrapped so third-party surfaces can declare their own capabilities
/// without editing the SPI crate. Platform-reserved ids are constructed via
/// the `platform` module below.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(String);

impl CapabilityId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! platform_cap {
    ($name:ident, $s:literal) => {
        #[allow(non_snake_case)]
        pub fn $name() -> CapabilityId {
            CapabilityId::new($s)
        }
    };
}

/// Convenience constructors for platform-reserved ids. Prefer these over
/// the `const`-looking associated values above (which exist as future
/// placeholders for when const String is stable).
pub mod platform {
    use super::CapabilityId;

    platform_cap!(spi_extension_proto, "spi.extension.proto");
    platform_cap!(spi_msg, "spi.msg");
    platform_cap!(spi_node_schema, "spi.node.schema");
    platform_cap!(spi_flow_schema, "spi.flow.schema");
    platform_cap!(host_fn_wasm, "host_fn.wasm");
    platform_cap!(runtime_wasmtime, "runtime.wasmtime");
    platform_cap!(runtime_extension_process, "runtime.extension_process");
    platform_cap!(feature_jetstream, "feature.jetstream");
    platform_cap!(feature_tsdb_timescale, "feature.tsdb.timescale");
    platform_cap!(feature_tsdb_sqlite_rolling, "feature.tsdb.sqlite_rolling");
    platform_cap!(data_postgres, "data.postgres");
    platform_cap!(data_sqlite, "data.sqlite");
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A host-provided capability entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub id: CapabilityId,
    pub version: Version,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removal_planned: Option<String>,
}

impl Capability {
    pub fn new(id: CapabilityId, version: Version) -> Self {
        Self {
            id,
            version,
            deprecated_since: None,
            removal_planned: None,
        }
    }
}

/// A semver range expression (`^1.2`, `>=1.0, <2.0`, etc.) backed by
/// `semver::VersionReq`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SemverRange(VersionReq);

impl SemverRange {
    /// Accept anything in the major range: `^X.Y` or `^X`.
    pub fn caret(s: &str) -> Result<Self, semver::Error> {
        VersionReq::parse(&format!("^{}", s.trim_start_matches('^'))).map(Self)
    }

    pub fn parse(s: &str) -> Result<Self, semver::Error> {
        VersionReq::parse(s).map(Self)
    }

    pub fn any() -> Self {
        Self(VersionReq::STAR)
    }

    pub fn matches(&self, v: &Version) -> bool {
        self.0.matches(v)
    }
}

/// A single capability requirement declared by an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub id: CapabilityId,
    #[serde(default = "SemverRange::any")]
    pub version: SemverRange,
    #[serde(default)]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Requirement {
    pub fn required(id: CapabilityId, version: SemverRange) -> Self {
        Self {
            id,
            version,
            optional: false,
            reason: None,
        }
    }

    pub fn optional(id: CapabilityId, version: SemverRange, reason: impl Into<String>) -> Self {
        Self {
            id,
            version,
            optional: true,
            reason: Some(reason.into()),
        }
    }
}

/// Why a requirement could not be satisfied. Kept structured so UIs and
/// CLIs can render actionable messages (see VERSIONING.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    NotProvided {
        id: CapabilityId,
    },
    VersionMismatch {
        id: CapabilityId,
        required: String,
        provided: Version,
    },
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotProvided { id } => {
                write!(f, "required capability `{id}` — not provided on this agent")
            }
            Self::VersionMismatch {
                id,
                required,
                provided,
            } => {
                write!(
                    f,
                    "required capability `{id}` version {required} — host provides {provided}"
                )
            }
        }
    }
}

/// Match an extension's requirements against a host's provided set.
///
/// Returns `Ok` with any unmet *optional* requirements if every required
/// one is satisfied; `Err` listing all unmet required ones otherwise.
pub fn match_requirements(
    host: &[Capability],
    required: &[Requirement],
) -> Result<Vec<Mismatch>, Vec<Mismatch>> {
    let mut unmet_required = Vec::new();
    let mut unmet_optional = Vec::new();

    for req in required {
        let provided = host.iter().find(|c| c.id == req.id);
        match provided {
            None => {
                let m = Mismatch::NotProvided { id: req.id.clone() };
                if req.optional {
                    unmet_optional.push(m);
                } else {
                    unmet_required.push(m);
                }
            }
            Some(cap) if !req.version.matches(&cap.version) => {
                let m = Mismatch::VersionMismatch {
                    id: req.id.clone(),
                    required: format!("{:?}", req.version.0),
                    provided: cap.version.clone(),
                };
                if req.optional {
                    unmet_optional.push(m);
                } else {
                    unmet_required.push(m);
                }
            }
            _ => {}
        }
    }

    if unmet_required.is_empty() {
        Ok(unmet_optional)
    } else {
        Err(unmet_required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Vec<Capability> {
        vec![
            Capability::new(platform::spi_extension_proto(), Version::new(1, 3, 0)),
            Capability::new(platform::spi_msg(), Version::new(1, 0, 0)),
            Capability::new(platform::runtime_extension_process(), Version::new(1, 0, 0)),
        ]
    }

    #[test]
    fn matches_when_all_satisfied() {
        let req = vec![
            Requirement::required(
                platform::spi_extension_proto(),
                SemverRange::caret("1.2").unwrap(),
            ),
            Requirement::required(platform::spi_msg(), SemverRange::caret("1").unwrap()),
        ];
        assert!(match_requirements(&host(), &req).is_ok());
    }

    #[test]
    fn mismatches_when_version_too_low() {
        let req = vec![Requirement::required(
            platform::spi_extension_proto(),
            SemverRange::caret("1.5").unwrap(),
        )];
        let err = match_requirements(&host(), &req).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(err[0], Mismatch::VersionMismatch { .. }));
    }

    #[test]
    fn missing_required_capability_is_an_error() {
        let req = vec![Requirement::required(
            platform::runtime_wasmtime(),
            SemverRange::any(),
        )];
        let err = match_requirements(&host(), &req).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(err[0], Mismatch::NotProvided { .. }));
    }

    #[test]
    fn missing_optional_capability_is_surfaced_but_not_fatal() {
        let req = vec![Requirement::optional(
            platform::feature_tsdb_timescale(),
            SemverRange::any(),
            "historical trends disabled",
        )];
        let unmet = match_requirements(&host(), &req).unwrap();
        assert_eq!(unmet.len(), 1);
    }
}
