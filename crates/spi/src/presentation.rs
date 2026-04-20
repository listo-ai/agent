//! Node presentation update envelope — runtime status, color, icon, message.
//!
//! These are delivered over the message bus (never via a REST mutation)
//! and kept in a separate frontend `PresentationStore`. They do not
//! mutate the node manifest; they are sparse runtime decorations.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Operational status that a node can report at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum NodeStatus {
    /// Node kind does not report status — hide the status indicator.
    #[default]
    None,
    /// Node reports status but no reading has arrived yet (gray dot).
    Unknown,
    Ok,
    Warning,
    Error,
}

/// Sparse runtime update delivered over the message bus.
///
/// Merge semantics are defined in `domain-presentation::apply_patch`:
/// - Fields present in `patch` overwrite the current value.
/// - Fields listed in `clear` are reset to kind-manifest defaults.
/// - Out-of-order envelopes (lower `seq`) are ignored per field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePresentationUpdate {
    pub node_instance_id: Uuid,
    /// Monotonically increasing per node. Used for last-writer-wins ordering.
    pub seq: u64,
    /// ISO 8601 UTC timestamp of the update.
    pub ts: String,
    pub patch: PresentationPatch,
    /// Field names to clear back to manifest defaults.
    #[serde(default)]
    pub clear: Vec<PresentationField>,
}

/// Sparse field bag. Any field that is `None` is not changed by the patch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PresentationPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<NodeStatus>,
    /// CSS color token or hex string (e.g. `"#ff0000"` or `"emerald-500"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Lucide icon name (e.g. `"activity"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Short tooltip text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Enumeration of clearable presentation fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationField {
    Status,
    Color,
    Icon,
    Message,
}
