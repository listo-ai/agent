//! The kinds search scope — one reusable function every transport calls.
//!
//! REST, CLI, MCP, and the fleet all share this entry point. The only
//! thing a transport does is shape its request into [`KindsQuery`] and
//! render the returned [`crate::search::ScopeHits`] back to its wire
//! format. No per-transport palette code, no duplicated filters.

use spi::{Facet, KindId, NodePath};

use crate::kinds::dto::KindDto;
use crate::kinds::schema::kinds_query_schema;
use crate::placement::placement_allowed;
use crate::search::{ScopeError, ScopeHits, SearchScope};
use crate::store::GraphStore;

/// One palette request. Values mirror the historical `/api/v1/kinds`
/// query params; transports do the parsing.
#[derive(Debug, Default, Clone)]
pub struct KindsQuery {
    /// Restrict to kinds carrying this facet. Shortcut — composes with
    /// `filter`.
    pub facet: Option<Facet>,
    /// Restrict to kinds the graph would accept under this existing
    /// parent path. Uses [`placement_allowed`] — same predicate as
    /// `GraphStore::create_child`, so the palette and write path never
    /// disagree.
    pub placeable_under: Option<NodePath>,
    /// RSQL filter over the fields in [`kinds_query_schema`].
    pub filter: Option<String>,
    /// Comma-separated sort fields; `-field` for descending.
    pub sort: Option<String>,
}

/// Wraps the graph store and exposes the kinds search scope.
///
/// Transports hold one of these and call [`SearchScope::query`] —
/// nothing else. Future scopes (`nodes`, `flows`, `audit`) implement
/// the same trait and get fan-out search for free via
/// [`crate::search::run`].
pub struct KindsScope<'g> {
    graph: &'g GraphStore,
}

impl<'g> KindsScope<'g> {
    pub fn new(graph: &'g GraphStore) -> Self {
        Self { graph }
    }
}

impl<'g> SearchScope for KindsScope<'g> {
    type Query = KindsQuery;
    type Hit = KindDto;

    fn id(&self) -> &'static str {
        "kinds"
    }

    fn query(&self, q: Self::Query) -> Result<ScopeHits<Self::Hit>, ScopeError> {
        let registry = self.graph.kinds();
        let mut manifests = registry.all();

        if let Some(f) = q.facet {
            manifests.retain(|m| m.facets.contains(f));
        }

        if let Some(parent_path) = q.placeable_under.as_ref() {
            let parent = self.graph.get(parent_path).ok_or_else(|| {
                ScopeError::not_found(format!("no node at `{parent_path}`"))
            })?;
            let parent_manifest = registry.get(&parent.kind).ok_or_else(|| {
                ScopeError::bad_request(format!(
                    "parent kind `{}` is not registered",
                    parent.kind
                ))
            })?;
            manifests.retain(|candidate| {
                placement_allowed(&parent.kind, &parent_manifest, candidate)
            });
        }

        let dtos: Vec<KindDto> = manifests.into_iter().map(KindDto::from_manifest).collect();

        // Fast path: no RSQL + no sort → historical contract (ascending
        // by id, every match).
        if q.filter.is_none() && q.sort.is_none() {
            let mut dtos = dtos;
            dtos.sort_by(|a, b| a.manifest.id.as_str().cmp(b.manifest.id.as_str()));
            let total = dtos.len();
            return Ok(ScopeHits::new(dtos, total));
        }

        let schema = kinds_query_schema();
        let max = schema.max_page_size();
        let validated = query::validate(
            &schema,
            query::QueryRequest {
                filter: q.filter,
                sort: q.sort,
                page: Some(1),
                size: Some(max),
            },
        )
        .map_err(|e| ScopeError::bad_request(e.to_string()))?;

        let page = query::execute(dtos, &validated)
            .map_err(|e| ScopeError::bad_request(e.to_string()))?;
        let total = page.meta.total;
        Ok(ScopeHits::new(page.data, total))
    }
}

