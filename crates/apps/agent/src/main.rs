//! The agent binary.
//!
//! One source tree, three roles (edge / cloud / standalone) selected at
//! runtime via `--role` and gated at compile time via Cargo features.
//!
//! The binary serves two purposes:
//!   1. `agent run` — starts the long-lived daemon (engine + REST surface)
//!   2. `agent <command>` — CLI client that talks to a running agent via HTTP
//!
//! Config precedence per `docs/design/OVERVIEW.md`:
//! `cli > env > file > defaults`. The `config` crate owns the layer
//! types; this binary only wires them.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use auth::{DevNullProvider, StaticTokenProvider};
use clap::{Parser, Subcommand};
use config::{
    default_db_path, default_plugins_dir, from_env, from_file, AgentConfig, AgentConfigOverlay,
    AuthConfig, DatabaseOverlay, Defaults, FleetConfig, FleetOverlay, LogOverlay, PluginsOverlay,
    Role, ZenohFleetOverlay,
};
use data_repos::PreferencesService;
use data_sqlite::SqliteFlowRevisionRepo;
use data_sqlite::SqliteGraphRepo;
use data_sqlite::SqlitePreferencesRepo;
use domain_flows::FlowService;
use engine::{kinds as engine_kinds, Engine};
use extensions_host::{HostPolicy, PluginHost, PluginRegistry};
use graph::{seed, GraphStore, KindRegistry};
use spi::{AuthProvider, FleetTransport, KindId, TenantId};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{info, warn};
use transport_cli::{CliCommand, GlobalOpts};
use transport_fleet_zenoh::{ZenohConfig, ZenohTransport};
use transport_rest::AppState;

#[derive(Debug, Parser)]
#[command(
    name = "agent",
    version,
    about = "Flow-based integration platform agent"
)]
struct Cli {
    #[command(flatten)]
    global: GlobalOpts,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the agent daemon.
    Run {
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

        /// Plugins directory (scanned at startup). Role default applies
        /// if unset; pass `.` for in-tree dev.
        #[arg(long, value_name = "PATH")]
        plugins_dir: Option<PathBuf>,

        /// HTTP bind address for the REST + manual-test UI.
        #[arg(long, value_name = "ADDR", default_value = "127.0.0.1:8080")]
        http: SocketAddr,

        /// Enable the embedded Zenoh fleet transport. Without this flag
        /// the agent runs with `NullTransport` (standalone / fleet:null).
        #[arg(long)]
        fleet_zenoh: bool,

        /// Zenoh listen endpoints (e.g. `tcp/0.0.0.0:7447`). Repeatable.
        #[arg(long = "fleet-zenoh-listen", value_name = "ENDPOINT")]
        fleet_zenoh_listen: Vec<String>,

        /// Zenoh connect endpoints (peers / routers to dial outbound).
        #[arg(long = "fleet-zenoh-connect", value_name = "ENDPOINT")]
        fleet_zenoh_connect: Vec<String>,

        /// Tenant id for the fleet subject prefix. Defaults to `default`.
        #[arg(long, value_name = "TENANT", default_value = "default")]
        fleet_tenant: String,

        /// Agent id for the fleet subject prefix. Defaults to the
        /// machine hostname, falling back to `local`.
        #[arg(long, value_name = "ID")]
        fleet_agent_id: Option<String>,
    },

    /// CLI commands that talk to a running agent.
    #[command(flatten)]
    Cli(CliCommand),
}

fn parse_role(s: &str) -> Result<Role, String> {
    s.parse().map_err(|e: config::UnknownRole| e.to_string())
}

