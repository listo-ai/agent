//! `GET /ui/nav?root=<id>` — returns the `ui.nav` subtree rooted at
//! `root` as a nested JSON tree. Depth is capped by
//! [`crate::limits::MAX_NAV_DEPTH`].

use axum::extract::{Query, State};
use axum::Json;
use dashboard_runtime::{NodeReader, NodeSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::NodeId;

use crate::error::TransportError;
use crate::limits;
use crate::state::DashboardState;

const NAV_KIND: &str = "ui.nav";

#[derive(Debug, Deserialize)]
pub struct NavQuery {
    pub root: NodeId,
}

#[derive(Debug, Serialize)]
pub struct NavNode {
    pub id: NodeId,
    pub title: Option<String>,
    pub path: Option<String>,
    pub icon: Option<String>,
    pub order: Option<i64>,
    pub frame_alias: Option<String>,
    pub frame_ref: Option<JsonValue>,
    pub children: Vec<NavNode>,
}

pub async fn handler(
    State(state): State<DashboardState>,
    Query(q): Query<NavQuery>,
) -> Result<Json<NavNode>, TransportError> {
    let tree = build(&*state.reader, q.root, 0)?;
    Ok(Json(tree))
}

fn build<R: NodeReader + ?Sized>(
    reader: &R,
    id: NodeId,
    depth: usize,
) -> Result<NavNode, TransportError> {
    if depth >= limits::MAX_NAV_DEPTH {
        return Err(TransportError::LimitExceeded {
            what: "nav_depth",
            value: depth,
            max: limits::MAX_NAV_DEPTH,
        });
    }
    let snap = reader
        .get(&id)
        .ok_or_else(|| TransportError::MalformedPage(id, "nav node not found".into()))?;
    if snap.kind.as_str() != NAV_KIND {
        return Err(TransportError::KindMismatch {
            node: id,
            expected: NAV_KIND.into(),
            found: snap.kind.as_str().into(),
        });
    }

    let children = reader
        .children(&id)
        .into_iter()
        .filter_map(|cid| match reader.get(&cid) {
            Some(c) if c.kind.as_str() == NAV_KIND => Some(c.id),
            _ => None,
        })
        .map(|cid| build(reader, cid, depth + 1))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(NavNode {
        id: snap.id,
        title: read_string(&snap, "title"),
        path: read_string(&snap, "path"),
        icon: read_string(&snap, "icon"),
        order: snap.slots.get("order").and_then(|v| v.as_i64()),
        frame_alias: read_string(&snap, "frame_alias"),
        frame_ref: snap.slots.get("frame_ref").cloned(),
        children,
    })
}

fn read_string(snap: &NodeSnapshot, slot: &str) -> Option<String> {
    snap.slots
        .get(slot)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}
