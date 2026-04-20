//! Block registry — discovery, validation, lifecycle.
//!
//! `scan` is two-phase by design (PLUGINS.md § "Loader architecture"):
//! Phase 1 parses + validates every block dir into an in-memory
//! staging set without touching shared state. Phase 2 registers
//! contributions on the shared [`graph::KindRegistry`] in one pass.
//! A bad block in Phase 1 never pollutes the kind registry.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use graph::KindRegistry;
use serde::Serialize;
use spi::capabilities::{match_requirements, Capability};
use spi::KindManifest;

use crate::manifest::{BlockId, BlockManifest};

/// Where a block sits in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginLifecycle {
    /// Just found on disk; pre-validation.
    Discovered,
    /// Manifest parsed, capabilities satisfied, kinds namespaced OK.
    Validated,
    /// Contributions registered; serving.
    Enabled,
    /// Validated but intentionally turned off.
    Disabled,
    /// Rejected at some point in the scan.
    Failed,
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub id: BlockId,
    pub manifest: Option<BlockManifest>,
    pub root: PathBuf,
    pub lifecycle: PluginLifecycle,
    pub load_errors: Vec<String>,
}

impl LoadedPlugin {
    pub fn summary(&self) -> LoadedPluginSummary {
        LoadedPluginSummary {
            id: self.id.clone(),
            version: self
                .manifest
                .as_ref()
                .map(|m| m.version.to_string())
                .unwrap_or_default(),
            lifecycle: self.lifecycle,
            display_name: self.manifest.as_ref().and_then(|m| m.display_name.clone()),
            description: self.manifest.as_ref().and_then(|m| m.description.clone()),
            has_ui: self
                .manifest
                .as_ref()
                .map(|m| m.contributes.ui.is_some())
                .unwrap_or(false),
            ui_entry: self
                .manifest
                .as_ref()
                .and_then(|m| m.contributes.ui.as_ref())
                .map(|ui| ui.entry.clone()),
            kinds: self
                .manifest
                .as_ref()
                .map(|m| m.contributes.kinds.clone())
                .unwrap_or_default(),
            load_errors: self.load_errors.clone(),
        }
    }
}

/// Read-model of a loaded block. Serialised on the REST surface.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedPluginSummary {
    pub id: BlockId,
    pub version: String,
    pub lifecycle: PluginLifecycle,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub has_ui: bool,
    pub ui_entry: Option<String>,
    pub kinds: Vec<String>,
    pub load_errors: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BlockError {
    #[error("blocks dir `{0}` is not readable: {1}")]
    DirRead(PathBuf, std::io::Error),
    #[error("block `{id}` not found")]
    NotFound { id: String },
}

/// Thread-safe registry. Registrations happen once at boot (`scan`);
/// lookups are frequent.
///
/// The registry remembers its scan inputs (`dir`, `host_caps`,
/// `kinds`) so `reload()` can re-run discovery without the caller
/// having to plumb them all through again. This matches the REST
/// handler at `POST /api/v1/blocks/reload`, which doesn't have (and
/// shouldn't need) direct access to the host capability set.
#[derive(Debug, Clone)]
pub struct BlockRegistry {
    inner: Arc<RwLock<HashMap<BlockId, LoadedPlugin>>>,
    blocks_dir: Arc<RwLock<Option<PathBuf>>>,
    host_caps: Arc<Vec<Capability>>,
    kinds: KindRegistry,
}

impl Default for BlockRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            blocks_dir: Arc::new(RwLock::new(None)),
            host_caps: Arc::new(Vec::new()),
            kinds: KindRegistry::new(),
        }
    }
}

