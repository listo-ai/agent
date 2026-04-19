#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Dashboard runtime — context stack, binding resolver, cache-key.
//!
//! This is M2 of `docs/design/DASHBOARD.md`. No transport, no render-tree
//! producer (M3), no subscription plan (M5). The crate is deliberately
//! framework-only: it reads nodes through a [`NodeReader`] trait so
//! tests run against synthetic fixtures without the graph runtime.

pub mod binding;
pub mod cache;
pub mod reader;
pub mod stack;

pub use binding::{Binding, BindingError, BindingValue, EvalContext, Source};
pub use cache::{hash_page_state, CacheKey, CacheKeyInputs};
pub use reader::{InMemoryReader, NodeReader, NodeSnapshot};
pub use stack::{ContextStack, Frame, StackError};
