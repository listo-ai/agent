//! `/api/v1/kinds` — the palette endpoint.
//!
//! Returns every kind the [`KindRegistry`] currently holds, optionally
//! filtered by facet or by a parent path the caller is considering
//! placing the kind under. The response is a thin wrapper around
//! [`spi::KindManifest`] with a computed `placement_class` hint so the
//! UI can group free vs. bound kinds without re-deriving it.
//!
//! Plugin-contributed kinds appear here the moment their plugin is
//! loaded — the endpoint is the palette source of truth for both
//! built-in and plugin kinds.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use spi::{Facet, KindManifest, ParentMatcher};

use crate::routes::{parse_path, ApiError};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/kinds", get(list_kinds))
}

#[derive(Debug, Deserialize)]
pub struct ListKindsQuery {
    /// Filter to kinds carrying this facet (camelCase: `isProtocol`).
    #[serde(default)]
    pub facet: Option<Facet>,

    /// Filter to kinds the graph would accept as a child of this
    /// existing parent path. Combines the parent's `may_contain` rules
    /// with the candidate's `must_live_under` rules — same predicate
    /// `GraphStore::create_child` uses.
    #[serde(default)]
    pub placeable_under: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct KindDto {
    #[serde(flatten)]
    pub manifest: KindManifest,
    /// `free` = empty `must_live_under`; `bound` = restricted parent.
    /// UI uses this to decide whether to show the kind in the global
    /// palette or only contextually under a matching parent.
    pub placement_class: &'static str,
}

impl KindDto {
    fn from_manifest(m: KindManifest) -> Self {
        let placement_class = if m.containment.must_live_under.is_empty() {
            "free"
        } else {
            "bound"
        };
        Self {
            manifest: m,
            placement_class,
        }
    }
}

async fn list_kinds(
    State(s): State<AppState>,
    Query(q): Query<ListKindsQuery>,
) -> Result<Json<Vec<KindDto>>, ApiError> {
    let registry = s.graph.kinds();
    let mut all = registry.all();

    if let Some(f) = q.facet {
        all.retain(|m| m.facets.contains(f));
    }

    if let Some(parent_raw) = q.placeable_under.as_deref() {
        let parent_path = parse_path(parent_raw)?;
        let parent = s
            .graph
            .get(&parent_path)
            .ok_or_else(|| ApiError::not_found(format!("no node at `{parent_path}`")))?;
        let parent_manifest = registry
            .get(&parent.kind)
            .ok_or_else(|| ApiError::bad_request("parent kind is not registered"))?;

        all.retain(|candidate| placement_allowed(&parent.kind, &parent_manifest, candidate));
    }

    all.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

    Ok(Json(all.into_iter().map(KindDto::from_manifest).collect()))
}

/// Mirrors the two-sided placement check in
/// [`graph::GraphStore::create_child`]: parent's `may_contain` must
/// admit the candidate, and candidate's `must_live_under` must admit
/// the parent. Empty on either side = unconstrained on that side.
fn placement_allowed(
    parent_kind: &spi::KindId,
    parent_manifest: &KindManifest,
    candidate: &KindManifest,
) -> bool {
    let may_contain_ok = parent_manifest.containment.may_contain.is_empty()
        || parent_manifest
            .containment
            .may_contain
            .iter()
            .any(|m| match m {
                ParentMatcher::Kind(k) => k == &candidate.id,
                ParentMatcher::Facet(f) => candidate.facets.contains(*f),
            });

    let must_live_under_ok = candidate.containment.must_live_under.is_empty()
        || candidate
            .containment
            .must_live_under
            .iter()
            .any(|m| m.matches(parent_kind, &parent_manifest.facets));

    may_contain_ok && must_live_under_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::{
        Cardinality, CascadePolicy, ContainmentSchema, FacetSet, KindId, KindManifest,
        ParentMatcher,
    };

    fn kind(id: &str, facets: FacetSet, containment: ContainmentSchema) -> KindManifest {
        KindManifest {
            id: KindId::new(id),
            display_name: None,
            facets,
            containment,
            slots: Vec::new(),
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        }
    }

    fn schema(must: Vec<ParentMatcher>, may: Vec<ParentMatcher>) -> ContainmentSchema {
        ContainmentSchema {
            must_live_under: must,
            may_contain: may,
            cardinality_per_parent: Cardinality::ManyPerParent,
            cascade: CascadePolicy::Strict,
        }
    }

    #[test]
    fn placement_class_free_when_no_parent_constraint() {
        let k = kind("sys.foo", FacetSet::default(), schema(vec![], vec![]));
        assert_eq!(KindDto::from_manifest(k).placement_class, "free");
    }

    #[test]
    fn placement_class_bound_when_must_live_under_present() {
        let k = kind(
            "sys.foo.point",
            FacetSet::default(),
            schema(vec![ParentMatcher::Kind(KindId::new("sys.foo"))], vec![]),
        );
        assert_eq!(KindDto::from_manifest(k).placement_class, "bound");
    }

    #[test]
    fn placement_allowed_honours_both_sides() {
        // Parent only accepts `sys.driver.demo.device` children.
        let parent = kind(
            "sys.driver.demo",
            FacetSet::of([Facet::IsDriver]),
            schema(
                vec![],
                vec![ParentMatcher::Kind(KindId::new("sys.driver.demo.device"))],
            ),
        );

        let bound = kind(
            "sys.driver.demo.device",
            FacetSet::default(),
            schema(
                vec![ParentMatcher::Kind(KindId::new("sys.driver.demo"))],
                vec![],
            ),
        );
        let free = kind(
            "sys.core.folder",
            FacetSet::default(),
            schema(vec![], vec![]),
        );

        assert!(placement_allowed(&parent.id, &parent, &bound));
        // `may_contain` doesn't admit folders — rejected even though
        // folders are free.
        assert!(!placement_allowed(&parent.id, &parent, &free));
    }
}
