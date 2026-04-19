//! Smoke-test client: issue one fleet request against a running agent.
//!
//! Usage:
//!   cargo run --release -p transport-fleet-zenoh --example fleet_get -- \
//!       tcp/127.0.0.1:17447 acme edge-1 api.v1.nodes.list
//!
//! Prints the reply as JSON.

use std::env;
use std::time::Duration;

use spi::{FleetTransport, Subject, TenantId};
use transport_fleet_zenoh::{ZenohConfig, ZenohTransport};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let connect = args.get(1).cloned().unwrap_or_else(|| "tcp/127.0.0.1:17447".to_string());
    let tenant = args.get(2).cloned().unwrap_or_else(|| "acme".to_string());
    let agent_id = args.get(3).cloned().unwrap_or_else(|| "edge-1".to_string());
    let kind = args.get(4).cloned().unwrap_or_else(|| "api.v1.nodes.list".to_string());

    eprintln!(
        "connecting to {connect}, querying fleet.{tenant}.{agent_id}.{kind}..."
    );

    let t = ZenohTransport::connect(ZenohConfig {
        listen: vec![],
        connect: vec![connect],
    })
    .await?;

    // Give the session a moment to establish.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let subj = Subject::for_agent(&TenantId::new(&tenant), &agent_id)
        .kind(&kind)
        .build();

    let reply = t.request(&subj, vec![], Duration::from_secs(3)).await?;
    let v: serde_json::Value = serde_json::from_slice(&reply)?;
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}