/// Unused — kept as documentation of how a scope calls
/// [`KindId`]-dependent APIs. Removed if the palette never needs it.
#[allow(dead_code)]
fn _touch_kind_id(_k: &KindId) {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use spi::{
        Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest,
        ParentMatcher,
    };

    use super::*;
    use crate::event::NullSink;
    use crate::kind::KindRegistry;
    use crate::search::SearchScope;

    fn make_kind(id: &str, facets: FacetSet, containment: ContainmentSchema) -> KindManifest {
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

    fn open_schema() -> ContainmentSchema {
        ContainmentSchema {
            must_live_under: vec![],
            may_contain: vec![],
            cardinality_per_parent: Cardinality::ManyPerParent,
            cascade: CascadePolicy::Strict,
        }
    }

    fn seeded_graph() -> Arc<GraphStore> {
        let kinds = KindRegistry::new();
        kinds.register(make_kind(
            "sys.core.station",
            FacetSet::default(),
            ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![ParentMatcher::Kind(KindId::new("sys.core.folder"))],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
        ));
        kinds.register(make_kind(
            "sys.core.folder",
            FacetSet::default(),
            open_schema(),
        ));
        kinds.register(make_kind(
            "com.listo.mqtt.client",
            FacetSet::of([Facet::IsCompute]),
            open_schema(),
        ));
        kinds.register(make_kind(
            "com.acme.hello",
            FacetSet::default(),
            open_schema(),
        ));
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
    }

    #[test]
    fn returns_every_kind_by_default_sorted_by_id() {
        let graph = seeded_graph();
        let scope = KindsScope::new(&graph);
        let hits = scope.query(KindsQuery::default()).unwrap();
        let ids: Vec<&str> = hits.data.iter().map(|d| d.manifest.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "com.acme.hello",
                "com.listo.mqtt.client",
                "sys.core.folder",
                "sys.core.station",
            ]
        );
    }

    #[test]
    fn rsql_filter_by_org_narrows_to_publisher() {
        let graph = seeded_graph();
        let scope = KindsScope::new(&graph);
        let hits = scope
            .query(KindsQuery {
                filter: Some("org==com.listo".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(hits.total, 1);
        assert_eq!(hits.data[0].manifest.id.as_str(), "com.listo.mqtt.client");
    }

    #[test]
    fn facet_shortcut_narrows_to_compute() {
        let graph = seeded_graph();
        let scope = KindsScope::new(&graph);
        let hits = scope
            .query(KindsQuery {
                facet: Some(Facet::IsCompute),
                ..Default::default()
            })
            .unwrap();
        let ids: Vec<&str> = hits.data.iter().map(|d| d.manifest.id.as_str()).collect();
        assert_eq!(ids, vec!["com.listo.mqtt.client"]);
    }

    #[test]
    fn placeable_under_uses_placement_allowed() {
        let graph = seeded_graph();
        let scope = KindsScope::new(&graph);
        let hits = scope
            .query(KindsQuery {
                placeable_under: Some(NodePath::root()),
                ..Default::default()
            })
            .unwrap();
        // Station only admits `sys.core.folder`.
        let ids: Vec<&str> = hits.data.iter().map(|d| d.manifest.id.as_str()).collect();
        assert_eq!(ids, vec!["sys.core.folder"]);
    }

    #[test]
    fn placeable_under_missing_parent_is_not_found() {
        let graph = seeded_graph();
        let scope = KindsScope::new(&graph);
        let err = scope
            .query(KindsQuery {
                placeable_under: Some(NodePath::root().child("nowhere")),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, ScopeError::NotFound(_)));
    }

    #[test]
    fn rejects_unknown_rsql_field() {
        let graph = seeded_graph();
        let scope = KindsScope::new(&graph);
        let err = scope
            .query(KindsQuery {
                filter: Some("bogus==1".into()),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, ScopeError::BadRequest(_)));
    }
}
