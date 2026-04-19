#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Dashboard runtime — binding resolver, node reader, context stack.
//!
//! Framework-only: reads nodes through the [`NodeReader`] trait so
//! tests run against synthetic fixtures without the graph runtime.

pub mod binding;
pub mod reader;
pub mod stack;

pub use binding::{Binding, BindingError, BindingValue, EvalContext, Source};
pub use reader::{InMemoryReader, NodeReader, NodeSnapshot};
pub use stack::{ContextStack, Frame, StackError};
