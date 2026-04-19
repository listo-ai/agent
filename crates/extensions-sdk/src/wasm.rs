//! Wasm-plugin guest-side helpers (feature `wasm`).
//!
//! Plugin authors build a `cdylib` crate that links this SDK with
//! `features = ["wasm"]`, uses [`generate!`] to pull the WIT bindings
//! into their module, implements the `Guest` trait the macro
//! produces, and wraps up with `export!`.
//!
//! Minimal plugin:
//!
//! ```ignore
//! extensions_sdk::wasm::generate!({
//!     path: "../../crates/spi/wit",
//!     world: "plugin",
//! });
//!
//! struct MyPlugin;
//! impl Guest for MyPlugin {
//!     fn describe() -> DescribeResponse { /* … */ }
//!     fn on_input(node_id: String, kind_id: String, port: String, msg_json: String)
//!         -> Result<Vec<OutputMsg>, String> { /* … */ }
//! }
//! extensions_sdk::wasm::export!(MyPlugin with_types_in extensions_sdk::wasm::bindgen);
//! ```
//!
//! The path is relative to the plugin crate's `Cargo.toml` — keep
//! the plugin at `plugins/<vendor>.<name>/` inside this repo so the
//! two-level `..` hop lands on `crates/spi/wit`.

// Re-export wit-bindgen under our namespace so authors don't need a
// direct dep. Kept on a tight version pin to keep proc-macro
// expansions consistent across host + guest bindings.
pub use wit_bindgen::{generate, rt};

/// Re-exported for the `export!` macro's `with_types_in` clause.
///
/// Cleaner than having authors spell out `wit_bindgen` literally —
/// swapping the generator later (wasi-p3, moonbit, whatever) won't
/// break their code.
pub mod bindgen {
    pub use wit_bindgen::*;
}

pub use wit_bindgen::export;
