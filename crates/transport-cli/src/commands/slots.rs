//! `agent slots` — slot read/write and history operations.

use agent_client::AgentClient;
use anyhow::Result;
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum SlotsCmd {
    /// Write a slot value.
    Write {
        /// Node path (e.g. `/station/counter`).
        path: String,
        /// Slot name (e.g. `in`).
        slot: String,
        /// Value as JSON (e.g. `42`, `"hello"`, `{"x":1}`).
        value: String,
        /// OCC guard: require the slot's current generation to match.
        #[arg(long)]
        expected_generation: Option<u64>,
    },

    /// Structured history operations (String / Json / Binary slots).
    #[command(subcommand)]
    History(HistoryCmd),

    /// Scalar telemetry history (Bool / Number slots).
    #[command(subcommand)]
    Telemetry(TelemetryCmd),
}

#[derive(Debug, Subcommand)]
pub enum HistoryCmd {
    /// Query structured history records for a slot.
    List {
        /// Node path.
        path: String,
        /// Slot name.
        slot: String,
        /// Start time, Unix ms (default: 0).
        #[arg(long)]
        from: Option<i64>,
        /// End time, Unix ms (default: now).
        #[arg(long)]
        to: Option<i64>,
        /// Maximum number of records to return (default: 1000).
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Record the slot's current value on-demand.
    Record {
        /// Node path.
        path: String,
        /// Slot name.
        slot: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum TelemetryCmd {
    /// Query scalar telemetry records for a slot (Bool / Number).
    List {
        /// Node path.
        path: String,
        /// Slot name.
        slot: String,
        /// Start time, Unix ms (default: 0).
        #[arg(long)]
        from: Option<i64>,
        /// End time, Unix ms (default: now).
        #[arg(long)]
        to: Option<i64>,
        /// Maximum number of records to return (default: 1000).
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Record the slot's current value on-demand (Bool / Number slots).
    Record {
        /// Node path.
        path: String,
        /// Slot name.
        slot: String,
    },
}

impl SlotsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Write { .. } => "slots write",
            Self::History(sub) => sub.command_name(),
            Self::Telemetry(sub) => sub.command_name(),
        }
    }
}

impl HistoryCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List { .. } => "slots history list",
            Self::Record { .. } => "slots history record",
        }
    }
}

impl TelemetryCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::List { .. } => "slots telemetry list",
            Self::Record { .. } => "slots telemetry record",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &SlotsCmd) -> Result<()> {
    match cmd {
        SlotsCmd::Write {
            path,
            slot,
            value,
            expected_generation,
        } => {
            let parsed: serde_json::Value = serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::Value::String(value.clone()));
            let gen = match expected_generation {
                Some(expected) => {
                    client
                        .slots()
                        .write_with_generation(path, slot, &parsed, *expected)
                        .await?
                }
                None => client.slots().write(path, slot, &parsed).await?,
            };
            output::ok_msg(
                fmt,
                &serde_json::json!({ "generation": gen }),
                &format!("generation {gen}"),
            )?;
        }
        SlotsCmd::History(sub) => run_history(client, fmt, sub).await?,
        SlotsCmd::Telemetry(sub) => run_telemetry(client, fmt, sub).await?,
    }
    Ok(())
}

async fn run_history(client: &AgentClient, fmt: OutputFormat, cmd: &HistoryCmd) -> Result<()> {
    match cmd {
        HistoryCmd::List {
            path,
            slot,
            from,
            to,
            limit,
        } => {
            let resp = client
                .slots()
                .history_range(path, slot, *from, *to, *limit)
                .await?;
            let count = resp.data.len();
            output::ok_msg(
                fmt,
                &serde_json::to_value(&resp).unwrap_or_default(),
                &format!("{count} record(s)"),
            )?;
        }
        HistoryCmd::Record { path, slot } => {
            let resp = client.slots().record(path, slot).await?;
            output::ok_msg(
                fmt,
                &serde_json::json!({ "recorded": resp.recorded, "kind": resp.kind }),
                &format!("recorded ({})", resp.kind),
            )?;
        }
    }
    Ok(())
}

async fn run_telemetry(client: &AgentClient, fmt: OutputFormat, cmd: &TelemetryCmd) -> Result<()> {
    match cmd {
        TelemetryCmd::List {
            path,
            slot,
            from,
            to,
            limit,
        } => {
            let resp = client
                .slots()
                .telemetry_range(path, slot, *from, *to, *limit)
                .await?;
            let count = resp.data.len();
            output::ok_msg(
                fmt,
                &serde_json::to_value(&resp).unwrap_or_default(),
                &format!("{count} record(s)"),
            )?;
        }
        TelemetryCmd::Record { path, slot } => {
            let resp = client.slots().record(path, slot).await?;
            output::ok_msg(
                fmt,
                &serde_json::json!({ "recorded": resp.recorded, "kind": resp.kind }),
                &format!("recorded ({})", resp.kind),
            )?;
        }
    }
    Ok(())
}