impl BlockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Walk `dir`, validate every block against `host_caps`, register
    /// contributions on `kinds`. Returns the populated registry.
    ///
    /// A missing directory is **not** an error — it's the common case
    /// on a fresh install. The registry comes back empty.
    pub fn scan(
        dir: &Path,
        host_caps: &[Capability],
        kinds: &KindRegistry,
    ) -> Result<Self, BlockError> {
        let reg = Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            blocks_dir: Arc::new(RwLock::new(Some(dir.to_path_buf()))),
            host_caps: Arc::new(host_caps.to_vec()),
            kinds: kinds.clone(),
        };
        reg.rescan_into()?;
        Ok(reg)
    }

    /// Re-run the scan using the inputs captured at construction.
    /// Plugins that passed validation previously but have since been
    /// removed on disk drop out; new blocks appear; error lists refresh.
    ///
    /// Kinds that were already registered on the shared [`KindRegistry`]
    /// remain — hot-unload is Stage 10 — so reload is purely additive
    /// on the kind side. Block-node slots on the graph are owned by
    /// the binary and refreshed separately (see `apps/agent` startup).
    pub fn reload(&self) -> Result<(), BlockError> {
        self.rescan_into()
    }

    fn rescan_into(&self) -> Result<(), BlockError> {
        let Some(dir) = self.blocks_dir.read().expect("poisoned").clone() else {
            return Ok(());
        };
        if !dir.exists() {
            tracing::info!(dir = %dir.display(), "blocks dir absent — no blocks loaded");
            let mut map = self.inner.write().expect("poisoned");
            map.clear();
            return Ok(());
        }

        // ----- Phase 1: validate every dir into a staging set -----
        let entries = std::fs::read_dir(&dir).map_err(|e| BlockError::DirRead(dir.clone(), e))?;

        let mut staged: Vec<LoadedPlugin> = Vec::new();
        let mut staged_kinds: Vec<(BlockId, Vec<KindManifest>)> = Vec::new();
        let mut seen_ids: HashSet<BlockId> = HashSet::new();

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!(error = %err, "skipping unreadable dir entry");
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let (block, kinds_for) =
                validate_one(&path, &dir_name, self.host_caps.as_ref(), &mut seen_ids);
            staged.push(block);
            if let Some(k) = kinds_for {
                let id = staged.last().expect("just pushed").id.clone();
                staged_kinds.push((id, k));
            }
        }

        // ----- Phase 2: commit to shared state -----
        for (block_id, kind_list) in staged_kinds {
            for k in kind_list {
                if self.kinds.contains(&k.id) {
                    continue;
                }
                tracing::info!(block = %block_id, kind = %k.id, "registering block-contributed kind");
                self.kinds.register(k);
            }
        }

        let mut map = self.inner.write().expect("poisoned");
        map.clear();
        for block in staged {
            map.insert(block.id.clone(), block);
        }
        drop(map);
        Ok(())
    }

    pub fn list(&self) -> Vec<LoadedPluginSummary> {
        let map = self.inner.read().expect("poisoned");
        let mut out: Vec<_> = map.values().map(LoadedPlugin::summary).collect();
        out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        out
    }

    pub fn get(&self, id: &BlockId) -> Option<LoadedPluginSummary> {
        let map = self.inner.read().expect("poisoned");
        map.get(id).map(LoadedPlugin::summary)
    }

    pub fn blocks_dir(&self) -> Option<PathBuf> {
        self.blocks_dir.read().expect("poisoned").clone()
    }

    /// The on-disk root for a loaded block (`<blocks_dir>/<id>`),
    /// or `None` if the block isn't known.
    pub fn plugin_root(&self, id: &BlockId) -> Option<PathBuf> {
        let map = self.inner.read().expect("poisoned");
        map.get(id).map(|p| p.root.clone())
    }

    /// Process-bin contribution for a loaded block, or `None` if the
    /// block isn't a process block / isn't known. Used by the
    /// `BlockHost` to decide which blocks to supervise.
    pub fn process_bin(&self, id: &BlockId) -> Option<crate::manifest::ProcessBinContribution> {
        let map = self.inner.read().expect("poisoned");
        map.get(id)
            .and_then(|p| p.manifest.as_ref())
            .and_then(|m| m.contributes.process_bin.clone())
    }

    /// Flip a Validated/Enabled block to Disabled (or back). Does not
    /// unregister kinds — hot-unload is Stage 10.
    pub fn set_enabled(&self, id: &BlockId, enabled: bool) -> Result<(), BlockError> {
        let mut map = self.inner.write().expect("poisoned");
        let block = map.get_mut(id).ok_or_else(|| BlockError::NotFound {
            id: id.as_str().to_string(),
        })?;
        block.lifecycle = match (block.lifecycle, enabled) {
            (PluginLifecycle::Failed, _) => PluginLifecycle::Failed,
            (_, true) => PluginLifecycle::Enabled,
            (_, false) => PluginLifecycle::Disabled,
        };
        Ok(())
    }
}

