//! gRPC services generated from [`crates/spi/proto/extension.proto`].
//!
//! The [`proto`] module contains the `tonic`-generated client and
//! server for the `us.spi.extension.v1.Extension` service.
//!
//! Consumers:
//! - `extensions-host` (supervisor) uses [`ExtensionClient`] to talk to
//!   a spawned plugin over a Unix-domain socket.
//! - `extensions-sdk` (with `process` feature) uses [`ExtensionServer`]
//!   so plugin authors never hand-write the service.
//!
//! This crate is intentionally a thin re-export of the generated code
//! plus a `pub use` of `tonic` so callers don't need a direct dep.

pub mod proto {
    tonic::include_proto!("us.spi.extension.v1");
}

pub use proto::{
    extension_client::ExtensionClient,
    extension_server::{Extension, ExtensionServer},
    health_response::Status as HealthStatus,
    DescribeRequest, DescribeResponse, DiscoverEvent, DiscoverRequest, HealthRequest,
    HealthResponse, InvokeRequest, InvokeResponse, KindDeclaration, SlotEvent, SubscribeRequest,
};

pub use prost;
pub use tonic;
