//! `agent ui nav` / `agent ui resolve` / `agent ui action` / `agent ui table`
//! — dashboard surface.
//!
//! See `docs/design/DASHBOARD.md` for the endpoint semantics and
//! `docs/design/NEW-API.md` for the five-touchpoint rule this module
//! completes.

use agent_client::types::{
    UiActionContext, UiActionRequest, UiComponent, UiComposeRequest, UiResolveRequest,
    UiResolveResponse, UiTableParams,
};
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

    /// Dispatch a named action handler.
    Action {
        /// Fully-qualified handler name (e.g. `com.acme.hello.greet`).
        #[arg(long)]
        handler: String,

        /// Handler arguments as a JSON value.
        #[arg(long, default_value = "null")]
        args: String,

        /// Originating component id.
        #[arg(long)]
        target: Option<String>,

        /// Comma-separated nav node ids forming the context stack.
        #[arg(long, default_value = "")]
        stack: String,

        /// Page-local state as a JSON object.
        #[arg(long, default_value = "{}")]
        page_state: String,

        /// Opaque auth subject identifier.
        #[arg(long)]
        auth_subject: Option<String>,
    },

    /// Dump the `ui_ir::Component` JSON Schema (`GET /api/v1/ui/vocabulary`).
    Vocabulary,

    /// Generate or edit a `ui.page` layout with AI
    /// (`POST /api/v1/ui/compose`).
    ///
    /// Without `--apply`: prints the generated ComponentTree on stdout
    /// (pipe into `agent slots write ... layout @-`).
    ///
    /// With `--apply` + `--page`: reads the page's current layout,
    /// sends it as edit context, and writes the result back with an
    /// OCC-guarded slot write.
    Compose {
        /// Natural-language instruction.
        prompt: String,

        /// Page to use as edit context (id or path). Layout is read
        /// and passed to the model; without this, generation is
        /// cold-start.
        #[arg(long)]
        page: Option<String>,

        /// Free-text hints about surrounding graph state that the
        /// model should reference (node paths, kinds, slots).
        #[arg(long)]
        context: Option<String>,

        /// After generating, write the result to `--page`'s `layout`
        /// slot with an OCC guard. Requires `--page`.
        #[arg(long)]
        apply: bool,

        /// Override the agent's default AI provider for this call.
        /// One of `anthropic`, `openai`, `claude`, `codex`.
        #[arg(long)]
        provider: Option<String>,

        /// Override the model (e.g. `claude-opus-4-5`, `gpt-4o`).
        #[arg(long)]
        model: Option<String>,
    },

    /// Render a node's default SDUI view (`GET /api/v1/ui/render`).
    Render {
        /// Target node id (UUID).
        #[arg(long)]
        target: String,

        /// Optional view id (defaults to highest-priority view on the
        /// target's kind).
        #[arg(long)]
        view: Option<String>,
    },

    /// Fetch a paginated table of nodes matching an RSQL query.
    Table {
        /// Base RSQL query string.
        #[arg(long, default_value = "")]
        query: String,

        /// Additional client-side RSQL filter.
        #[arg(long)]
        filter: Option<String>,

        /// Sort expression (`field asc|desc`).
        #[arg(long)]
        sort: Option<String>,

        /// 1-based page number.
        #[arg(long)]
        page: Option<usize>,

        /// Page size (max 200).
        #[arg(long)]
        size: Option<usize>,

        /// Optional table component id for audit.
        #[arg(long)]
        source_id: Option<String>,
    },
}