fn validate_one(
    dir: &Path,
    dir_name: &str,
    host_caps: &[Capability],
    seen: &mut HashSet<BlockId>,
) -> (LoadedPlugin, Option<Vec<KindManifest>>) {
    let mut errors: Vec<String> = Vec::new();
    let manifest_path = dir.join("block.yaml");

    // 1. Parse block.yaml.
    let bytes = match std::fs::read(&manifest_path) {
        Ok(b) => b,
        Err(e) => {
            errors.push(format!("reading {}: {e}", manifest_path.display()));
            let id = BlockId::parse(dir_name).unwrap_or_else(|_| fallback_id(dir_name));
            return (
                LoadedPlugin {
                    id,
                    manifest: None,
                    root: dir.to_path_buf(),
                    lifecycle: PluginLifecycle::Failed,
                    load_errors: errors,
                },
                None,
            );
        }
    };
    let manifest: BlockManifest = match serde_yml::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            errors.push(format!("parsing block.yaml: {e}"));
            let id = BlockId::parse(dir_name).unwrap_or_else(|_| fallback_id(dir_name));
            return (
                LoadedPlugin {
                    id,
                    manifest: None,
                    root: dir.to_path_buf(),
                    lifecycle: PluginLifecycle::Failed,
                    load_errors: errors,
                },
                None,
            );
        }
    };

    // 2. Directory name must equal manifest id.
    if manifest.id.as_str() != dir_name {
        errors.push(format!(
            "directory name `{dir_name}` does not match manifest id `{}`",
            manifest.id
        ));
    }

    // Validate block id shape (catches anything that bypassed the
    // constructor via raw deserialization — serde accepts any string).
    let block_id = match BlockId::parse(manifest.id.as_str()) {
        Ok(pid) => pid,
        Err(e) => {
            errors.push(e.to_string());
            return (
                LoadedPlugin {
                    id: fallback_id(dir_name),
                    manifest: Some(manifest),
                    root: dir.to_path_buf(),
                    lifecycle: PluginLifecycle::Failed,
                    load_errors: errors,
                },
                None,
            );
        }
    };
    if !seen.insert(block_id.clone()) {
        errors.push(format!("duplicate block id `{block_id}`"));
    }

    // 3. Capability match — hard fail on missing required.
    if let Err(mismatches) = match_requirements(host_caps, &manifest.requires) {
        for m in mismatches {
            errors.push(m.to_string());
        }
    }

    // 4. Parse & namespace-check every kinds/*.yaml the manifest lists.
    let mut parsed_kinds: Vec<KindManifest> = Vec::new();
    for rel in &manifest.contributes.kinds {
        let kind_path = dir.join(rel);
        match std::fs::read(&kind_path) {
            Ok(b) => match serde_yml::from_slice::<KindManifest>(&b) {
                Ok(km) => {
                    if !block_id.owns_kind(km.id.as_str()) {
                        errors.push(format!(
                            "kind `{}` in `{}` is outside block namespace `{}`",
                            km.id, rel, block_id
                        ));
                    } else {
                        parsed_kinds.push(km);
                    }
                }
                Err(e) => errors.push(format!("parsing {rel}: {e}")),
            },
            Err(e) => errors.push(format!("reading {rel}: {e}")),
        }
    }

    // 5. Deferred-seam warnings for Stage 3b/3c features.
    if manifest.contributes.process_bin.is_some() {
        tracing::warn!(
            block = %block_id,
            "process_bin declared — process blocks need Stage 3c"
        );
    }
    if !manifest.contributes.wasm_modules.is_empty() {
        tracing::warn!(
            block = %block_id,
            "wasm_modules declared — wasm blocks need Stage 3b"
        );
    }
    if manifest.contributes.native_lib.is_some() {
        tracing::warn!(
            block = %block_id,
            "native_lib declared — native blocks need Stage 3c"
        );
    }
    if dir.join("signature").exists() {
        tracing::warn!(
            block = %block_id,
            "signature file present — verification lands Stage 10"
        );
    }

    let lifecycle = if errors.is_empty() {
        PluginLifecycle::Enabled
    } else {
        PluginLifecycle::Failed
    };
    let kinds_out = (lifecycle == PluginLifecycle::Enabled).then_some(parsed_kinds);

    (
        LoadedPlugin {
            id: block_id,
            manifest: Some(manifest),
            root: dir.to_path_buf(),
            lifecycle,
            load_errors: errors,
        },
        kinds_out,
    )
}

