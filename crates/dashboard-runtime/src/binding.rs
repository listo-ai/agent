//! Binding expressions — parse + evaluate `$stack.*`, `$self.*`,
//! `$user.*`, `$page.*`.
//!
//! Grammar (informal):
//!
//! ```text
//! binding := source ( "." ident )*
//! source  := "$stack"   "." ident        # alias
//!          | "$stack"   "[" int  "]"     # index (may be negative)
//!          | "$self"    "." ident
//!          | "$user"    "." ident
//!          | "$page"    "." ident
//! ```
//!
//! A trailing `.ident` walks one slot. If the current value is a nodeRef
//! (`{ "id": "<uuid>" }`), the walk reads that slot on the referenced
//! node — a single hop of ref-walking. Deeper multi-hop walks chain
//! naturally: `$stack.target.owner.name` walks `target`'s nodeRef →
//! `owner` slot (itself a nodeRef) → `name` slot.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value as JsonValue;
use spi::NodeId;
use thiserror::Error;

use crate::reader::NodeReader;
use crate::stack::{ContextStack, Frame};

/// The source selector at the head of a binding expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    StackAlias(String),
    StackIndex(i64),
    SelfSlot(String),
    UserClaim(String),
    PageField(String),
    /// `$vars.<key>` — author-declared constants on the ComponentTree.
    /// Resolved against the tree's `vars` map at resolve time; same
    /// ref-walk semantics as the other sources.
    Var(String),
}

/// Parsed binding: a source plus an optional trailing slot walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub source: Source,
    /// Dotted segments after the source selector.
    pub path: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BindingError {
    #[error("empty binding expression")]
    Empty,
    #[error("binding must start with `$stack`, `$self`, `$user`, `$page`, or `$vars` — got `{0}`")]
    UnknownSource(String),
    #[error("malformed binding: {0}")]
    Malformed(String),
    #[error("`$self` requires a field after the dot (e.g. `$self.name`)")]
    SelfNeedsField,
    #[error("stack index `{index}` out of range (stack has {len} frames)")]
    IndexOutOfRange { index: i64, len: usize },
    #[error("no frame aliased `{0}` in the context stack")]
    UnknownAlias(String),
    #[error("user claim `{0}` not present")]
    UnknownClaim(String),
    #[error("page state missing field `{0}`")]
    UnknownPageField(String),
    #[error("`$vars.{0}` not declared in ComponentTree.vars")]
    UnknownVar(String),
    #[error("cannot walk slot `{slot}` — current value is not an object or nodeRef")]
    WalkThroughNonObject { slot: String },
    #[error("ref walk: node `{0}` not found")]
    RefNodeMissing(NodeId),
    #[error("ref walk: slot `{slot}` not present on node `{node}`")]
    SlotMissing { node: NodeId, slot: String },
}

/// Runtime-owned value produced by a binding evaluation. A binding may
/// resolve to a primitive, a nodeRef, or a whole JSON object (e.g. if
/// the user binds `$self` directly). Callers typically expect one of
/// these shapes and coerce as needed.
pub type BindingValue = JsonValue;