impl UiCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Nav { .. } => "ui nav",
            Self::Resolve { .. } => "ui resolve",
            Self::Action { .. } => "ui action",
            Self::Table { .. } => "ui table",
            Self::Render { .. } => "ui render",
            Self::Vocabulary => "ui vocabulary",
            Self::Compose { .. } => "ui compose",
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
                layout: None,
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
        UiCmd::Action {
            handler,
            args,
            target,
            stack,
            page_state,
            auth_subject,
        } => {
            let args_val: serde_json::Value =
                serde_json::from_str(args).map_err(|e| anyhow!("--args is not valid JSON: {e}"))?;
            let page_state_val: serde_json::Value = serde_json::from_str(page_state)
                .map_err(|e| anyhow!("--page-state is not valid JSON: {e}"))?;
            let stack_ids: Vec<String> = stack
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            let req = UiActionRequest {
                handler: handler.clone(),
                args: args_val,
                context: UiActionContext {
                    target: target.clone(),
                    stack: stack_ids,
                    page_state: page_state_val,
                    auth_subject: auth_subject.clone(),
                },
            };
            let resp = client.ui().action(&req).await?;
            output::ok(fmt, &resp)?;
        }
        UiCmd::Vocabulary => {
            let resp = client.ui().vocabulary().await?;
            match fmt {
                OutputFormat::Json => output::ok(fmt, &resp)?,
                OutputFormat::Table => {
                    let rows = vocabulary_rows(&resp.schema);
                    output::ok_table(fmt, &["TYPE", "DESCRIPTION"], &rows, |r| {
                        vec![r.type_name.clone(), r.description.clone()]
                    })?;
                    eprintln!(
                        // NO_PRINTLN_LINT:allow
                        "ir_version={}",
                        resp.ir_version,
                    );
                }
            }
        }
        UiCmd::Compose {
            prompt,
            page,
            context,
            apply,
            provider,
            model,
        } => {
            if *apply && page.is_none() {
                return Err(anyhow!("--apply requires --page"));
            }
            // When editing, read the current layout + generation.
            let (current_layout, current_path, base_gen) = if let Some(p) = page {
                let snap = resolve_page_snapshot(client, p).await?;
                let slot = snap.slots.iter().find(|s| s.name == "layout");
                let layout = slot.and_then(|s| {
                    if s.value.is_null() {
                        None
                    } else {
                        Some(s.value.clone())
                    }
                });
                let gen = slot.map(|s| s.generation).unwrap_or(0);
                (layout, Some(snap.path.clone()), gen)
            } else {
                (None, None, 0)
            };

            let req = UiComposeRequest {
                prompt: prompt.clone(),
                current_layout,
                context_hints: context.clone(),
                provider: provider.clone(),
                model: model.clone(),
            };
            let resp = client.ui().compose(&req).await?;

            if *apply {
                let path = current_path.ok_or_else(|| anyhow!("--apply requires --page"))?;
                client
                    .slots()
                    .write_with_generation(&path, "layout", &resp.layout, base_gen)
                    .await?;
                output::ok_msg(
                    fmt,
                    &serde_json::json!({
                        "applied": true,
                        "path": path,
                        "note": resp.note,
                    }),
                    &format!("applied to {path}"),
                )?;
            } else {
                // Pipe-friendly: JSON mode prints the whole response;
                // table mode prints the layout itself so `| agent slots write` works.
                match fmt {
                    OutputFormat::Json => output::ok(fmt, &resp)?,
                    OutputFormat::Table => {
                        println!(
                            // NO_PRINTLN_LINT:allow
                            "{}",
                            serde_json::to_string_pretty(&resp.layout)?,
                        );
                        if let Some(note) = &resp.note {
                            eprintln!("{note}"); // NO_PRINTLN_LINT:allow
                        }
                    }
                }
            }
        }
        UiCmd::Render { target, view } => {
            let resp = client.ui().render(target, view.as_deref()).await?;
            match (&resp, fmt) {
                (_, OutputFormat::Json) => output::ok(fmt, &resp)?,
                (UiResolveResponse::DryRun { errors }, OutputFormat::Table) => {
                    output::ok_table(fmt, &["LOCATION", "MESSAGE"], errors, |e| {
                        vec![e.location.clone(), e.message.clone()]
                    })?;
                }
                (UiResolveResponse::Ok { render, meta, .. }, OutputFormat::Table) => {
                    let mut rows: Vec<ComponentRow> = Vec::new();
                    flatten_component(&render.root, 0, &mut rows);
                    output::ok_table(fmt, &["DEPTH", "TYPE", "ID"], &rows, |r| {
                        vec![r.depth.to_string(), r.component_type.clone(), r.id.clone()]
                    })?;
                    eprintln!(
                        // NO_PRINTLN_LINT:allow
                        "ir_version={}  cache_key={}  widgets={}",
                        render.ir_version, meta.cache_key, meta.widget_count,
                    );
                }
            }
        }
        UiCmd::Table {
            query,
            filter,
            sort,
            page,
            size,
            source_id,
        } => {
            let params = UiTableParams {
                query: query.clone(),
                filter: filter.clone(),
                sort: sort.clone(),
                page: *page,
                size: *size,
                source_id: source_id.clone(),
            };
            let resp = client.ui().table(&params).await?;
            match fmt {
                OutputFormat::Json => output::ok(fmt, &resp)?,
                OutputFormat::Table => {
                    output::ok_table(fmt, &["ID", "KIND", "PATH", "PARENT"], &resp.data, |r| {
                        vec![
                            r.id.clone(),
                            r.kind.clone(),
                            r.path.clone(),
                            r.parent_id.as_deref().unwrap_or("").to_string(),
                        ]
                    })?;
                    eprintln!(
                        // NO_PRINTLN_LINT:allow
                        "total={}  page={}/{}  size={}",
                        resp.meta.total, resp.meta.page, resp.meta.pages, resp.meta.size,
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

/// Accept either a node path (starts with `/`) or an id (UUID). The
/// compose CLI needs the node's full snapshot to read its layout and
/// generation, so we normalise up-front.
async fn resolve_page_snapshot(
    client: &AgentClient,
    page_ref: &str,
) -> Result<agent_client::types::NodeSnapshot> {
    if page_ref.starts_with('/') {
        Ok(client.nodes().get(page_ref).await?)
    } else {
        let resp = client
            .nodes()
            .list_page(&agent_client::NodeListParams {
                filter: Some(format!("kind==ui.page")),
                sort: None,
                page: None,
                size: Some(500),
            })
            .await?;
        resp.data
            .into_iter()
            .find(|n| n.id == page_ref)
            .ok_or_else(|| anyhow!("no ui.page node with id {page_ref}"))
    }
}

#[derive(serde::Serialize)]
struct VocabRow {
    type_name: String,
    description: String,
}

fn vocabulary_rows(schema: &serde_json::Value) -> Vec<VocabRow> {
    let mut rows = Vec::new();
    let variants = schema.get("oneOf").and_then(|v| v.as_array());
    if let Some(arr) = variants {
        for v in arr {
            let type_name = v
                .pointer("/properties/type/enum/0")
                .and_then(|x| x.as_str())
                .or_else(|| v.pointer("/properties/type/const").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string();
            let description = v
                .get("description")
                .and_then(|x| x.as_str())
                .or_else(|| v.get("title").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string();
            if !type_name.is_empty() {
                rows.push(VocabRow {
                    type_name,
                    description,
                });
            }
        }
    }
    rows
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
        UiComponent::Chart { id, .. } => ("chart", id.as_deref(), &[]),
        UiComponent::Sparkline { id, .. } => ("sparkline", id.as_deref(), &[]),
        UiComponent::Table { id, .. } => ("table", id.as_deref(), &[]),
        UiComponent::RichText { id, .. } => ("rich_text", id.as_deref(), &[]),
        UiComponent::Button { id, .. } => ("button", id.as_deref(), &[]),
        UiComponent::Form { id, .. } => ("form", id.as_deref(), &[]),
        UiComponent::Forbidden { id, .. } => ("forbidden", Some(id), &[]),
        UiComponent::Dangling { id } => ("dangling", Some(id), &[]),
        UiComponent::Custom { id, .. } => ("custom", id.as_deref(), &[]),
        UiComponent::Tree { id, .. } => ("tree", id.as_deref(), &[]),
        UiComponent::Timeline { id, .. } => ("timeline", id.as_deref(), &[]),
        UiComponent::Markdown { id, .. } => ("markdown", id.as_deref(), &[]),
        UiComponent::RefPicker { id, .. } => ("ref_picker", id.as_deref(), &[]),
        UiComponent::Wizard { id, .. } => ("wizard", id.as_deref(), &[]),
        UiComponent::DateRange { id, .. } => ("date_range", id.as_deref(), &[]),
        UiComponent::Select { id, .. } => ("select", id.as_deref(), &[]),
        UiComponent::Kpi { id, .. } => ("kpi", id.as_deref(), &[]),
        UiComponent::Drawer { id, children, .. } => ("drawer", id.as_deref(), children),
    }
}