/// Intercept `--help-json` before clap validates required positional args.
///
/// When `--help-json` is present, clap would fail on subcommands with
/// required positional arguments (e.g. `nodes create`) before our code
/// runs. Instead we scan raw args, find the command path, print the
/// machine-readable metadata, and exit 0 — all without invoking clap's
/// full parser.
///
/// Arg scanning rule: collect non-flag tokens (not starting with `-`)
/// after the binary name and before the first flag or `--help-json`.
/// `agent nodes create --help-json` → cmd = `"nodes create"`.
fn try_help_json() {
    let raw: Vec<String> = std::env::args().collect();
    if !raw.iter().any(|a| a == "--help-json") {
        return;
    }
    let cmd_parts: Vec<&str> = raw[1..]
        .iter()
        .take_while(|a| !a.starts_with('-'))
        .map(String::as_str)
        .collect();
    let cmd_name = cmd_parts.join(" ");
    match transport_cli::meta::find_command(&cmd_name) {
        Some(info) => {
            let help = info.to_help_json();
            let json =
                serde_json::to_string_pretty(&help).expect("help metadata always serialises");
            println!("{json}"); // NO_PRINTLN_LINT:allow
            std::process::exit(0);
        }
        None => {
            eprintln!("--help-json: unknown command `{cmd_name}`"); // NO_PRINTLN_LINT:allow
            std::process::exit(3);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    try_help_json(); // must run before Cli::parse() — see fn doc

    let cli = Cli::parse();

    match &cli.command {
        Command::Run {
            role,
            config,
            db,
            log,
            plugins_dir,
            http,
            fleet_zenoh,
            fleet_zenoh_listen,
            fleet_zenoh_connect,
            fleet_tenant,
            fleet_agent_id,
        } => {
            let fleet_overlay = if *fleet_zenoh {
                Some(FleetOverlay::Zenoh(ZenohFleetOverlay {
                    listen: (!fleet_zenoh_listen.is_empty()).then(|| fleet_zenoh_listen.clone()),
                    connect: (!fleet_zenoh_connect.is_empty()).then(|| fleet_zenoh_connect.clone()),
                    tenant: Some(fleet_tenant.clone()),
                    agent_id: fleet_agent_id.clone(),
                }))
            } else {
                if !fleet_zenoh_listen.is_empty() || !fleet_zenoh_connect.is_empty() {
                    warn!(
                        "--fleet-zenoh-listen/--fleet-zenoh-connect supplied but --fleet-zenoh is off; ignoring"
                    );
                }
                None
            };
            run_daemon(
                *role,
                config.clone(),
                db.clone(),
                log.clone(),
                plugins_dir.clone(),
                *http,
                fleet_overlay,
            )
            .await
        }
        Command::Cli(cmd) => {
            transport_cli::execute(&cli.global, cmd).await;
            Ok(())
        }
    }
}

async fn run_daemon(
    role: Option<Role>,
    config_path: Option<PathBuf>,
    db: Option<PathBuf>,
    log: Option<String>,
    plugins_dir: Option<PathBuf>,
    http: SocketAddr,
    fleet: Option<FleetOverlay>,
) -> Result<()> {
    let cfg = resolve_config(role, config_path, db, log, plugins_dir, fleet)?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&cfg.log.filter)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!(
        role = %cfg.role,
        db = ?cfg.database.path.as_deref(),
        plugins_dir = %cfg.plugins.dir.display(),
        flow_schema = spi::FLOW_SCHEMA_VERSION,
        node_schema = spi::NODE_SCHEMA_VERSION,
        "agent starting",
    );

    let (engine, graph, events, ring, plugins) = bootstrap(&cfg).await?;
    engine.start().await?;
    info!(state = ?engine.state(), "engine running");

    // Start the process-plugin host. Sockets go under
    // <plugins_dir>/.sockets/ so they share the plugins dir's
    // writability guarantees without colliding with plugin contents.
    let socket_dir = cfg.plugins.dir.join(".sockets");
    let plugin_host = match PluginHost::start(plugins.clone(), socket_dir, HostPolicy::default())
        .await
    {
        Ok(h) => {
            info!("process-plugin host started");
            Some(h)
        }
        Err(e) => {
            tracing::warn!(error = %e, "process-plugin host unavailable — process plugins will not run");
            None
        }
    };

    let mut app_state = AppState::new_with_ring(
        graph.clone(),
        engine.behaviors().clone(),
        events,
        ring,
        plugins,
    );
    if let Some(ref h) = plugin_host {
        app_state = app_state.with_plugin_host(h.clone());
    }

    // Resolve the identity provider. Absent / `dev_null` → default
    // `DevNullProvider` stays; `static_token` → swap in a
    // `StaticTokenProvider` populated from config.
    let auth_provider: Arc<dyn AuthProvider> = match &cfg.auth {
        AuthConfig::DevNull => {
            info!(provider = "dev_null", "auth provider resolved");
            Arc::new(DevNullProvider::new())
        }
        AuthConfig::StaticToken { tokens } => {
            info!(
                provider = "static_token",
                token_count = tokens.len(),
                "auth provider resolved",
            );
            Arc::new(StaticTokenProvider::new(tokens.iter().cloned()))
        }
    };
    app_state = app_state.with_auth_provider(auth_provider);

    // Wire flow service when a DB path is configured.
    if let Some(ref path) = cfg.database.path {
        match SqliteFlowRevisionRepo::open_file(path) {
            Ok(repo) => {
                let svc = FlowService::new(std::sync::Arc::new(repo));
                app_state = app_state.with_flow_service(svc);
                info!("flow revision service attached");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to open flow revision repo — undo/redo unavailable");
            }
        }

        // Wire preferences service from the same DB path.
        match SqlitePreferencesRepo::open_file(path) {
            Ok(repo) => {
                let svc = PreferencesService::new(std::sync::Arc::new(repo));
                app_state = app_state.with_prefs_service(svc);
                info!("preferences service attached");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to open preferences repo — preferences endpoints unavailable");
            }
        }
    }

    // Optional embedded fleet transport. `NullTransport` stays in place
    // when `fleet: null` (the default) is resolved.
    let _fleet_servers = match &cfg.fleet {
        FleetConfig::Null => {
            info!("fleet: null — running without cloud transport");
            None
        }
        FleetConfig::Zenoh {
            listen,
            connect,
            tenant,
            agent_id,
        } => {
            let tenant_id = TenantId::new(tenant);
            info!(
                tenant = %tenant_id,
                agent_id,
                ?listen,
                ?connect,
                "opening zenoh fleet transport",
            );
            let zcfg = ZenohConfig {
                listen: listen.clone(),
                connect: connect.clone(),
            };
            let transport: Arc<dyn FleetTransport> = Arc::new(
                ZenohTransport::connect(zcfg)
                    .await
                    .context("zenoh connect")?,
            );
            app_state = app_state.with_fleet(transport);
            let servers = transport_rest::fleet::mount(app_state.clone(), &tenant_id, agent_id)
                .await
                .context("mounting fleet handlers")?;
            info!(
                handlers = servers.len(),
                "fleet handlers mounted on `fleet.{tenant_id}.{agent_id}.api.v1.*`"
            );
            Some(servers)
        }
    };

    let dashboard_reader: Arc<dyn dashboard_runtime::NodeReader + Send + Sync> =
        Arc::new(dashboard_transport::GraphReader::new(graph.clone()));
    let router = transport_rest::router(app_state).merge(dashboard_transport::router(
        dashboard_transport::DashboardState::new(dashboard_reader),
    ));
    let listener = tokio::net::TcpListener::bind(http).await?;
    info!(addr = %http, "http surface listening");
    let server = tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router).await {
            tracing::error!(error = %err, "http server exited");
        }
    });

    wait_for_termination().await;
    info!("termination signal received \u{2014} beginning graceful shutdown");

    server.abort();
    if let Some(host) = plugin_host {
        info!("shutting down process plugins");
        host.shutdown().await;
    }
    engine.shutdown().await?;
    info!(state = ?engine.state(), "agent exited cleanly");
    Ok(())
}

