//! `agent find <query>` — unified lookup across every scope using the
//! same DSL the Studio palette speaks.
//!
//! The DSL parser here mirrors `ui-core/src/features/search/dsl.ts` —
//! if you add a scope alias there, add it here too.

use agent_client::{AgentClient, SearchParams};
use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::output::{self, OutputFormat};

const SCOPES: &[&str] = &["flows", "nodes", "blocks", "kinds", "links"];

#[derive(Debug, Args)]
pub struct FindCmd {
    /// Search expression. Examples:
    ///
    ///   agent find 'hvac'              # global fuzzy
    ///   agent find 'kind:compute'      # scoped
    ///   agent find '@/station/floor1'  # node-path prefix
    ///   agent find '#flows hvac'       # alt scope form
    pub query: String,

    /// Cap results per scope. Defaults to 20 — plenty for a CLI dump.
    #[arg(long, default_value = "20")]
    pub per_scope: u64,
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &FindCmd) -> Result<()> {
    let parsed = parse_query(&cmd.query);
    let hits = fetch(client, &parsed, cmd.per_scope).await?;
    render(fmt, &hits)
}

// ---------------------------------------------------------------------------
// DSL parser — mirror of `ui-core/src/features/search/dsl.ts`.
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Parsed {
    scope: Option<&'static str>,
    term: String,
    path_prefix: Option<String>,
}

fn parse_query(raw: &str) -> Parsed {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Parsed::default();
    }

    // `#scope term`
    if let Some(rest) = trimmed.strip_prefix('#') {
        let mut it = rest.splitn(2, char::is_whitespace);
        let head = it.next().unwrap_or("");
        let tail = it.next().unwrap_or("").trim();
        if let Some(scope) = resolve_scope(head) {
            return Parsed {
                scope: Some(scope),
                term: tail.to_string(),
                path_prefix: None,
            };
        }
    }

    // `@path [term]`
    if let Some(rest) = trimmed.strip_prefix('@') {
        let mut it = rest.splitn(2, char::is_whitespace);
        let path = it.next().unwrap_or("");
        let tail = it.next().unwrap_or("").trim();
        return Parsed {
            scope: Some("nodes"),
            term: tail.to_string(),
            path_prefix: Some(normalise_path(path)),
        };
    }

    // `scope:term` / `kind:term` (only valid on the first whitespace-split token).
    let (head, tail) = match trimmed.split_once(char::is_whitespace) {
        Some((h, t)) => (h, t.trim().to_string()),
        None => (trimmed, String::new()),
    };
    if let Some((left, right)) = head.split_once(':') {
        if let Some(scope) = resolve_scope(&left.to_lowercase()) {
            let term = match (right.is_empty(), tail.is_empty()) {
                (true, true) => String::new(),
                (true, false) => tail,
                (false, true) => right.to_string(),
                (false, false) => format!("{right} {tail}"),
            };
            return Parsed {
                scope: Some(scope),
                term,
                path_prefix: None,
            };
        }
    }

    Parsed {
        scope: None,
        term: trimmed.to_string(),
        path_prefix: None,
    }
}

fn resolve_scope(raw: &str) -> Option<&'static str> {
    match raw.to_lowercase().as_str() {
        "kind" | "kinds" => Some("kinds"),
        "node" | "nodes" => Some("nodes"),
        "block" | "blocks" => Some("blocks"),
        "link" | "links" => Some("links"),
        "flow" | "flows" => Some("flows"),
        _ => None,
    }
}

fn normalise_path(raw: &str) -> String {
    if raw.is_empty() {
        "/".to_string()
    } else if raw.starts_with('/') {
        raw.to_string()
    } else {
        format!("/{raw}")
    }
}

// ---------------------------------------------------------------------------
// Fetch — parallel scope hits with post-filter.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct Row {
    scope: String,
    label: String,
    subtitle: String,
}

async fn fetch(client: &AgentClient, parsed: &Parsed, per_scope: u64) -> Result<Vec<Row>> {
    let scopes: Vec<&'static str> = match parsed.scope {
        Some(s) => vec![s],
        None => SCOPES.to_vec(),
    };

    let mut out: Vec<Row> = Vec::new();
    for scope in scopes {
        let env = client
            .search()
            .query(SearchParams {
                scope,
                size: Some(per_scope),
                ..Default::default()
            })
            .await?;
        for v in env.hits {
            if let Some(row) = project(scope, &v, parsed) {
                out.push(row);
            }
        }
    }
    Ok(out)
}

