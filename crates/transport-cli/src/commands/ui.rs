//! `agent ui nav` / `agent ui resolve` — dashboard surface.
//!
//! See `docs/design/DASHBOARD.md` for the endpoint semantics and
//! `docs/design/NEW-API.md` for the five-touchpoint rule this module
//! completes.

use agent_client::types::{UiComponent, UiResolveRequest, UiResolveResponse};
use agent_client::AgentClient;
use anyhow::{anyhow, Result};
use clap::Subcommand;

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum UiCmd {
    /// Fetch the `ui.nav` subtree rooted at a node id.
    Nav {
        /// Root nav node id (UUID).
        #[arg(long)]
        root: String,
    },

    /// Resolve a `ui.page` into a render tree + subscription metadata.
    Resolve {
        /// Page node id (UUID).
        #[arg(long)]
        page: String,

        /// Ordered list of `ui.nav` node ids forming the context stack.
        /// Comma-separated. Empty = no stack.
        #[arg(long, default_value = "")]
        stack: String,

        /// Page-local state as a JSON object (e.g. `'{"row":3}'`).
        #[arg(long, default_value = "{}")]
        page_state: String,

        /// Validate only — return structured errors instead of a render tree.
        #[arg(long)]
        dry_run: bool,

        /// Opaque auth subject identifier, threaded into the cache key
        /// + audit events. Leave unset for anonymous callers.
        #[arg(long)]
        auth_subject: Option<String>,
    },
}

impl UiCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Nav { .. } => "ui nav",
            Self::Resolve { .. } => "ui resolve",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &UiCmd) -> Result<()> {
    match cmd {
        UiCmd::Nav { root } => {
            let tree = client.ui().nav(root).await?;
            // Table mode collapses the tree into a depth-ordered list;
            // JSON mode is the full nested structure.
            match fmt {
                OutputFormat::Json => output::ok(fmt, &tree)?,
                OutputFormat::Table => {
                    let rows = flatten(&tree, 0);
                    output::ok_table(
                        fmt,
                        &["ID", "TITLE", "PATH", "ALIAS", "DEPTH"],
                        &rows,
                        |r| {
                            vec![
                                r.id.clone(),
                                r.title.clone(),
                                r.path.clone(),
                                r.alias.clone(),
                                r.depth.to_string(),
                            ]
                        },
                    )?;
                }
            }
        }
        UiCmd::Resolve {
            page,
            stack,
            page_state,
            dry_run,
            auth_subject,
        } => {
            let stack_ids: Vec<String> = stack
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            let page_state_val: serde_json::Value = serde_json::from_str(page_state)
                .map_err(|e| anyhow!("--page-state is not valid JSON: {e}"))?;
            let req = UiResolveRequest {
                page_ref: page.clone(),
                stack: stack_ids,
                page_state: page_state_val,
                dry_run: *dry_run,
                auth_subject: auth_subject.clone(),
                user_claims: Default::default(),
            };
            let resp = client.ui().resolve(&req).await?;
            match (&resp, fmt) {
                (_, OutputFormat::Json) => output::ok(fmt, &resp)?,
                (UiResolveResponse::DryRun { errors }, OutputFormat::Table) => {
                    output::ok_table(fmt, &["LOCATION", "MESSAGE"], errors, |e| {
                        vec![e.location.clone(), e.message.clone()]
                    })?;
                }
                (
                    UiResolveResponse::Ok {
                        render,
                        meta,
                        subscriptions: _,
                    },
                    OutputFormat::Table,
                ) => {
                    let mut rows: Vec<ComponentRow> = Vec::new();
                    flatten_component(&render.root, 0, &mut rows);
                    output::ok_table(fmt, &["DEPTH", "TYPE", "ID"], &rows, |r| {
                        vec![r.depth.to_string(), r.component_type.clone(), r.id.clone()]
                    })?;
                    eprintln!(
                        // NO_PRINTLN_LINT:allow
                        "ir_version={}  cache_key={}  widgets={}  forbidden={}  dangling={}",
                        render.ir_version,
                        meta.cache_key,
                        meta.widget_count,
                        meta.forbidden_count,
                        meta.dangling_count,
                    );
                }
            }
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct NavRow {
    id: String,
    title: String,
    path: String,
    alias: String,
    depth: usize,
}

fn flatten(n: &agent_client::types::UiNavNode, depth: usize) -> Vec<NavRow> {
    let mut out = vec![NavRow {
        id: n.id.clone(),
        title: n.title.clone().unwrap_or_default(),
        path: n.path.clone().unwrap_or_default(),
        alias: n.frame_alias.clone().unwrap_or_default(),
        depth,
    }];
    for c in &n.children {
        out.extend(flatten(c, depth + 1));
    }
    out
}

#[derive(serde::Serialize)]
struct ComponentRow {
    depth: usize,
    component_type: String,
    id: String,
}

fn flatten_component(c: &UiComponent, depth: usize, out: &mut Vec<ComponentRow>) {
    let (ctype, id, children) = component_info(c);
    out.push(ComponentRow {
        depth,
        component_type: ctype.into(),
        id: id.unwrap_or("").into(),
    });
    for child in children {
        flatten_component(child, depth + 1, out);
    }
}

fn component_info(c: &UiComponent) -> (&str, Option<&str>, &[UiComponent]) {
    match c {
        UiComponent::Page { id, children, .. } => ("page", Some(id), children),
        UiComponent::Row { id, children, .. } => ("row", id.as_deref(), children),
        UiComponent::Col { id, children, .. } => ("col", id.as_deref(), children),
        UiComponent::Grid { id, children, .. } => ("grid", id.as_deref(), children),
        UiComponent::Tabs { id, .. } => ("tabs", id.as_deref(), &[]),
        UiComponent::Text { id, .. } => ("text", id.as_deref(), &[]),
        UiComponent::Heading { id, .. } => ("heading", id.as_deref(), &[]),
        UiComponent::Badge { id, .. } => ("badge", id.as_deref(), &[]),
        UiComponent::Diff { id, .. } => ("diff", id.as_deref(), &[]),
        UiComponent::Table { id, .. } => ("table", id.as_deref(), &[]),
        UiComponent::RichText { id, .. } => ("rich_text", id.as_deref(), &[]),
        UiComponent::Button { id, .. } => ("button", id.as_deref(), &[]),
        UiComponent::Form { id, .. } => ("form", id.as_deref(), &[]),
        UiComponent::Forbidden { id, .. } => ("forbidden", Some(id), &[]),
        UiComponent::Dangling { id } => ("dangling", Some(id), &[]),
    }
}