fn resolve_config(
    role: Option<Role>,
    config_path: Option<PathBuf>,
    db: Option<PathBuf>,
    log: Option<String>,
    plugins_dir: Option<PathBuf>,
    fleet: Option<FleetOverlay>,
) -> Result<AgentConfig> {
    let cli_layer = AgentConfigOverlay {
        role,
        database: db.map(|p| DatabaseOverlay { path: Some(p) }),
        log: log.map(|f| LogOverlay { filter: Some(f) }),
        plugins: plugins_dir.map(|d| PluginsOverlay { dir: Some(d) }),
        fleet,
        auth: None,
    };
    let env_layer = from_env().context("reading env config")?;
    let file_layer = match config_path {
        Some(ref path) => {
            from_file(path).with_context(|| format!("loading config file {}", path.display()))?
        }
        None => AgentConfigOverlay::default(),
    };
    Ok(cli_layer
        .merge_over(env_layer)
        .merge_over(file_layer)
        .resolve(Defaults {
            db_path: &default_db_path,
            plugins_dir: &default_plugins_dir,
        }))
}

async fn bootstrap(
    cfg: &AgentConfig,
) -> Result<(
    Arc<Engine>,
    Arc<GraphStore>,
    tokio::sync::broadcast::Sender<transport_rest::SequencedEvent>,
    transport_rest::EventRing,
    PluginRegistry,
)> {
    let (sink, events_rx, bcast, ring) = transport_rest::agent_sink();

    let kinds = KindRegistry::new();
    seed::register_builtins(&kinds);
    engine_kinds::register(&kinds);
    domain_compute::register_kinds(&kinds);
    domain_logic::register_kinds(&kinds);
    domain_extensions::register_kinds(&kinds);
    domain_fleet::register_kinds(&kinds);
    dashboard_nodes::register_kinds(&kinds);

    // Scan plugins *before* opening the graph so plugin-contributed
    // kinds are in the registry the graph later validates against.
    let host_caps = transport_rest::host_capabilities().capabilities;
    let plugins = PluginRegistry::scan(&cfg.plugins.dir, &host_caps, &kinds)
        .context("scanning plugins dir")?;
    for p in plugins.list() {
        info!(
            id = %p.id,
            version = %p.version,
            lifecycle = ?p.lifecycle,
            has_ui = p.has_ui,
            kinds = ?p.kinds,
            errors = ?p.load_errors,
            "plugin discovered"
        );
    }

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
        graph.create_root(KindId::new("sys.core.station"))?;
    }
    seed_plugin_nodes(&graph, &plugins)?;
    if !cfg.role.runs_engine() {
        tracing::debug!(role = %cfg.role, "role does not run the engine; keeping graph idle");
    }
    let engine = Engine::new(graph.clone(), events_rx);
    engine
        .behaviors()
        .register(
            <domain_compute::Count as extensions_sdk::NodeKind>::kind_id(),
            domain_compute::behavior(),
        )
        .context("registering count behaviour")?;
    engine
        .behaviors()
        .register(
            <domain_logic::Trigger as extensions_sdk::NodeKind>::kind_id(),
            domain_logic::behavior(),
        )
        .context("registering trigger behaviour")?;
    engine
        .behaviors()
        .register(
            <domain_logic::Heartbeat as extensions_sdk::NodeKind>::kind_id(),
            domain_logic::heartbeat_behavior(),
        )
        .context("registering heartbeat behaviour")?;

    Ok((engine, graph, bcast, ring, plugins))
}

