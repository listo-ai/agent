//! Clap command tree and dispatch.

use clap::Subcommand;

use agent_client::AgentClient;
use anyhow::Result;

use crate::output::OutputFormat;

mod ai;
mod auth;
mod backup;
mod blocks;
mod capabilities;
mod config;
mod find;
mod flows;
mod health;
mod kinds;
mod lifecycle;
mod links;
pub mod meta;
mod nodes;
mod preferences;
mod schema;
mod seed;
mod slots;
mod tags;
mod ui;
mod units;
mod users;

/// Global options shared by every CLI subcommand.
#[derive(Debug, Clone, clap::Args)]
pub struct GlobalOpts {
    /// Agent URL.
    #[arg(
        long,
        short = 'u',
        default_value = "http://localhost:8080",
        global = true,
        env = "AGENT_URL"
    )]
    pub url: String,

    /// Bearer token for authenticated agents.
    #[arg(long, global = true, env = "AGENT_TOKEN")]
    pub token: Option<String>,

    /// Output format.
    #[arg(long, short = 'o', default_value = "table", global = true, value_enum)]
    pub output: OutputFormat,

    /// Print machine-readable JSON metadata about this subcommand and exit.
    /// LLM-friendly alternative to --help. See CLI.md § "--help-json".
    #[arg(long, global = true, hide = true)]
    pub help_json: bool,
}

/// CLI subcommands — everything except `run` (which starts the daemon
/// and lives in the agent binary itself).
#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// Check agent health.
    Health,

    /// Show the agent's capability manifest.
    Capabilities,

    /// Node operations.
    #[command(subcommand)]
    Nodes(nodes::NodesCmd),

    /// Slot operations.
    #[command(subcommand)]
    Slots(slots::SlotsCmd),

    /// Config operations.
    #[command(subcommand)]
    Config(config::ConfigCmd),

    /// Link operations.
    #[command(subcommand)]
    Links(links::LinksCmd),

    /// Lifecycle transitions.
    Lifecycle {
        /// Node path (e.g. `/station/floor1/ahu-5`).
        path: String,
        /// Target lifecycle state (e.g. `active`, `disabled`).
        to: String,
    },

    /// Kind registry.
    #[command(subcommand)]
    Kinds(kinds::KindsCmd),

    /// Block operations.
    #[command(subcommand)]
    Plugins(blocks::PluginsCmd),

    /// Auth introspection.
    #[command(subcommand)]
    Auth(auth::AuthCmd),

    /// User + organisation preferences — locale, timezone, units,
    /// formats, theme. See docs/design/USER-PREFERENCES.md.
    #[command(subcommand)]
    Prefs(preferences::PrefsCmd),

    /// Dashboard UI operations.
    #[command(subcommand)]
    Ui(ui::UiCmd),

    /// Flow document operations.
    #[command(subcommand)]
    Flows(flows::FlowsCmd),

    /// Tag operations (config.tags slot).
    #[command(subcommand)]
    Tags(tags::TagsCmd),

    /// User management operations.
    #[command(subcommand)]
    Users(users::UsersCmd),

    /// AI runner operations — list providers, run one-shot prompts.
    #[command(subcommand)]
    Ai(ai::AiCmd),

    /// Unit registry — list all quantities and their units.
    #[command(subcommand)]
    Units(units::UnitsCmd),

    /// Backup & restore — snapshot export/import.
    #[command(subcommand)]
    Backup(backup::BackupCmd),

    /// Unified search across nodes, flows, blocks, kinds and links.
    Find(find::FindCmd),

    /// Seed a preset graph for testing.
    Seed {
        /// Preset name: `count_chain` or `trigger_demo`.
        preset: String,
    },

    /// Dump JSON Schema for a command's inputs, outputs, and error codes.
    Schema {
        /// Show schemas for all commands in a single document.
        #[arg(long)]
        all: bool,
        /// Command path (e.g. `nodes create`). Ignored when --all is set.
        command: Vec<String>,
    },
}

impl CliCommand {
    /// Canonical command name for metadata lookup.
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::Capabilities => "capabilities",
            Self::Nodes(sub) => sub.command_name(),
            Self::Slots(sub) => sub.command_name(),
            Self::Config(sub) => sub.command_name(),
            Self::Links(sub) => sub.command_name(),
            Self::Kinds(sub) => sub.command_name(),
            Self::Plugins(sub) => sub.command_name(),
            Self::Auth(sub) => sub.command_name(),
            Self::Prefs(sub) => sub.command_name(),
            Self::Ui(sub) => sub.command_name(),
            Self::Flows(sub) => sub.command_name(),
            Self::Tags(sub) => sub.command_name(),
            Self::Users(sub) => sub.command_name(),
            Self::Ai(sub) => sub.command_name(),
            Self::Units(sub) => sub.command_name(),
            Self::Backup(sub) => sub.command_name(),
            Self::Find(_) => "find",
            Self::Lifecycle { .. } => "lifecycle",
            Self::Seed { .. } => "seed",
            Self::Schema { .. } => "schema",
        }
    }
}

pub async fn dispatch(client: &AgentClient, global: &GlobalOpts, cmd: &CliCommand) -> Result<()> {
    let fmt = global.output;
    match cmd {
        CliCommand::Health => health::run(client, fmt).await,
        CliCommand::Capabilities => capabilities::run(client, fmt).await,
        CliCommand::Nodes(sub) => nodes::run(client, fmt, sub).await,
        CliCommand::Slots(sub) => slots::run(client, fmt, sub).await,
        CliCommand::Config(sub) => config::run(client, fmt, sub).await,
        CliCommand::Links(sub) => links::run(client, fmt, sub).await,
        CliCommand::Kinds(sub) => kinds::run(client, fmt, sub).await,
        CliCommand::Plugins(sub) => blocks::run(client, fmt, sub).await,
        CliCommand::Auth(sub) => auth::run(client, fmt, sub).await,
        CliCommand::Prefs(sub) => preferences::run(client, fmt, sub).await,
        CliCommand::Ui(sub) => ui::run(client, fmt, sub).await,
        CliCommand::Flows(sub) => flows::run(client, fmt, sub).await,
        CliCommand::Tags(sub) => tags::run(client, fmt, sub).await,
        CliCommand::Users(sub) => users::run(client, fmt, sub).await,
        CliCommand::Ai(sub) => ai::run(client, fmt, sub).await,
        CliCommand::Units(sub) => units::run(client, fmt, sub).await,
        CliCommand::Backup(sub) => backup::run(client, fmt, sub).await,
        CliCommand::Find(cmd) => find::run(client, fmt, cmd).await,
        CliCommand::Lifecycle { path, to } => lifecycle::run(client, fmt, path, to).await,
        CliCommand::Seed { preset } => seed::run(client, fmt, preset).await,
        CliCommand::Schema { all, command } => schema::run(fmt, *all, command),
    }
}
