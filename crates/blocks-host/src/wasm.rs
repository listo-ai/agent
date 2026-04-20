//! Wasm-plugin supervisor (core-wasm ABI).
//!
//! One [`WasmSupervisor`] = one loaded `.wasm` module = one plugin.
//! Sandboxed under `wasmtime` with fuel + memory caps.
//!
//! # ABI
//!
//! We deliberately skip the Component Model for the first landing —
//! it would force plugin authors through `wasm-tools component new`
//! / `cargo-component`, which breaks the edge-image build story.
//! The boundary is three exports:
//!
//! ```text
//!   alloc(size: i32) -> i32
//!   describe() -> i64                          // packed (ptr << 32) | len
//!   on_input(env_ptr: i32, env_len: i32) -> i64
//! ```
//!
//! All payloads are JSON bytes. `describe` returns a `DescribeResponse`;
//! `on_input` takes a JSON envelope `{node_id, kind_id, port, msg_json}`
//! and returns `Result<Vec<OutputMsg>, String>`. The envelope dodges
//! wasm's 1-return-value limit without us needing multi-value support.
//!
//! Each call gets a **fresh store / fresh instance**, so memory
//! resets and there's no need for a guest-side `free` — memory is
//! dropped with the store. Fuel is metered per call so a runaway
//! plugin can't monopolise the thread.
//!
//! # Swapping this for Component Model later
//!
//! The `WasmSupervisor::load / describe / on_input` public API is
//! the contract the engine consumes. When component-model tooling
//! settles, the guts here flip without a `plugin.yaml` change.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wasmtime::{Config, Engine, Instance, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

use crate::manifest::PluginId;

/// Defaults chosen to be safe in a shared agent process.
#[derive(Debug, Clone)]
pub struct WasmLimits {
    /// Hard cap on linear memory, in bytes.
    pub max_memory: usize,
    /// Fuel units consumed per `on_input` call. ~1M ≈ 1ms on modern
    /// hardware for typical compute work; 10M is ~10ms.
    pub fuel_per_call: u64,
}

impl Default for WasmLimits {
    fn default() -> Self {
        Self {
            max_memory: 256 * 1024 * 1024,
            fuel_per_call: 10_000_000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WasmError {
    #[error("reading wasm module `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("compiling wasm module `{path}`: {source}")]
    Compile {
        path: PathBuf,
        #[source]
        source: anyhow::Error,
    },
    #[error("instantiating wasm module `{path}`: {source}")]
    Instantiate {
        path: PathBuf,
        #[source]
        source: anyhow::Error,
    },
    #[error(
        "plugin `{plugin}` missing required export `{export}` — not a us-extension plugin?"
    )]
    MissingExport { plugin: String, export: &'static str },
    #[error("describe() on plugin `{plugin}` trapped: {source}")]
    Describe {
        plugin: String,
        #[source]
        source: anyhow::Error,
    },
    #[error("on_input() on plugin `{plugin}` trapped: {source}")]
    OnInput {
        plugin: String,
        #[source]
        source: anyhow::Error,
    },
    #[error("plugin `{plugin}` returned malformed JSON: {source}")]
    BadJson {
        plugin: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("plugin `{expected}` identified itself as `{actual}`")]
    IdentityMismatch { expected: String, actual: String },
    #[error(
        "plugin `{plugin}` declared kind `{kind}` outside its namespace — refused"
    )]
    NamespaceViolation { plugin: String, kind: String },
    #[error("plugin `{plugin}` ran out of fuel (budget {budget})")]
    OutOfFuel { plugin: String, budget: u64 },
}

/// Host state attached to every `Store`. Thin today; grows when we
/// wire NodeCtx imports.
pub struct HostState {
    limits: StoreLimits,
}

impl HostState {
    fn new(max_memory: usize) -> Self {
        Self {
            limits: StoreLimitsBuilder::new().memory_size(max_memory).build(),
        }
    }
}

