//! `plugin.yaml` schema.
//!
//! One manifest, forever — every later stage (Wasm, native, process,
//! signing, kind migrations) adds fields here, never replaces the file.
//! `deny_unknown_fields` so typos become parse errors instead of silent
//! no-ops.

use std::fmt;

use semver::Version;
use serde::{Deserialize, Serialize};
use spi::capabilities::Requirement;

/// Reverse-DNS plugin identifier (e.g. `com.acme.hello`).
///
/// Directory name is authoritative at load time; the manifest `id`
/// must match or the plugin is `Failed`. A plugin id must contain at
/// least one dot and no path-hostile characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginId(String);

impl PluginId {
    /// Validate and wrap. Enforces:
    /// - non-empty
    /// - at least one `.` (reverse-DNS shape)
    /// - each dotted segment is non-empty and made of `[a-z0-9-]`
    pub fn parse(s: impl Into<String>) -> Result<Self, InvalidPluginId> {
        let s = s.into();
        if s.is_empty() {
            return Err(InvalidPluginId::Empty);
        }
        if !s.contains('.') {
            return Err(InvalidPluginId::NotReverseDns(s));
        }
        for seg in s.split('.') {
            if seg.is_empty() {
                return Err(InvalidPluginId::EmptySegment(s.clone()));
            }
            if !seg
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                return Err(InvalidPluginId::BadSegment {
                    id: s.clone(),
                    segment: seg.to_string(),
                });
            }
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True if `kind_id` is equal to or a dotted descendant of this
    /// plugin id. Enforces the namespace-ownership rule documented in
    /// PLUGINS.md § "Namespace ownership".
    pub fn owns_kind(&self, kind_id: &str) -> bool {
        kind_id == self.0 || kind_id.starts_with(&format!("{}.", self.0))
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidPluginId {
    #[error("plugin id is empty")]
    Empty,
    #[error("plugin id `{0}` is not reverse-DNS (needs at least one `.`)")]
    NotReverseDns(String),
    #[error("plugin id `{0}` has an empty dotted segment")]
    EmptySegment(String),
    #[error("plugin id `{id}` segment `{segment}` contains forbidden characters (allowed: `[a-z0-9-]`)")]
    BadSegment { id: String, segment: String },
}

/// `plugin.yaml` — the single source of truth a plugin directory ships.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: PluginId,
    pub version: Version,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub contributes: Contributes,

    /// Capability requirements. `requires ⊆ host_caps` is a hard fail
    /// at scan time (see PLUGINS.md § "Decisions locked" #6).
    #[serde(default)]
    pub requires: Vec<Requirement>,
}

/// Everything a plugin contributes to the host. Additive across every
/// stage — new kinds of contribution grow this struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contributes {
    /// Module-Federation UI bundle. Served under `/plugins/<id>/ui/`.
    #[serde(default)]
    pub ui: Option<UiContribution>,

    /// Manifest-declared node kinds under `kinds/*.yaml`. The manifest
    /// file names are just strings — the runtime parses each file at
    /// scan time and validates namespace ownership.
    #[serde(default)]
    pub kinds: Vec<String>,

    /// Native `.so` / `.dll` / `.dylib` — Stage 3c.
    #[serde(default)]
    pub native_lib: Option<NativeLibContribution>,

    /// Wasm modules — Stage 3b.
    #[serde(default)]
    pub wasm_modules: Vec<WasmContribution>,

    /// Process-plugin binary — Stage 3c.
    #[serde(default)]
    pub process_bin: Option<ProcessBinContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiContribution {
    /// Entry point relative to the plugin directory, e.g.
    /// `ui/remoteEntry.js`.
    pub entry: String,
    #[serde(default)]
    pub exposes: Vec<UiExpose>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiExpose {
    pub name: String,
    pub module: String,
    /// Where the exposed module mounts in the host. Free-form today;
    /// Studio validates against its own slot list.
    pub contributes_to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeLibContribution {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmContribution {
    pub path: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessBinContribution {
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_id_accepts_reverse_dns() {
        assert!(PluginId::parse("com.acme.hello").is_ok());
        assert!(PluginId::parse("io.nube.gateway").is_ok());
        assert!(PluginId::parse("a.b").is_ok());
    }

    #[test]
    fn plugin_id_rejects_flat_strings() {
        assert!(PluginId::parse("hello").is_err());
        assert!(PluginId::parse("").is_err());
    }

    #[test]
    fn plugin_id_rejects_bad_chars() {
        assert!(PluginId::parse("com.Acme.hello").is_err()); // uppercase
        assert!(PluginId::parse("com.acme..hello").is_err()); // empty seg
        assert!(PluginId::parse("com.acme.hello!").is_err()); // punctuation
    }

    #[test]
    fn owns_kind_enforces_namespace() {
        let p = PluginId::parse("com.acme.hello").unwrap();
        assert!(p.owns_kind("com.acme.hello"));
        assert!(p.owns_kind("com.acme.hello.panel"));
        assert!(p.owns_kind("com.acme.hello.deeper.still"));
        assert!(!p.owns_kind("com.acme.other"));
        assert!(!p.owns_kind("acme.core.folder"));
        assert!(!p.owns_kind("com.acme.hellox")); // prefix but not dotted descendant
    }

    #[test]
    fn good_manifest_round_trips() {
        let yaml = r#"
id: com.acme.hello
version: 0.1.0
display_name: "Hello plugin"
description: "Reference plugin"
contributes:
  ui:
    entry: ui/remoteEntry.js
    exposes:
      - name: Panel
        module: "./Panel"
        contributes_to: sidebar
  kinds: []
requires:
  - id: spi.msg
    version: "^1"
"#;
        let m: PluginManifest = serde_yml::from_str(yaml).unwrap();
        assert_eq!(m.id.as_str(), "com.acme.hello");
        assert_eq!(m.version, Version::new(0, 1, 0));
        let ui = m.contributes.ui.unwrap();
        assert_eq!(ui.entry, "ui/remoteEntry.js");
        assert_eq!(ui.exposes[0].name, "Panel");
    }

    #[test]
    fn unknown_field_is_rejected() {
        let yaml = r#"
id: com.acme.hello
version: 0.1.0
totally_made_up: true
"#;
        let err = serde_yml::from_str::<PluginManifest>(yaml).unwrap_err();
        assert!(err.to_string().contains("totally_made_up"), "got: {err}");
    }

    #[test]
    fn missing_id_is_rejected() {
        let yaml = "version: 0.1.0\n";
        assert!(serde_yml::from_str::<PluginManifest>(yaml).is_err());
    }
}