/// Last-resort id when everything else has failed. The block will be
/// `Failed` regardless; this just keeps the error list indexable.
fn fallback_id(dir_name: &str) -> BlockId {
    BlockId::parse(dir_name).unwrap_or_else(|_| {
        BlockId::parse(format!("invalid.{}", sanitize(dir_name))).expect("sanitized id")
    })
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;
    use spi::capabilities::platform;
    use std::fs;

    fn host() -> Vec<Capability> {
        vec![Capability::new(platform::spi_msg(), Version::new(1, 0, 0))]
    }

    fn write_plugin(root: &Path, id: &str, yaml: &str) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("block.yaml"), yaml).unwrap();
    }

    #[test]
    fn scan_empty_dir_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(tmp.path(), &host(), &kinds).unwrap();
        assert!(reg.list().is_empty());
    }

    #[test]
    fn scan_missing_dir_returns_empty_registry() {
        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(Path::new("/definitely/not/real"), &host(), &kinds).unwrap();
        assert!(reg.list().is_empty());
    }

    #[test]
    fn good_plugin_enables() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(
            tmp.path(),
            "com.acme.hello",
            r#"
id: com.acme.hello
version: 0.1.0
contributes:
  ui:
    entry: ui/remoteEntry.js
requires:
  - id: spi.msg
    version: "^1"
"#,
        );
        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(tmp.path(), &host(), &kinds).unwrap();
        let list = reg.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].lifecycle, PluginLifecycle::Enabled);
        assert!(list[0].load_errors.is_empty());
    }

    #[test]
    fn dir_name_mismatch_fails() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(
            tmp.path(),
            "com.acme.hello",
            r#"
id: com.other.name
version: 0.1.0
"#,
        );
        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(tmp.path(), &host(), &kinds).unwrap();
        let list = reg.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].lifecycle, PluginLifecycle::Failed);
        assert!(list[0]
            .load_errors
            .iter()
            .any(|e| e.contains("does not match manifest id")));
    }

    #[test]
    fn missing_capability_fails() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(
            tmp.path(),
            "com.acme.hello",
            r#"
id: com.acme.hello
version: 0.1.0
requires:
  - id: runtime.wasmtime
    version: "^1"
"#,
        );
        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(tmp.path(), &host(), &kinds).unwrap();
        let list = reg.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].lifecycle, PluginLifecycle::Failed);
        assert!(list[0]
            .load_errors
            .iter()
            .any(|e| e.contains("runtime.wasmtime")));
    }

    #[test]
    fn unknown_field_fails() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(
            tmp.path(),
            "com.acme.hello",
            r#"
id: com.acme.hello
version: 0.1.0
not_a_real_field: true
"#,
        );
        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(tmp.path(), &host(), &kinds).unwrap();
        let list = reg.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].lifecycle, PluginLifecycle::Failed);
    }

    #[test]
    fn kind_outside_namespace_fails_without_registering() {
        let tmp = tempfile::tempdir().unwrap();
        let pdir = tmp.path().join("com.acme.hello");
        fs::create_dir_all(pdir.join("kinds")).unwrap();
        fs::write(
            pdir.join("block.yaml"),
            r#"
id: com.acme.hello
version: 0.1.0
contributes:
  kinds:
    - kinds/bad.yaml
"#,
        )
        .unwrap();
        // A kind id that does NOT live under com.acme.hello.*
        fs::write(
            pdir.join("kinds/bad.yaml"),
            r#"
id: sys.core.folder
containment: {}
"#,
        )
        .unwrap();

        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(tmp.path(), &host(), &kinds).unwrap();
        let list = reg.list();
        assert_eq!(list[0].lifecycle, PluginLifecycle::Failed);
        assert!(list[0]
            .load_errors
            .iter()
            .any(|e| e.contains("outside block namespace")));
        // Phase 2 never ran for this block — shared registry untouched.
        assert!(!kinds.contains(&spi::KindId::new("sys.core.folder")));
    }

    #[test]
    fn kind_in_namespace_registers() {
        let tmp = tempfile::tempdir().unwrap();
        let pdir = tmp.path().join("com.acme.hello");
        fs::create_dir_all(pdir.join("kinds")).unwrap();
        fs::write(
            pdir.join("block.yaml"),
            r#"
id: com.acme.hello
version: 0.1.0
contributes:
  kinds:
    - kinds/panel.yaml
"#,
        )
        .unwrap();
        fs::write(
            pdir.join("kinds/panel.yaml"),
            r#"
id: com.acme.hello.panel
containment: {}
"#,
        )
        .unwrap();

        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(tmp.path(), &host(), &kinds).unwrap();
        assert_eq!(reg.list()[0].lifecycle, PluginLifecycle::Enabled);
        assert!(kinds.contains(&spi::KindId::new("com.acme.hello.panel")));
    }

    #[test]
    fn set_enabled_toggles() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(
            tmp.path(),
            "com.acme.hello",
            r#"
id: com.acme.hello
version: 0.1.0
"#,
        );
        let kinds = KindRegistry::new();
        let reg = BlockRegistry::scan(tmp.path(), &host(), &kinds).unwrap();
        let id = BlockId::parse("com.acme.hello").unwrap();
        reg.set_enabled(&id, false).unwrap();
        assert_eq!(reg.get(&id).unwrap().lifecycle, PluginLifecycle::Disabled);
        reg.set_enabled(&id, true).unwrap();
        assert_eq!(reg.get(&id).unwrap().lifecycle, PluginLifecycle::Enabled);
    }
}
