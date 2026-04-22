//! `agent backup` subcommands — snapshot export and import.
//!
//! Phase 1: local-path-only (`--to /path/...`). URL schemes
//! (`s3://`, `local://`) arrive in Phase 3 behind feature gates.
//! See BACKUP.md § 6.5.

use anyhow::{bail, Context, Result};
use clap::Subcommand;

use agent_client::AgentClient;

use crate::output::{ok, OutputFormat};

/// `agent backup …`
#[derive(Debug, Subcommand)]
pub enum BackupCmd {
    /// Snapshot operations (disaster recovery).
    #[command(subcommand)]
    Snapshot(SnapshotCmd),
    /// Template export/import — portability-filtered node graph.
    #[command(subcommand)]
    Template(TemplateCmd),
}

impl BackupCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Snapshot(sub) => sub.command_name(),
            Self::Template(sub) => sub.command_name(),
        }
    }
}

/// `agent backup snapshot …`
#[derive(Debug, Subcommand)]
pub enum SnapshotCmd {
    /// Export a full snapshot of the running agent.
    Export {
        /// Destination path for the `.listo-snapshot` file.
        #[arg(long)]
        to: String,
    },
    /// Import (restore) a snapshot onto this agent.
    Import {
        /// Path to the `.listo-snapshot` file.
        path: String,
        /// Downgrade to a template import if device_id doesn't match.
        #[arg(long)]
        as_template: bool,
    },
}

impl SnapshotCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Export { .. } => "backup snapshot export",
            Self::Import { .. } => "backup snapshot import",
        }
    }
}

/// `agent backup template …`
#[derive(Debug, Subcommand)]
pub enum TemplateCmd {
    /// Export a portability-filtered template from the running agent.
    Export {
        /// Destination path for the `.listo-template` file.
        #[arg(long)]
        to: String,
    },
    /// Plan (and preview) a template import onto this agent.
    Import {
        /// Path to the `.listo-template` file.
        path: String,
        /// Conflict resolution strategy.
        #[arg(long, default_value = "merge")]
        strategy: String,
    },
}

impl TemplateCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Export { .. } => "backup template export",
            Self::Import { .. } => "backup template import",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &BackupCmd) -> Result<()> {
    match cmd {
        BackupCmd::Snapshot(sub) => run_snapshot(client, fmt, sub).await,
        BackupCmd::Template(sub) => run_template(client, fmt, sub).await,
    }
}

async fn run_snapshot(client: &AgentClient, fmt: OutputFormat, cmd: &SnapshotCmd) -> Result<()> {
    match cmd {
        SnapshotCmd::Export { to } => {
            if to.contains("://") {
                bail!(
                    "URL schemes are not supported in this build. \
                     Use a local path (e.g. /var/backups/snapshot.listo-snapshot)."
                );
            }
            let resp = client
                .backup()
                .export_snapshot(to)
                .await
                .context("POST /api/v1/backup/snapshot/export")?;
            ok(fmt, &resp)
        }
        SnapshotCmd::Import { path, as_template } => {
            let resp = client
                .backup()
                .import_snapshot(path, *as_template)
                .await
                .context("POST /api/v1/backup/snapshot/import")?;
            ok(fmt, &resp)
        }
    }
}

async fn run_template(client: &AgentClient, fmt: OutputFormat, cmd: &TemplateCmd) -> Result<()> {
    match cmd {
        TemplateCmd::Export { to } => {
            if to.contains("://") {
                bail!(
                    "URL schemes are not supported in this build. \
                     Use a local path (e.g. /var/backups/graph.listo-template)."
                );
            }
            let resp = client
                .backup()
                .export_template(to)
                .await
                .context("POST /api/v1/backup/template/export")?;
            ok(fmt, &resp)
        }
        TemplateCmd::Import { path, strategy } => {
            let resp = client
                .backup()
                .plan_template_import(path, strategy)
                .await
                .context("POST /api/v1/backup/template/import")?;
            ok(fmt, &resp)
        }
    }
}