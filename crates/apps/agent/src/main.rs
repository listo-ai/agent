//! The agent binary.
//!
//! One source tree, three roles (edge / cloud / standalone) selected at
//! runtime via `--role` and gated at compile time via Cargo features.
//! Stage 0: starts, logs, exits cleanly. That is the whole acceptance
//! criterion — no engine, no server, just proof the wiring compiles.

use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    info!(
        flow_schema = spi::FLOW_SCHEMA_VERSION,
        node_schema = spi::NODE_SCHEMA_VERSION,
        "agent starting (stage 0: hello-world)",
    );

    // Future stages: role selection, engine start, graceful shutdown on SIGTERM.
    info!("agent exiting cleanly");
    Ok(())
}