/// Project a scope-specific JSON row into the uniform `Row` shape and
/// apply client-side `term` / `pathPrefix` filters. The server already
/// narrowed by scope; the palette DSL's free-text term doesn't compile
/// cleanly into a single RSQL expression (different scopes expose
/// different name-like fields), so we substring-match locally.
fn project(scope: &str, v: &serde_json::Value, parsed: &Parsed) -> Option<Row> {
    let row = match scope {
        "kinds" => Row {
            scope: "kinds".into(),
            label: string_or(v, "display_name").unwrap_or_else(|| string_at(v, "id")),
            subtitle: string_at(v, "id"),
        },
        "nodes" => {
            let path = string_at(v, "path");
            if let Some(prefix) = &parsed.path_prefix {
                if !path.starts_with(prefix) {
                    return None;
                }
            }
            Row {
                scope: "nodes".into(),
                label: path,
                subtitle: string_at(v, "kind"),
            }
        }
        "blocks" => Row {
            scope: "blocks".into(),
            label: string_or(v, "display_name").unwrap_or_else(|| string_at(v, "id")),
            subtitle: format!(
                "{} · {}",
                string_at(v, "id"),
                v.get("lifecycle")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown"),
            ),
        },
        "flows" => Row {
            scope: "flows".into(),
            label: string_at(v, "name"),
            subtitle: format!("flow · {}", string_at(v, "id")),
        },
        "links" => Row {
            scope: "links".into(),
            label: format!(
                "{}.{} → {}.{}",
                nested_string(v, &["source", "path"])
                    .unwrap_or_else(|| nested_string(v, &["source", "node_id"]).unwrap_or_default()),
                nested_string(v, &["source", "slot"]).unwrap_or_default(),
                nested_string(v, &["target", "path"])
                    .unwrap_or_else(|| nested_string(v, &["target", "node_id"]).unwrap_or_default()),
                nested_string(v, &["target", "slot"]).unwrap_or_default(),
            ),
            subtitle: string_or(v, "scope_path").unwrap_or_default(),
        },
        _ => return None,
    };

    let term = parsed.term.to_lowercase();
    if !term.is_empty()
        && !row.label.to_lowercase().contains(&term)
        && !row.subtitle.to_lowercase().contains(&term)
    {
        return None;
    }
    Some(row)
}

fn string_at(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .unwrap_or_default()
}

fn string_or(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn nested_string(v: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = v;
    for p in path {
        current = current.get(*p)?;
    }
    current.as_str().map(str::to_string)
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

fn render(fmt: OutputFormat, rows: &[Row]) -> Result<()> {
    output::ok_table(fmt, &["SCOPE", "LABEL", "SUBTITLE"], rows, |r: &Row| {
        vec![r.scope.clone(), r.label.clone(), r.subtitle.clone()]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty() {
        let p = parse_query("");
        assert!(p.scope.is_none());
        assert!(p.term.is_empty());
    }

    #[test]
    fn parses_scope_colon_form() {
        let p = parse_query("kind:compute");
        assert_eq!(p.scope, Some("kinds"));
        assert_eq!(p.term, "compute");
    }

    #[test]
    fn parses_scope_colon_with_trailing_term() {
        let p = parse_query("flow:hvac east");
        assert_eq!(p.scope, Some("flows"));
        assert_eq!(p.term, "hvac east");
    }

    #[test]
    fn parses_hash_form() {
        let p = parse_query("#nodes temp");
        assert_eq!(p.scope, Some("nodes"));
        assert_eq!(p.term, "temp");
    }

    #[test]
    fn parses_at_path() {
        let p = parse_query("@/hvac/ahu");
        assert_eq!(p.scope, Some("nodes"));
        assert_eq!(p.path_prefix.as_deref(), Some("/hvac/ahu"));
        assert!(p.term.is_empty());
    }

    #[test]
    fn parses_at_path_with_term() {
        let p = parse_query("@hvac temp");
        assert_eq!(p.scope, Some("nodes"));
        assert_eq!(p.path_prefix.as_deref(), Some("/hvac"));
        assert_eq!(p.term, "temp");
    }

    #[test]
    fn unknown_prefix_falls_through_as_term() {
        let p = parse_query("foo:bar");
        assert!(p.scope.is_none());
        assert_eq!(p.term, "foo:bar");
    }

    #[test]
    fn bare_term_is_global() {
        let p = parse_query("hvac");
        assert!(p.scope.is_none());
        assert_eq!(p.term, "hvac");
    }
}
