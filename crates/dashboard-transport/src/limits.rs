//! Size / DoS limits. Mirrors DASHBOARD.md § "Size & DoS limits".
//!
//! Exceeding any of these is a 413 at the transport layer, not a
//! partial-resolve — the render tree must be produced under bounded
//! resources or refused.

pub const MAX_WIDGETS_PER_PAGE: usize = 200;
pub const MAX_NAV_DEPTH: usize = 16;
pub const MAX_RENDER_TREE_BYTES: usize = 2 * 1024 * 1024; // 2 MB
pub const MAX_BINDING_REF_DEPTH: usize = 8;
pub const MAX_SUBSCRIPTIONS_PER_PAGE: usize = 500;
pub const MAX_PAGE_STATE_BYTES: usize = 64 * 1024; // 64 KB
