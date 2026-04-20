//! Context stack — the ordered list of frames a nav walk contributes.
//!
//! Per DASHBOARD.md § "The context stack" and § "Alias collision rule":
//!
//! * Each `ui.nav` may contribute one frame `{ alias, nodeRef }`.
//! * Alias lookup resolves to the deepest (innermost) frame with that
//!   alias — standard lexical shadowing.
//! * Index lookup is unambiguous and always available, including
//!   negative indices (`-1` = innermost).
//! * Shadowing along a walk is a save-time **warning**, not an error.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use spi::NodeId;
use thiserror::Error;

use crate::reader::{NodeReader, NodeSnapshot};

const NAV_KIND: &str = "ui.nav";
const FRAME_ALIAS_SLOT: &str = "frame_alias";
const FRAME_REF_SLOT: &str = "frame_ref";

/// A single frame: an alias bound to a node reference. Frames with no
/// alias are possible (indexed-only); frames with no nodeRef are rejected
/// at build time — a frame is a `(alias?, nodeRef)` pair, not just an
/// alias.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// Author-chosen alias. `None` means the frame is addressable only
    /// by index.
    pub alias: Option<String>,
    pub node_ref: NodeId,
}

/// The ordered stack + the set of aliases that were shadowed along the
/// walk (for save-time warnings).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextStack {
    frames: Vec<Frame>,
    shadowed: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StackError {
    #[error("node `{0}` not found in reader")]
    NodeMissing(NodeId),
    #[error("node `{node}` is `{found}`, expected `ui.nav`")]
    NotNavKind { node: NodeId, found: String },
    #[error("nav `{0}` declared frame_alias but no frame_ref")]
    AliasWithoutRef(NodeId),
    #[error("nav `{0}` has malformed frame_ref slot: {1}")]
    MalformedFrameRef(NodeId, String),
    #[error("nav walk exceeds max depth {max}")]
    TooDeep { max: usize },
}

