#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! # `extensions-sdk` — author SDK for node kinds
//!
//! Every node kind in the platform — core native, Wasm, or process
//! plugin — is written against this SDK. One authoring API, three
//! packaging choices via the mutually-exclusive feature flags `native`
//! (default), `wasm`, `process`.
//!
//! Stage 3a-1 ships the **declarative** half of the SDK: the
//! [`NodeKind`] trait driven by `#[derive(NodeKind)]`, plus the shape
//! of the [`NodeBehavior`] trait that imperative kinds will implement
//! in Stage 3a-2.
//!
//! See `docs/sessions/NODE-SCOPE.md` for the full surface and
//! `docs/sessions/STEPS.md` § "Stage 3" for the current scope.
//!
//! ## Example — manifest-only container kind
//!
//! ```ignore
//! use extensions_sdk::prelude::*;
//!
//! #[derive(NodeKind)]
//! #[node(
//!     kind = "acme.core.folder",
//!     manifest = "manifests/folder.yaml",
//!     behavior = "none",
//! )]
//! pub struct Folder;
//! ```
//!
//! The `manifest` path is resolved relative to the *crate's*
//! `CARGO_MANIFEST_DIR`, not the source file — keep YAMLs under a
//! top-level `manifests/` directory in each crate.

// Exactly one adapter feature may be active per consumer.
#[cfg(any(
    all(feature = "native", feature = "wasm"),
    all(feature = "native", feature = "process"),
    all(feature = "wasm", feature = "process"),
))]
compile_error!(
    "extensions-sdk: features `native`, `wasm`, and `process` are mutually \
     exclusive — enable exactly one on each consuming crate"
);

pub mod error;
pub mod node;
pub mod prelude;

pub use error::NodeError;
// Derive and trait share the name `NodeKind` (macro namespace vs type
// namespace — both resolve unambiguously).
pub use extensions_sdk_macros::NodeKind;
pub use node::{NodeBehavior, NodeCtx, NodeKind};

// Re-exports for the contract surface. Plugin authors refer to these
// through the `prelude`; direct access works too.
pub use spi::capabilities;
pub use spi::{
    Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest,
    MessageId, Msg, NodeId, NodePath, ParentMatcher, SlotRole, SlotSchema,
};

/// Private re-exports consumed by the `#[derive(NodeKind)]` expansion.
/// Not part of the stable surface — do not reference directly.
#[doc(hidden)]
pub mod __private {
    pub use serde_yml;
    pub use spi;
}
