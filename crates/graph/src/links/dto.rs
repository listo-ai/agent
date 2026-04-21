//! Wire shape for links — mirrors [`crate::Link`] with endpoint paths
//! resolved (source/target nodes looked up so tree UIs render without
//! a second round trip).

use serde::Serialize;

use crate::link::Link;
use crate::store::GraphStore;

#[derive(Debug, Clone, Serialize)]
pub struct LinkDto {
    pub id: String,
    pub source: EndpointDto,
    pub target: EndpointDto,
    /// Materialised parent scope for quick "which flow does this link
    /// belong to?" filters — the `/foo/bar` prefix shared by both
    /// endpoints, or `None` if they diverge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndpointDto {
    pub node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub slot: String,
}

impl LinkDto {
    pub fn from_link(graph: &GraphStore, link: Link) -> Self {
        let source_path = graph
            .get_by_id(link.source.node)
            .map(|n| n.path.to_string());
        let target_path = graph
            .get_by_id(link.target.node)
            .map(|n| n.path.to_string());
        let scope_path = match (&source_path, &target_path) {
            (Some(s), Some(t)) => common_parent(s, t),
            _ => None,
        };
        Self {
            id: link.id.to_string(),
            source: EndpointDto {
                node_id: link.source.node.to_string(),
                path: source_path,
                slot: link.source.slot,
            },
            target: EndpointDto {
                node_id: link.target.node.to_string(),
                path: target_path,
                slot: link.target.slot,
            },
            scope_path,
        }
    }
}

/// Longest shared `/a/b/c` prefix of two node paths. `/` if they share
/// only the root, `None` if the inputs are both `/` (trivial).
fn common_parent(a: &str, b: &str) -> Option<String> {
    let mut out = String::from("/");
    let a_parts = a.split('/').filter(|s| !s.is_empty());
    let b_parts = b.split('/').filter(|s| !s.is_empty());
    for (x, y) in a_parts.zip(b_parts) {
        if x != y {
            break;
        }
        if out.len() > 1 {
            out.push('/');
        }
        out.push_str(x);
    }
    if out == "/" {
        None
    } else {
        Some(out)
    }
}