impl ContextStack {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a stack from an explicit ordered list of `ui.nav` node ids.
    /// Callers (tests or the `/ui/nav` handler) pre-resolve the walk;
    /// this function just reads each nav's `frame_alias`/`frame_ref`.
    pub fn build<R: NodeReader + ?Sized>(
        reader: &R,
        walk: &[NodeId],
        max_depth: usize,
    ) -> Result<Self, StackError> {
        if walk.len() > max_depth {
            return Err(StackError::TooDeep { max: max_depth });
        }

        let mut frames: Vec<Frame> = Vec::with_capacity(walk.len());
        let mut shadowed: Vec<String> = Vec::new();

        for id in walk {
            let snap = reader.get(id).ok_or(StackError::NodeMissing(*id))?;
            assert_nav_kind(&snap)?;

            let alias = snap.slots.get(FRAME_ALIAS_SLOT).and_then(|v| match v {
                JsonValue::String(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            });

            let frame_ref = snap.slots.get(FRAME_REF_SLOT);
            let node_ref = match frame_ref {
                Some(JsonValue::Null) | None => {
                    // Nav contributes no frame. Alias alone is
                    // meaningless without a ref — reject to surface
                    // authoring mistakes early.
                    if alias.is_some() {
                        return Err(StackError::AliasWithoutRef(*id));
                    }
                    continue;
                }
                Some(v) => parse_node_ref(v).map_err(|e| StackError::MalformedFrameRef(*id, e))?,
            };

            if let Some(a) = &alias {
                if frames
                    .iter()
                    .any(|f| f.alias.as_deref() == Some(a.as_str()))
                {
                    shadowed.push(a.clone());
                }
            }
            frames.push(Frame { alias, node_ref });
        }

        Ok(Self { frames, shadowed })
    }

    pub fn from_frames(frames: Vec<Frame>) -> Self {
        let mut shadowed = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for f in &frames {
            if let Some(a) = f.alias.as_deref() {
                if seen.contains(&a) {
                    shadowed.push(a.to_string());
                }
                seen.push(a);
            }
        }
        Self { frames, shadowed }
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn shadowed(&self) -> &[String] {
        &self.shadowed
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Alias lookup — innermost wins per DASHBOARD.md § "Alias collision
    /// rule". Returns `None` if no frame carries the alias.
    pub fn by_alias(&self, alias: &str) -> Option<&Frame> {
        self.frames
            .iter()
            .rev()
            .find(|f| f.alias.as_deref() == Some(alias))
    }

    /// Index lookup. Positive indices count from the root; negative
    /// indices count from the innermost frame (`-1` = innermost).
    pub fn by_index(&self, i: i64) -> Option<&Frame> {
        let len = self.frames.len() as i64;
        let idx = if i < 0 { len + i } else { i };
        if idx < 0 || idx >= len {
            return None;
        }
        self.frames.get(idx as usize)
    }
}

fn assert_nav_kind(snap: &NodeSnapshot) -> Result<(), StackError> {
    if snap.kind.as_str() != NAV_KIND {
        return Err(StackError::NotNavKind {
            node: snap.id,
            found: snap.kind.as_str().to_string(),
        });
    }
    Ok(())
}

fn parse_node_ref(v: &JsonValue) -> Result<NodeId, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "frame_ref must be an object".to_string())?;
    let id_str = obj
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "frame_ref.id missing or not a string".to_string())?;
    id_str
        .parse()
        .map(NodeId)
        .map_err(|e| format!("frame_ref.id is not a valid uuid: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::InMemoryReader;
    use serde_json::json;

    fn nav(id: NodeId, alias: Option<&str>, target: Option<NodeId>) -> NodeSnapshot {
        let mut s = NodeSnapshot::new(id, NAV_KIND);
        s = match alias {
            Some(a) => s.with_slot(FRAME_ALIAS_SLOT, json!(a)),
            None => s.with_slot(FRAME_ALIAS_SLOT, JsonValue::Null),
        };
        s = match target {
            Some(t) => s.with_slot(FRAME_REF_SLOT, json!({ "id": t.to_string() })),
            None => s.with_slot(FRAME_REF_SLOT, JsonValue::Null),
        };
        s
    }

    #[test]
    fn build_stack_collects_frames_with_aliases() {
        let n1 = NodeId::new();
        let n2 = NodeId::new();
        let t1 = NodeId::new();
        let t2 = NodeId::new();
        let reader = InMemoryReader::new()
            .with(nav(n1, Some("org"), Some(t1)))
            .with(nav(n2, Some("site"), Some(t2)));

        let stack = ContextStack::build(&reader, &[n1, n2], 16).unwrap();
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.by_alias("org").unwrap().node_ref, t1);
        assert_eq!(stack.by_alias("site").unwrap().node_ref, t2);
        assert!(stack.shadowed().is_empty());
    }

    #[test]
    fn alias_shadowing_resolves_to_innermost() {
        let n1 = NodeId::new();
        let n2 = NodeId::new();
        let t_outer = NodeId::new();
        let t_inner = NodeId::new();
        let reader = InMemoryReader::new()
            .with(nav(n1, Some("current"), Some(t_outer)))
            .with(nav(n2, Some("current"), Some(t_inner)));

        let stack = ContextStack::build(&reader, &[n1, n2], 16).unwrap();
        assert_eq!(stack.by_alias("current").unwrap().node_ref, t_inner);
        assert_eq!(stack.shadowed(), &["current".to_string()]);
    }

    #[test]
    fn index_lookup_supports_negative() {
        let n1 = NodeId::new();
        let n2 = NodeId::new();
        let t1 = NodeId::new();
        let t2 = NodeId::new();
        let reader = InMemoryReader::new()
            .with(nav(n1, Some("a"), Some(t1)))
            .with(nav(n2, Some("b"), Some(t2)));
        let stack = ContextStack::build(&reader, &[n1, n2], 16).unwrap();
        assert_eq!(stack.by_index(0).unwrap().node_ref, t1);
        assert_eq!(stack.by_index(1).unwrap().node_ref, t2);
        assert_eq!(stack.by_index(-1).unwrap().node_ref, t2);
        assert_eq!(stack.by_index(-2).unwrap().node_ref, t1);
        assert!(stack.by_index(2).is_none());
        assert!(stack.by_index(-3).is_none());
    }

    #[test]
    fn nav_with_no_ref_contributes_no_frame() {
        let n1 = NodeId::new();
        let n2 = NodeId::new();
        let t2 = NodeId::new();
        let reader =
            InMemoryReader::new()
                .with(nav(n1, None, None))
                .with(nav(n2, Some("x"), Some(t2)));
        let stack = ContextStack::build(&reader, &[n1, n2], 16).unwrap();
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.by_alias("x").unwrap().node_ref, t2);
    }

    #[test]
    fn alias_without_ref_is_rejected() {
        let n1 = NodeId::new();
        let reader = InMemoryReader::new().with(nav(n1, Some("x"), None));
        let err = ContextStack::build(&reader, &[n1], 16).unwrap_err();
        assert_eq!(err, StackError::AliasWithoutRef(n1));
    }

    #[test]
    fn non_nav_kind_is_rejected() {
        let n1 = NodeId::new();
        let t1 = NodeId::new();
        let mut snap = NodeSnapshot::new(n1, "ui.page");
        snap = snap.with_slot(FRAME_REF_SLOT, json!({ "id": t1.to_string() }));
        let reader = InMemoryReader::new().with(snap);
        let err = ContextStack::build(&reader, &[n1], 16).unwrap_err();
        assert!(matches!(err, StackError::NotNavKind { .. }));
    }

    #[test]
    fn missing_node_is_reported() {
        let missing = NodeId::new();
        let reader = InMemoryReader::new();
        let err = ContextStack::build(&reader, &[missing], 16).unwrap_err();
        assert_eq!(err, StackError::NodeMissing(missing));
    }

    #[test]
    fn exceeding_max_depth_is_rejected() {
        let reader = InMemoryReader::new();
        let walk: Vec<NodeId> = (0..5).map(|_| NodeId::new()).collect();
        let err = ContextStack::build(&reader, &walk, 2).unwrap_err();
        assert_eq!(err, StackError::TooDeep { max: 2 });
    }
}
