//! `agent prefs …` — user + organisation preferences.
//!
//! Thin wrapper over `agent_client::Preferences`. The patch builder is
//! a ~20-line parser over `--set key=value` / `--clear key` flags;
//! nothing here is domain logic.

use agent_client::{AgentClient, PreferencesPatch};
use anyhow::Result;
use clap::{Args, Subcommand};

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum PrefsCmd {
    /// Resolved preferences (`user ?? org ?? system_default`).
    Get(GetArgs),
    /// Patch the user-per-org layer.
    Set(SetArgs),
    /// Read the org-layer row (admin only).
    OrgGet(OrgGetArgs),
    /// Patch the org-layer row (admin only).
    OrgSet(OrgSetArgs),
}

#[derive(Debug, Args)]
pub struct GetArgs {
    /// Scope to this org id. Defaults to the caller's active tenant.
    #[arg(long)]
    pub org: Option<String>,
}

#[derive(Debug, Args)]
pub struct SetArgs {
    #[arg(long)]
    pub org: Option<String>,
    /// `key=value` assignment — repeat for multiple fields.
    #[arg(long = "set", value_name = "KEY=VALUE")]
    pub sets: Vec<String>,
    /// Revert `key` to inherit-from-org. Repeatable.
    #[arg(long = "clear", value_name = "KEY")]
    pub clears: Vec<String>,
}

#[derive(Debug, Args)]
pub struct OrgGetArgs {
    pub org: String,
}

#[derive(Debug, Args)]
pub struct OrgSetArgs {
    pub org: String,
    #[arg(long = "set", value_name = "KEY=VALUE")]
    pub sets: Vec<String>,
    #[arg(long = "clear", value_name = "KEY")]
    pub clears: Vec<String>,
}

impl PrefsCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Get(_) => "prefs get",
            Self::Set(_) => "prefs set",
            Self::OrgGet(_) => "prefs org-get",
            Self::OrgSet(_) => "prefs org-set",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &PrefsCmd) -> Result<()> {
    match cmd {
        PrefsCmd::Get(args) => {
            let resolved = client.preferences().get_mine(args.org.as_deref()).await?;
            output::ok(fmt, &resolved)?;
        }
        PrefsCmd::Set(args) => {
            let patch = build_patch(&args.sets, &args.clears)?;
            let resolved = client
                .preferences()
                .patch_mine(args.org.as_deref(), &patch)
                .await?;
            output::ok(fmt, &resolved)?;
        }
        PrefsCmd::OrgGet(args) => {
            let org = client.preferences().get_org(&args.org).await?;
            output::ok(fmt, &org)?;
        }
        PrefsCmd::OrgSet(args) => {
            let patch = build_patch(&args.sets, &args.clears)?;
            let org = client.preferences().patch_org(&args.org, &patch).await?;
            output::ok(fmt, &org)?;
        }
    }
    Ok(())
}

/// Every field name the server accepts. Kept in one place so a typo
/// fails with a named list of valid options instead of a silent
/// server-side reject.
const KNOWN_FIELDS: &[&str] = &[
    "timezone",
    "locale",
    "language",
    "unit_system",
    "temperature_unit",
    "pressure_unit",
    "date_format",
    "time_format",
    "week_start",
    "number_format",
    "currency",
    "theme",
];

fn build_patch(sets: &[String], clears: &[String]) -> Result<PreferencesPatch> {
    let mut p = PreferencesPatch::default();
    for raw in sets {
        let (k, v) = raw
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--set expects `key=value`, got `{raw}`"))?;
        ensure_known(k)?;
        p = p.set(k, v);
    }
    for k in clears {
        ensure_known(k)?;
        p = p.clear(k);
    }
    Ok(p)
}

fn ensure_known(field: &str) -> Result<()> {
    if KNOWN_FIELDS.contains(&field) {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "unknown preference field `{field}`; expected one of: {}",
            KNOWN_FIELDS.join(", ")
        ))
    }
}
