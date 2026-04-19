//! Size / DoS limits. Mirrors SDUI.md § "Size & DoS limits".
//!
//! Exceeding any of these is a 413 at the transport layer, not a
//! partial-resolve — the render tree must be produced under bounded
//! resources or refused. The `what` field on `LimitExceeded` is a
//! stable identifier the client can branch on.

pub const MAX_NAV_DEPTH: usize = 16;
pub const MAX_RENDER_TREE_BYTES: usize = 2 * 1024 * 1024; // 2 MB
pub const MAX_SUBSCRIPTIONS_PER_PAGE: usize = 500;
pub const MAX_PAGE_STATE_BYTES: usize = 64 * 1024; // 64 KB
pub const MAX_TREE_NODES: usize = 2_000;
pub const MAX_TREE_DEPTH: usize = 32;
pub const MAX_COMPONENT_TYPES: usize = 60;