impl Binding {
    pub fn parse(expr: &str) -> Result<Self, BindingError> {
        let expr = expr.trim();
        if expr.is_empty() {
            return Err(BindingError::Empty);
        }
        if !expr.starts_with('$') {
            return Err(BindingError::UnknownSource(expr.to_string()));
        }

        let (head, rest) = split_head(expr);

        let (source, remaining) = match head.as_str() {
            "$stack" => parse_stack_source(rest)?,
            "$self" => match rest {
                Some(r) => {
                    let (field, rest) = split_first_segment(r)?;
                    (Source::SelfSlot(field), rest)
                }
                None => return Err(BindingError::SelfNeedsField),
            },
            "$user" => {
                let r = rest.ok_or(BindingError::Malformed("$user needs a claim".into()))?;
                let (field, rest) = split_first_segment(r)?;
                (Source::UserClaim(field), rest)
            }
            "$vars" => {
                let r = rest.ok_or(BindingError::Malformed("$vars needs a key".into()))?;
                let (field, rest) = split_first_segment(r)?;
                (Source::Var(field), rest)
            }
            "$page" => {
                let r = rest.ok_or(BindingError::Malformed("$page needs a field".into()))?;
                let (field, rest) = split_first_segment(r)?;
                (Source::PageField(field), rest)
            }
            other => return Err(BindingError::UnknownSource(other.to_string())),
        };

        let path = match remaining {
            Some(s) if !s.is_empty() => s
                .split('.')
                .map(|p| {
                    if p.is_empty() {
                        Err(BindingError::Malformed("empty path segment".into()))
                    } else {
                        Ok(p.to_string())
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };

        Ok(Self { source, path })
    }

    /// Evaluate the binding against the current resolution context.
    pub fn evaluate<R: NodeReader + ?Sized>(
        &self,
        ctx: &EvalContext<'_, R>,
    ) -> Result<BindingValue, BindingError> {
        let (mut value, mut cursor_node) = self.seed_value(ctx)?;
        for segment in &self.path {
            let (next_value, next_node) = walk_one(&value, cursor_node, segment, ctx.reader, ctx)?;
            value = next_value;
            cursor_node = next_node;
        }
        Ok(value)
    }

    /// Returns `(value, cursor_node)` — where `cursor_node` is the
    /// node id that further `.slot` walks should read from (None if
    /// the source is a non-node value like a user claim or page state
    /// field).
    fn seed_value<R: NodeReader + ?Sized>(
        &self,
        ctx: &EvalContext<'_, R>,
    ) -> Result<(BindingValue, Option<NodeId>), BindingError> {
        match &self.source {
            Source::StackAlias(alias) => ctx
                .stack
                .by_alias(alias)
                .map(|f| (frame_to_ref(f), Some(f.node_ref)))
                .ok_or_else(|| BindingError::UnknownAlias(alias.clone())),
            Source::StackIndex(i) => ctx
                .stack
                .by_index(*i)
                .map(|f| (frame_to_ref(f), Some(f.node_ref)))
                .ok_or(BindingError::IndexOutOfRange {
                    index: *i,
                    len: ctx.stack.len(),
                }),
            Source::SelfSlot(slot) => {
                let v = read_slot(ctx.reader, &ctx.self_id, slot)?;
                ctx.record_slot(ctx.self_id, slot);
                Ok((v, None))
            }
            Source::UserClaim(claim) => ctx
                .user_claims
                .get(claim)
                .cloned()
                .map(|v| (v, None))
                .ok_or_else(|| BindingError::UnknownClaim(claim.clone())),
            Source::PageField(field) => ctx
                .page_state
                .as_object()
                .and_then(|m| m.get(field))
                .cloned()
                .map(|v| (v, None))
                .ok_or_else(|| BindingError::UnknownPageField(field.clone())),
            Source::Var(key) => ctx
                .vars
                .get(key)
                .cloned()
                .map(|v| (v, None))
                .ok_or_else(|| BindingError::UnknownVar(key.clone())),
        }
    }
}

/// Runtime inputs for evaluating a binding.
pub struct EvalContext<'a, R: NodeReader + ?Sized> {
    pub reader: &'a R,
    pub stack: &'a ContextStack,
    /// The node the binding is declared on (`ui.widget` or `ui.page`).
    pub self_id: NodeId,
    pub user_claims: &'a HashMap<String, JsonValue>,
    pub page_state: &'a JsonValue,
    /// ComponentTree-scoped author-declared constants. Empty when
    /// the tree has no `vars` section.
    pub vars: &'a HashMap<String, JsonValue>,
    /// Optional recorder for `(node_id, slot_name)` slot reads — feeds
    /// the subscription-plan emitter in `dashboard-transport::resolve`.
    /// Leave `None` when subscriptions aren't needed (e.g. dry-run).
    pub access_log: Option<&'a RefCell<Vec<(NodeId, String)>>>,
}

impl<'a, R: NodeReader + ?Sized> EvalContext<'a, R> {
    fn record_slot(&self, node: NodeId, slot: &str) {
        if let Some(log) = self.access_log {
            log.borrow_mut().push((node, slot.to_string()));
        }
    }
}

fn frame_to_ref(f: &Frame) -> JsonValue {
    serde_json::json!({ "id": f.node_ref.to_string() })
}

fn read_slot<R: NodeReader + ?Sized>(
    reader: &R,
    node: &NodeId,
    slot: &str,
) -> Result<JsonValue, BindingError> {
    let snap = reader
        .get(node)
        .ok_or(BindingError::RefNodeMissing(*node))?;
    snap.slots
        .get(slot)
        .cloned()
        .ok_or_else(|| BindingError::SlotMissing {
            node: *node,
            slot: slot.to_string(),
        })
}

fn walk_one<R: NodeReader + ?Sized>(
    value: &JsonValue,
    cursor_node: Option<NodeId>,
    segment: &str,
    reader: &R,
    ctx: &EvalContext<'_, R>,
) -> Result<(JsonValue, Option<NodeId>), BindingError> {
    if let Some(node) = try_as_noderef(value) {
        let v = read_slot(reader, &node, segment)?;
        ctx.record_slot(node, segment);
        // If the slot value is itself a nodeRef, the next walk step
        // reads from that node; otherwise further walks index JSON
        // objects.
        let next_cursor = try_as_noderef(&v).or(Some(node));
        return Ok((v, next_cursor));
    }
    if let Some(obj) = value.as_object() {
        let v = obj
            .get(segment)
            .cloned()
            .ok_or_else(|| BindingError::WalkThroughNonObject {
                slot: segment.to_string(),
            })?;
        return Ok((v, cursor_node));
    }
    Err(BindingError::WalkThroughNonObject {
        slot: segment.to_string(),
    })
}

fn try_as_noderef(v: &JsonValue) -> Option<NodeId> {
    let obj = v.as_object()?;
    // Only treat as nodeRef if `id` is the sole field — avoids
    // accidentally ref-walking a rich object that happens to carry an
    // `id` key.
    if obj.len() != 1 {
        return None;
    }
    let id_str = obj.get("id")?.as_str()?;
    id_str.parse().map(NodeId).ok()
}

fn split_head(expr: &str) -> (String, Option<&str>) {
    // Head is $stack / $self / $user / $page up to the first `.` or `[`.
    let end = expr.find(['.', '[']).unwrap_or(expr.len());
    let (head, rest) = expr.split_at(end);
    let rest = if rest.starts_with('.') {
        Some(&rest[1..])
    } else if rest.starts_with('[') {
        Some(rest)
    } else if rest.is_empty() {
        None
    } else {
        Some(rest)
    };
    (head.to_string(), rest)
}

fn parse_stack_source(rest: Option<&str>) -> Result<(Source, Option<&str>), BindingError> {
    let rest = rest.ok_or_else(|| {
        BindingError::Malformed("$stack must be followed by .alias or [index]".into())
    })?;
    if rest.starts_with('[') {
        let close = rest
            .find(']')
            .ok_or_else(|| BindingError::Malformed("$stack[...] missing closing bracket".into()))?;
        let idx: i64 = rest[1..close]
            .parse()
            .map_err(|_| BindingError::Malformed("$stack index is not an integer".into()))?;
        let after = &rest[close + 1..];
        let after = if let Some(stripped) = after.strip_prefix('.') {
            Some(stripped)
        } else if after.is_empty() {
            None
        } else {
            return Err(BindingError::Malformed(format!(
                "unexpected chars after $stack[{idx}]: `{after}`"
            )));
        };
        Ok((Source::StackIndex(idx), after))
    } else {
        let (alias, after) = split_first_segment(rest)?;
        Ok((Source::StackAlias(alias), after))
    }
}

fn split_first_segment(s: &str) -> Result<(String, Option<&str>), BindingError> {
    let end = s.find('.').unwrap_or(s.len());
    if end == 0 {
        return Err(BindingError::Malformed("empty identifier".into()));
    }
    let (head, rest) = s.split_at(end);
    let rest = if rest.starts_with('.') {
        Some(&rest[1..])
    } else {
        None
    };
    Ok((head.to_string(), rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::{InMemoryReader, NodeSnapshot};
    use serde_json::json;

    fn ctx<'a, R: NodeReader>(
        reader: &'a R,
        stack: &'a ContextStack,
        self_id: NodeId,
        user: &'a HashMap<String, JsonValue>,
        page: &'a JsonValue,
    ) -> EvalContext<'a, R> {
        // Test helper: every call uses empty vars. Leaks a fresh
        // static on each invocation rather than threading vars through
        // every existing test.
        let vars: &'static HashMap<String, JsonValue> =
            Box::leak(Box::new(HashMap::new()));
        EvalContext {
            reader,
            stack,
            self_id,
            user_claims: user,
            page_state: page,
            vars,
            access_log: None,
        }
    }

    #[test]
    fn parse_stack_alias_no_path() {
        let b = Binding::parse("$stack.target").unwrap();
        assert_eq!(b.source, Source::StackAlias("target".into()));
        assert!(b.path.is_empty());
    }

    #[test]
    fn parse_stack_alias_with_path() {
        let b = Binding::parse("$stack.target.name").unwrap();
        assert_eq!(b.source, Source::StackAlias("target".into()));
        assert_eq!(b.path, vec!["name"]);
    }

    #[test]
    fn parse_stack_index_positive() {
        let b = Binding::parse("$stack[0].slot").unwrap();
        assert_eq!(b.source, Source::StackIndex(0));
        assert_eq!(b.path, vec!["slot"]);
    }

    #[test]
    fn parse_stack_index_negative() {
        let b = Binding::parse("$stack[-1]").unwrap();
        assert_eq!(b.source, Source::StackIndex(-1));
        assert!(b.path.is_empty());
    }

    #[test]
    fn parse_self_user_page() {
        assert_eq!(
            Binding::parse("$self.name").unwrap().source,
            Source::SelfSlot("name".into())
        );
        assert_eq!(
            Binding::parse("$user.orgId").unwrap().source,
            Source::UserClaim("orgId".into())
        );
        assert_eq!(
            Binding::parse("$page.selectedRow").unwrap().source,
            Source::PageField("selectedRow".into())
        );
    }

    #[test]
    fn parse_rejects_bad_inputs() {
        assert_eq!(Binding::parse("").unwrap_err(), BindingError::Empty);
        assert!(matches!(
            Binding::parse("foo").unwrap_err(),
            BindingError::UnknownSource(_)
        ));
        assert!(matches!(
            Binding::parse("$stack").unwrap_err(),
            BindingError::Malformed(_)
        ));
        assert_eq!(
            Binding::parse("$self").unwrap_err(),
            BindingError::SelfNeedsField
        );
        assert!(matches!(
            Binding::parse("$stack[abc]").unwrap_err(),
            BindingError::Malformed(_)
        ));
        assert!(matches!(
            Binding::parse("$stack..x").unwrap_err(),
            BindingError::Malformed(_)
        ));
    }

    #[test]
    fn eval_stack_alias_returns_noderef() {
        let n1 = NodeId::new();
        let t1 = NodeId::new();
        let reader = InMemoryReader::new().with(
            NodeSnapshot::new(n1, "ui.nav")
                .with_slot("frame_alias", json!("target"))
                .with_slot("frame_ref", json!({ "id": t1.to_string() })),
        );
        let stack = ContextStack::build(&reader, &[n1], 16).unwrap();
        let empty_user: HashMap<String, JsonValue> = HashMap::new();
        let page = json!({});
        let b = Binding::parse("$stack.target").unwrap();
        let v = b
            .evaluate(&ctx(&reader, &stack, NodeId::new(), &empty_user, &page))
            .unwrap();
        assert_eq!(v, json!({ "id": t1.to_string() }));
    }

    #[test]
    fn eval_stack_alias_then_walks_ref_slot() {
        let n1 = NodeId::new();
        let t1 = NodeId::new();
        let reader = InMemoryReader::new()
            .with(
                NodeSnapshot::new(n1, "ui.nav")
                    .with_slot("frame_alias", json!("target"))
                    .with_slot("frame_ref", json!({ "id": t1.to_string() })),
            )
            .with(NodeSnapshot::new(t1, "sys.whatever").with_slot("name", json!("Site A")));
        let stack = ContextStack::build(&reader, &[n1], 16).unwrap();
        let empty: HashMap<String, JsonValue> = HashMap::new();
        let page = json!({});
        let b = Binding::parse("$stack.target.name").unwrap();
        let v = b
            .evaluate(&ctx(&reader, &stack, NodeId::new(), &empty, &page))
            .unwrap();
        assert_eq!(v, json!("Site A"));
    }

    #[test]
    fn eval_multi_hop_ref_walk() {
        let n1 = NodeId::new();
        let site = NodeId::new();
        let owner = NodeId::new();
        let reader = InMemoryReader::new()
            .with(
                NodeSnapshot::new(n1, "ui.nav")
                    .with_slot("frame_alias", json!("t"))
                    .with_slot("frame_ref", json!({ "id": site.to_string() })),
            )
            .with(
                NodeSnapshot::new(site, "sys.site")
                    .with_slot("owner", json!({ "id": owner.to_string() })),
            )
            .with(NodeSnapshot::new(owner, "sys.person").with_slot("name", json!("Ada")));
        let stack = ContextStack::build(&reader, &[n1], 16).unwrap();
        let empty: HashMap<String, JsonValue> = HashMap::new();
        let page = json!({});
        let v = Binding::parse("$stack.t.owner.name")
            .unwrap()
            .evaluate(&ctx(&reader, &stack, NodeId::new(), &empty, &page))
            .unwrap();
        assert_eq!(v, json!("Ada"));
    }

    #[test]
    fn eval_self_reads_own_slot() {
        let self_id = NodeId::new();
        let reader = InMemoryReader::new()
            .with(NodeSnapshot::new(self_id, "ui.widget").with_slot("title", json!("Hello")));
        let stack = ContextStack::empty();
        let empty: HashMap<String, JsonValue> = HashMap::new();
        let page = json!({});
        let v = Binding::parse("$self.title")
            .unwrap()
            .evaluate(&ctx(&reader, &stack, self_id, &empty, &page))
            .unwrap();
        assert_eq!(v, json!("Hello"));
    }

    #[test]
    fn eval_user_claim_and_page_field() {
        let reader = InMemoryReader::new();
        let stack = ContextStack::empty();
        let mut user: HashMap<String, JsonValue> = HashMap::new();
        user.insert("orgId".into(), json!("sys"));
        let page = json!({ "selectedRow": 3 });
        let self_id = NodeId::new();
        assert_eq!(
            Binding::parse("$user.orgId")
                .unwrap()
                .evaluate(&ctx(&reader, &stack, self_id, &user, &page))
                .unwrap(),
            json!("sys")
        );
        assert_eq!(
            Binding::parse("$page.selectedRow")
                .unwrap()
                .evaluate(&ctx(&reader, &stack, self_id, &user, &page))
                .unwrap(),
            json!(3)
        );
    }

    #[test]
    fn eval_unknown_alias_is_error() {
        let reader = InMemoryReader::new();
        let stack = ContextStack::empty();
        let user: HashMap<String, JsonValue> = HashMap::new();
        let page = json!({});
        let err = Binding::parse("$stack.missing")
            .unwrap()
            .evaluate(&ctx(&reader, &stack, NodeId::new(), &user, &page))
            .unwrap_err();
        assert_eq!(err, BindingError::UnknownAlias("missing".into()));
    }

    #[test]
    fn access_log_captures_slot_reads_across_ref_walks() {
        let n1 = NodeId::new();
        let site = NodeId::new();
        let owner = NodeId::new();
        let reader = InMemoryReader::new()
            .with(
                NodeSnapshot::new(n1, "ui.nav")
                    .with_slot("frame_alias", json!("t"))
                    .with_slot("frame_ref", json!({ "id": site.to_string() })),
            )
            .with(
                NodeSnapshot::new(site, "sys.site")
                    .with_slot("owner", json!({ "id": owner.to_string() })),
            )
            .with(NodeSnapshot::new(owner, "sys.person").with_slot("name", json!("Ada")));
        let stack = ContextStack::build(&reader, &[n1], 16).unwrap();
        let empty: HashMap<String, JsonValue> = HashMap::new();
        let page = json!({});

        let empty_vars: HashMap<String, JsonValue> = HashMap::new();
        let log = RefCell::new(Vec::new());
        let ctx = EvalContext {
            reader: &reader,
            stack: &stack,
            self_id: NodeId::new(),
            user_claims: &empty,
            page_state: &page,
            vars: &empty_vars,
            access_log: Some(&log),
        };
        let _ = Binding::parse("$stack.t.owner.name")
            .unwrap()
            .evaluate(&ctx)
            .unwrap();
        let entries = log.into_inner();
        // Walk visits: site.owner (one read), owner.name (second read).
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], (site, "owner".to_string()));
        assert_eq!(entries[1], (owner, "name".to_string()));
    }

    #[test]
    fn eval_index_out_of_range() {
        let reader = InMemoryReader::new();
        let stack = ContextStack::empty();
        let user: HashMap<String, JsonValue> = HashMap::new();
        let page = json!({});
        let err = Binding::parse("$stack[0]")
            .unwrap()
            .evaluate(&ctx(&reader, &stack, NodeId::new(), &user, &page))
            .unwrap_err();
        assert_eq!(err, BindingError::IndexOutOfRange { index: 0, len: 0 });
    }
}
