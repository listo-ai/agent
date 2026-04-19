//! Zenoh backend for [`spi::FleetTransport`].
//!
//! Zenoh is a pure-Rust pub/sub + query protocol. We embed it as a
//! library — no separate broker binary — which makes it the right fit
//! for "simple stack" deployments: developer laptops, standalone
//! appliances, single-tenant clouds. See `docs/design/FLEET-TRANSPORT.md`.
//!
//! For multi-tenant SaaS with durable buffering and browser-WS clients,
//! prefer the NATS backend.

mod transport;

pub use transport::{ZenohConfig, ZenohTransport};