// ---- wire types ------------------------------------------------------------
//
// JSON-on-the-wire. These mirror the `describe-response` record in
// `crates/spi/wit/plugin.wit` (kept as design reference); the WIT
// file is documentation until we swap this for Component Model.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeResponse {
    pub extension_id: String,
    pub version: String,
    #[serde(default)]
    pub kinds: Vec<KindDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindDecl {
    pub kind_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnInputEnvelope<'a> {
    pub node_id: &'a str,
    pub kind_id: &'a str,
    pub port: &'a str,
    /// Message payload, itself JSON. We keep it as a string here so
    /// the engine can pass opaque `Msg` values without double-encoding
    /// concerns leaking into the ABI.
    pub msg_json: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputMsg {
    pub port: String,
    pub msg_json: String,
}

// ---- supervisor ------------------------------------------------------------

/// A loaded, identity-verified Wasm plugin.
pub struct WasmSupervisor {
    plugin_id: PluginId,
    wasm_path: PathBuf,
    engine: Engine,
    module: Module,
    limits: WasmLimits,
    identity: DescribeResponse,
}

impl std::fmt::Debug for WasmSupervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmSupervisor")
            .field("plugin_id", &self.plugin_id)
            .field("wasm_path", &self.wasm_path)
            .field("identity", &self.identity)
            .finish()
    }
}

impl WasmSupervisor {
    /// Load a compiled `.wasm` module from disk and run `describe()`
    /// to capture identity + declared kinds.
    pub fn load(
        plugin_id: &PluginId,
        wasm_path: &Path,
        limits: WasmLimits,
    ) -> Result<Self, WasmError> {
        let bytes = std::fs::read(wasm_path).map_err(|e| WasmError::Read {
            path: wasm_path.to_path_buf(),
            source: e,
        })?;

        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).map_err(|e| WasmError::Compile {
            path: wasm_path.to_path_buf(),
            source: e,
        })?;
        let module = Module::from_binary(&engine, &bytes).map_err(|e| WasmError::Compile {
            path: wasm_path.to_path_buf(),
            source: e,
        })?;

        let mut sup = Self {
            plugin_id: plugin_id.clone(),
            wasm_path: wasm_path.to_path_buf(),
            engine,
            module,
            limits,
            identity: DescribeResponse {
                extension_id: String::new(),
                version: String::new(),
                kinds: Vec::new(),
            },
        };
        sup.verify_identity()?;
        Ok(sup)
    }

    fn fresh_store_and_instance(&self) -> Result<(Store<HostState>, Instance), WasmError> {
        let mut store = Store::new(&self.engine, HostState::new(self.limits.max_memory));
        store.limiter(|s| &mut s.limits);
        store
            .set_fuel(self.limits.fuel_per_call)
            .map_err(|e| WasmError::Instantiate {
                path: self.wasm_path.clone(),
                source: e.into(),
            })?;
        let linker = Linker::<HostState>::new(&self.engine);
        let instance =
            linker
                .instantiate(&mut store, &self.module)
                .map_err(|e| WasmError::Instantiate {
                    path: self.wasm_path.clone(),
                    source: e,
                })?;
        Ok((store, instance))
    }

    fn verify_identity(&mut self) -> Result<(), WasmError> {
        let (mut store, instance) = self.fresh_store_and_instance()?;

        let describe = instance
            .get_typed_func::<(), i64>(&mut store, "describe")
            .map_err(|_| WasmError::MissingExport {
                plugin: self.plugin_id.as_str().to_string(),
                export: "describe",
            })?;
        let packed = describe
            .call(&mut store, ())
            .map_err(|e| WasmError::Describe {
                plugin: self.plugin_id.as_str().to_string(),
                source: e,
            })?;

        let memory = instance.get_memory(&mut store, "memory").ok_or({
            WasmError::MissingExport {
                plugin: self.plugin_id.as_str().to_string(),
                export: "memory",
            }
        })?;
        let bytes = read_packed(&store, &memory, packed).map_err(|e| WasmError::Describe {
            plugin: self.plugin_id.as_str().to_string(),
            source: e,
        })?;
        let resp: DescribeResponse =
            serde_json::from_slice(&bytes).map_err(|e| WasmError::BadJson {
                plugin: self.plugin_id.as_str().to_string(),
                source: e,
            })?;

        if resp.extension_id != self.plugin_id.as_str() {
            return Err(WasmError::IdentityMismatch {
                expected: self.plugin_id.as_str().to_string(),
                actual: resp.extension_id,
            });
        }
        for k in &resp.kinds {
            if !self.plugin_id.owns_kind(&k.kind_id) {
                return Err(WasmError::NamespaceViolation {
                    plugin: self.plugin_id.as_str().to_string(),
                    kind: k.kind_id.clone(),
                });
            }
        }
        self.identity = resp;
        Ok(())
    }

    pub fn plugin_id(&self) -> &PluginId {
        &self.plugin_id
    }

    pub fn identity(&self) -> &DescribeResponse {
        &self.identity
    }

    /// Dispatch one input message. Each call gets a fresh store and
    /// fuel budget — traps / fuel exhaustion in one call don't affect
    /// the next.
    pub fn on_input(
        &self,
        node_id: &str,
        kind_id: &str,
        port: &str,
        msg_json: &str,
    ) -> Result<Vec<OutputMsg>, WasmError> {
        let envelope_bytes = serde_json::to_vec(&OnInputEnvelope {
            node_id,
            kind_id,
            port,
            msg_json,
        })
        .map_err(|e| WasmError::BadJson {
            plugin: self.plugin_id.as_str().to_string(),
            source: e,
        })?;

        let (mut store, instance) = self.fresh_store_and_instance()?;

        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|_| WasmError::MissingExport {
                plugin: self.plugin_id.as_str().to_string(),
                export: "alloc",
            })?;
        let on_input = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, "on_input")
            .map_err(|_| WasmError::MissingExport {
                plugin: self.plugin_id.as_str().to_string(),
                export: "on_input",
            })?;
        let memory = instance.get_memory(&mut store, "memory").ok_or({
            WasmError::MissingExport {
                plugin: self.plugin_id.as_str().to_string(),
                export: "memory",
            }
        })?;

        // Guest-allocate scratch, write the envelope in.
        let env_len = envelope_bytes.len() as i32;
        let env_ptr = alloc
            .call(&mut store, env_len)
            .map_err(|e| on_input_err(self, e))?;
        memory
            .write(&mut store, env_ptr as usize, &envelope_bytes)
            .map_err(|e| WasmError::OnInput {
                plugin: self.plugin_id.as_str().to_string(),
                source: e.into(),
            })?;

        let packed = on_input
            .call(&mut store, (env_ptr, env_len))
            .map_err(|e| on_input_err(self, e))?;

        let out_bytes = read_packed(&store, &memory, packed).map_err(|e| WasmError::OnInput {
            plugin: self.plugin_id.as_str().to_string(),
            source: e,
        })?;
        let result: Result<Vec<OutputMsg>, String> = serde_json::from_slice(&out_bytes)
            .map_err(|e| WasmError::BadJson {
                plugin: self.plugin_id.as_str().to_string(),
                source: e,
            })?;
        result.map_err(|msg| WasmError::OnInput {
            plugin: self.plugin_id.as_str().to_string(),
            source: anyhow::anyhow!(msg),
        })
    }
}

