//! `agent units …` — inspect the platform's unit registry.
//!
//! Useful for operators validating a block-author PR ("did the
//! `pressure` quantity actually land?") and for humans building a
//! preference configuration without opening Studio. Thin wrapper
//! over `client.units().get()`; no logic lives here beyond the
//! filter by quantity.

use agent_client::AgentClient;
use anyhow::Result;
use clap::{Args, Subcommand};

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum UnitsCmd {
    /// List every quantity the platform knows about — canonical
    /// unit, allowed alternatives, human labels, compact symbols.
    /// Output format honours `-o json|yaml|table` from the global
    /// flags.
    List,
    /// Show one quantity's details — canonical unit + every allowed
    /// alternative with its symbol and human label. Useful for
    /// building a unit-picker by hand or validating a preference
    /// value against the server's registry.
    Show(ShowArgs),
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Quantity id in snake_case form (e.g. `temperature`,
    /// `flow_rate`). Must match a variant on the server's
    /// `spi::Quantity` enum.
    pub quantity: String,
}

impl UnitsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List => "units list",
            Self::Show(_) => "units show",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &UnitsCmd) -> Result<()> {
    match cmd {
        UnitsCmd::List => {
            let registry = client.units().get().await?;
            output::ok(fmt, &registry)?;
        }
        UnitsCmd::Show(args) => {
            let registry = client.units().get().await?;
            let quantity = registry
                .quantities
                .iter()
                .find(|q| q.id == args.quantity)
                .cloned()
                .ok_or_else(|| unknown_quantity(&args.quantity, &registry))?;
            // Join each allowed unit with its display metadata from
            // the flat table so the caller gets one coherent document.
            let enriched = enrich_quantity(&quantity, &registry.units);
            output::ok(fmt, &enriched)?;
        }
    }
    Ok(())
}

fn unknown_quantity(
    requested: &str,
    registry: &agent_client::UnitRegistryDto,
) -> anyhow::Error {
    let known: Vec<&str> = registry
        .quantities
        .iter()
        .map(|q| q.id.as_str())
        .collect();
    anyhow::anyhow!(
        "unknown quantity `{requested}`; known: {}",
        known.join(", ")
    )
}

/// Merge a `QuantityEntryDto` with its allowed units' metadata
/// (symbol + label) from the flat registry table. Output is a plain
/// JSON shape purpose-built for human reading — not a wire contract,
/// so `serde_json::Value` is appropriate here.
fn enrich_quantity(
    q: &agent_client::QuantityEntryDto,
    units: &[agent_client::UnitEntryDto],
) -> serde_json::Value {
    let allowed: Vec<serde_json::Value> = q
        .allowed
        .iter()
        .map(|unit_id| {
            let entry = units.iter().find(|u| &u.id == unit_id);
            let (symbol, label) = entry
                .map(|e| (e.symbol.as_str(), e.label.as_str()))
                .unwrap_or(("", unit_id.as_str()));
            serde_json::json!({
                "id": unit_id,
                "symbol": symbol,
                "label": label,
                "is_canonical": *unit_id == q.canonical,
            })
        })
        .collect();
    serde_json::json!({
        "id": q.id,
        "label": q.label,
        "canonical": q.canonical,
        "symbol": q.symbol,
        "allowed": allowed,
    })
}

