#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Message bus abstraction.
//!
//! One trait, multiple backends. Stage 0 ships the trait and an
//! in-process implementation. The NATS implementation lands in Stage 7
//! without changing callers — that's the point of the trait.

mod bus;
mod error;
mod in_process;

pub use bus::{MessageBus, Subject, Subscription};
pub use error::MessagingError;
pub use in_process::InProcessBus;