fn on_input_err(sup: &WasmSupervisor, e: anyhow::Error) -> WasmError {
    let is_fuel = e
        .downcast_ref::<wasmtime::Trap>()
        .map(|t| *t == wasmtime::Trap::OutOfFuel)
        .unwrap_or(false);
    if is_fuel {
        WasmError::OutOfFuel {
            plugin: sup.plugin_id.as_str().to_string(),
            budget: sup.limits.fuel_per_call,
        }
    } else {
        WasmError::OnInput {
            plugin: sup.plugin_id.as_str().to_string(),
            source: e,
        }
    }
}

/// Unpack `(ptr << 32) | len` and read that many bytes from guest
/// memory.
fn read_packed(
    store: &Store<HostState>,
    memory: &wasmtime::Memory,
    packed: i64,
) -> Result<Vec<u8>, anyhow::Error> {
    let ptr = ((packed as u64) >> 32) as usize;
    let len = (packed as u64 & 0xFFFF_FFFF) as usize;
    let data = memory.data(store);
    let slice = data
        .get(ptr..ptr.saturating_add(len))
        .ok_or_else(|| anyhow::anyhow!("guest returned out-of-bounds ptr={ptr} len={len}"))?;
    Ok(slice.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_have_reasonable_defaults() {
        let l = WasmLimits::default();
        assert_eq!(l.max_memory, 256 * 1024 * 1024);
        assert_eq!(l.fuel_per_call, 10_000_000);
    }

    // Integration tests that actually load a `.wasm` fixture live in
    // `tests/wasm_fixture.rs` — they run only when the fixture is
    // present on disk (built from `plugins/com.acme.wasm-demo`), so
    // the test suite doesn't require a wasm toolchain on CI.
}
