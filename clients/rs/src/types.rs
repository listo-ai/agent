//! Wire-shape DTOs matching the REST API JSON.
//!
//! These are the client's own types — they don't depend on any server
//! crate. They match the JSON the agent sends/receives, analogous to
//! the TS client's Zod schemas.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---- nodes ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSnapshot {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub parent_id: Option<String>,
    pub lifecycle: String,
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    pub name: String,
    pub value: JsonValue,
    pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedNode {
    pub id: String,
    pub path: String,
}

// ---- slots ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteSlotResponse {
    pub generation: u64,
}

// ---- links ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    pub source: LinkEndpoint,
    pub target: LinkEndpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkEndpoint {
    pub node_id: String,
    pub path: Option<String>,
    pub slot: String,
}

/// Reference used when creating a link — either by node path or node ID.
#[derive(Debug, Clone, Serialize)]
pub struct LinkEndpointRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub slot: String,
}

impl LinkEndpointRef {
    pub fn by_path(path: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            path: Some(path.into()),
            node_id: None,
            slot: slot.into(),
        }
    }

    pub fn by_id(node_id: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            path: None,
            node_id: Some(node_id.into()),
            slot: slot.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CreatedLink {
    pub id: String,
}

// ---- lifecycle ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleResponse {
    pub path: String,
    pub to: String,
}

// ---- seed -----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedResult {
    pub folder: String,
    pub nodes: Vec<SeededNode>,
    pub links: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeededNode {
    pub path: String,
    pub kind: String,
}

// ---- capabilities ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub platform: PlatformInfo,
    pub api: ApiInfo,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub version: String,
    pub flow_schema: u32,
    pub node_schema: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiInfo {
    pub rest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removal_planned: Option<String>,
}

// ---- events ---------------------------------------------------------------

/// Graph event from the SSE stream. Internally tagged by `"event"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GraphEvent {
    NodeCreated {
        id: String,
        kind: String,
        path: String,
    },
    NodeRemoved {
        id: String,
        kind: String,
        path: String,
    },
    NodeRenamed {
        id: String,
        old_path: String,
        new_path: String,
    },
    SlotChanged {
        id: String,
        path: String,
        slot: String,
        value: JsonValue,
        generation: u64,
    },
    LifecycleTransition {
        id: String,
        path: String,
        from: String,
        to: String,
    },
    LinkAdded(Link),
    LinkRemoved {
        id: String,
        source: LinkEventEndpoint,
        target: LinkEventEndpoint,
    },
    LinkBroken {
        id: String,
        broken_end: LinkEventEndpoint,
        surviving_end: LinkEventEndpoint,
    },
}

/// Endpoint in a link event — uses `node` (not `node_id`) to match
/// the server's `SlotRef` serde shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkEventEndpoint {
    pub node: String,
    pub slot: String,
}