/// Reflect every loaded plugin as an `sys.agent.plugin` node under
/// `/agent/plugins/`. Per `docs/design/EVERYTHING-AS-NODE.md` § "The
/// agent itself is a node too" — plugin state lives in the graph, not
/// in a parallel registry, so Studio subscribes via the same event
/// bus as every other slot change.
fn seed_plugin_nodes(graph: &GraphStore, plugins: &PluginRegistry) -> Result<()> {
    use std::str::FromStr;

    let folder = KindId::new("sys.core.folder");
    let plugin_kind = KindId::new("sys.agent.plugin");
    let root = spi::NodePath::root();

    let agent_path = spi::NodePath::from_str("/agent").expect("literal path");
    if graph.get(&agent_path).is_none() {
        graph
            .create_child(&root, folder.clone(), "agent")
            .context("creating /agent folder")?;
    }
    let plugins_path = spi::NodePath::from_str("/agent/plugins").expect("literal path");
    if graph.get(&plugins_path).is_none() {
        graph
            .create_child(&agent_path, folder, "plugins")
            .context("creating /agent/plugins folder")?;
    }

    for p in plugins.list() {
        let node_path = plugins_path.child(p.id.as_str());
        if graph.get(&node_path).is_none() {
            graph
                .create_child(&plugins_path, plugin_kind.clone(), p.id.as_str())
                .with_context(|| format!("creating plugin node {}", p.id))?;
        }
        // Reflect current state onto the node's slots. Each write goes
        // through the same sink as every other mutation, so SSE
        // subscribers see plugin lifecycle changes natively.
        graph.write_slot(
            &node_path,
            "lifecycle",
            serde_json::to_value(p.lifecycle).expect("lifecycle serialises"),
        )?;
        graph.write_slot(
            &node_path,
            "version",
            serde_json::Value::String(p.version.clone()),
        )?;
        let err_value = if p.load_errors.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(p.load_errors.join("; "))
        };
        graph.write_slot(&node_path, "last_error", err_value)?;
        graph.write_slot(
            &node_path,
            "enabled",
            serde_json::Value::Bool(!matches!(
                p.lifecycle,
                extensions_host::PluginLifecycle::Disabled
                    | extensions_host::PluginLifecycle::Failed
            )),
        )?;
    }
    Ok(())
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
