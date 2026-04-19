//! The agent binary.
//!
//! One source tree, three roles (edge / cloud / standalone) selected at
//! runtime via `--role` and gated at compile time via Cargo features.
//!
//! Config precedence per `docs/design/OVERVIEW.md`:
//! `cli > env > file > defaults`. The `config` crate owns the layer
//! types; this binary only wires them.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use config::{
    default_db_path, from_env, from_file, AgentConfig, AgentConfigOverlay, DatabaseOverlay,
    LogOverlay, Role,
};
use data_sqlite::SqliteGraphRepo;
use engine::{kinds as engine_kinds, queue, Engine};
use graph::{seed, GraphStore, KindId, KindRegistry};
use tokio::signal::unix::{signal, SignalKind};
use tracing::info;

/// Agent CLI. Flags take the highest precedence in the config layer
/// stack. Keep the surface narrow \u{2014} every flag needs a corresponding
/// config-file field so operators can pick their preferred source.
#[derive(Debug, Parser)]
#[command(
    name = "agent",
    version,
    about = "Flow-based integration platform agent"
)]
struct Cli {
    /// Deployment role. Overrides `AGENT_ROLE` and the config file.
    #[arg(long, value_parser = parse_role)]
    role: Option<Role>,

    /// Path to a YAML config file.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// SQLite database path; unset keeps the graph in memory.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,

    /// Tracing filter, e.g. `info,engine=debug`.
    #[arg(long, value_name = "DIRECTIVE")]
    log: Option<String>,
}

fn parse_role(s: &str) -> Result<Role, String> {
    s.parse().map_err(|e: config::UnknownRole| e.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = resolve_config(&cli)?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&cfg.log.filter)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    info!(
        role = %cfg.role,
        db = ?cfg.database.path.as_deref(),
        flow_schema = spi::FLOW_SCHEMA_VERSION,
        node_schema = spi::NODE_SCHEMA_VERSION,
        "agent starting",
    );

    let engine = bootstrap(&cfg).await?;
    engine.start().await?;
    info!(state = ?engine.state(), "engine running");

    wait_for_termination().await;
    info!("termination signal received \u{2014} beginning graceful shutdown");

    engine.shutdown().await?;
    info!(state = ?engine.state(), "agent exited cleanly");
    Ok(())
}

fn resolve_config(cli: &Cli) -> Result<AgentConfig> {
    let cli_layer = AgentConfigOverlay {
        role: cli.role,
        database: cli.db.as_ref().map(|p| DatabaseOverlay {
            path: Some(p.clone()),
        }),
        log: cli.log.as_ref().map(|f| LogOverlay {
            filter: Some(f.clone()),
        }),
    };
    let env_layer = from_env().context("reading env config")?;
    let file_layer = match cli.config.as_ref() {
        Some(path) => {
            from_file(path).with_context(|| format!("loading config file {}", path.display()))?
        }
        None => AgentConfigOverlay::default(),
    };
    Ok(cli_layer
        .merge_over(env_layer)
        .merge_over(file_layer)
        .resolve(default_db_path))
}

async fn bootstrap(cfg: &AgentConfig) -> Result<Arc<Engine>> {
    let (sink, events) = queue::channel();
    let kinds = KindRegistry::new();
    seed::register_builtins(&kinds);
    engine_kinds::register(&kinds);

    let graph = match cfg.database.path.as_ref() {
        Some(path) => {
            info!(db = %path.display(), "opening sqlite graph repo");
            let repo = Arc::new(SqliteGraphRepo::open_file(path)?);
            Arc::new(GraphStore::with_repo(kinds, sink, repo)?)
        }
        None => {
            info!("no db path \u{2014} running with in-memory graph");
            Arc::new(GraphStore::new(kinds, sink))
        }
    };
    if graph.is_empty() {
        graph.create_root(KindId::new("acme.core.station"))?;
    }
    if !cfg.role.runs_engine() {
        // Reserved for future roles that don't host an engine
        // (Studio-only client, pure API gateway). All three current
        // roles return true; keep the branch so the seam exists.
        tracing::debug!(role = %cfg.role, "role does not run the engine; keeping graph idle");
    }
    Ok(Engine::new(graph, events))
}

async fn wait_for_termination() {
    let mut term = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(error = %err, "SIGTERM handler unavailable \u{2014} falling back to Ctrl-C only");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = term.recv() => info!("SIGTERM"),
        _ = tokio::signal::ctrl_c() => info!("SIGINT"),
    }
}
