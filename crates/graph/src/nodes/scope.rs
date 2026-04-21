//! The nodes search scope — the reusable entry point for "list live
//! nodes matching this filter".
//!
//! Implements [`crate::search::SearchScope`]. REST, fleet, CLI and the
//! MCP tool all call [`SearchScope::query`] with the same
//! [`NodesQuery`]; only request parsing and response rendering differ
//! between transports.

use crate::nodes::dto::NodeDto;
use crate::nodes::schema::node_query_schema;
use crate::search::{ScopeError, ScopeHits, SearchScope};
use crate::store::GraphStore;

/// One node-list request. Pagination uses the usual `page` / `size`
/// semantics from [`query::QueryRequest`].
#[derive(Debug, Default, Clone)]
pub struct NodesQuery {
    /// RSQL filter over the fields in [`node_query_schema`].
    pub filter: Option<String>,
    /// Comma-separated sort fields; `-field` for descending.
    pub sort: Option<String>,
    pub page: Option<usize>,
    pub size: Option<usize>,
}

/// Wraps the graph store and exposes the nodes search scope.
pub struct NodesScope<'g> {
    graph: &'g GraphStore,
}

impl<'g> NodesScope<'g> {
    pub fn new(graph: &'g GraphStore) -> Self {
        Self { graph }
    }

    /// Paginated variant — the transport can use this when it wants
    /// the full page metadata (`page`, `size`, `pages`). The default
    /// [`SearchScope::query`] path collapses pagination to
    /// `ScopeHits { data, total }`, which is all the generic envelope
    /// carries.
    pub fn query_page(&self, q: NodesQuery) -> Result<query::Page<NodeDto>, ScopeError> {
        let mut out: Vec<NodeDto> = self
            .graph
            .snapshots()
            .into_iter()
            .map(NodeDto::from)
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        let validated = query::validate(
            &node_query_schema(),
            query::QueryRequest {
                filter: q.filter,
                sort: q.sort,
                page: q.page,
                size: q.size,
            },
        )
        .map_err(|e| ScopeError::bad_request(e.to_string()))?;
        query::execute(out, &validated).map_err(|e| ScopeError::bad_request(e.to_string()))
    }
}

impl<'g> SearchScope for NodesScope<'g> {
    type Query = NodesQuery;
    type Hit = NodeDto;

    fn id(&self) -> &'static str {
        "nodes"
    }

    fn query(&self, q: Self::Query) -> Result<ScopeHits<Self::Hit>, ScopeError> {
        let page = self.query_page(q)?;
        let total = page.meta.total;
        Ok(ScopeHits::new(page.data, total))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use spi::{
        Cardinality, CascadePolicy, ContainmentSchema, Facet, FacetSet, KindId, KindManifest,
        NodePath, ParentMatcher,
    };

    use super::*;
    use crate::event::NullSink;
    use crate::kind::KindRegistry;
    use crate::search::SearchScope;

    fn kind(id: &str, containment: ContainmentSchema) -> KindManifest {
        KindManifest {
            id: KindId::new(id),
            display_name: None,
            facets: FacetSet::of([Facet::IsContainer]),
            containment,
            slots: Vec::new(),
            settings_schema: serde_json::Value::Null,
            msg_overrides: Default::default(),
            trigger_policy: Default::default(),
            schema_version: 1,
            views: Vec::new(),
        }
    }

    fn seeded_graph() -> Arc<GraphStore> {
        let kinds = KindRegistry::new();
        kinds.register(kind(
            "sys.core.station",
            ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![ParentMatcher::Kind(KindId::new("sys.core.folder"))],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
        ));
        kinds.register(kind(
            "sys.core.folder",
            ContainmentSchema {
                must_live_under: vec![],
                may_contain: vec![],
                cardinality_per_parent: Cardinality::ManyPerParent,
                cascade: CascadePolicy::Strict,
            },
        ));
        let graph = Arc::new(GraphStore::new(kinds, Arc::new(NullSink)));
        graph.create_root(KindId::new("sys.core.station")).unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("sys.core.folder"), "alpha")
            .unwrap();
        graph
            .create_child(&NodePath::root(), KindId::new("sys.core.folder"), "beta")
            .unwrap();
        graph
    }

    #[test]
    fn returns_every_node_by_default_sorted_by_path() {
        let graph = seeded_graph();
        let scope = NodesScope::new(&graph);
        let hits = scope.query(NodesQuery::default()).unwrap();
        let paths: Vec<&str> = hits.data.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, vec!["/", "/alpha", "/beta"]);
        assert_eq!(hits.total, 3);
    }

    #[test]
    fn rsql_filter_by_kind_narrows_results() {
        let graph = seeded_graph();
        let scope = NodesScope::new(&graph);
        let hits = scope
            .query(NodesQuery {
                filter: Some("kind==sys.core.folder".into()),
                ..Default::default()
            })
            .unwrap();
        let paths: Vec<&str> = hits.data.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, vec!["/alpha", "/beta"]);
    }

    #[test]
    fn query_page_applies_pagination_meta() {
        let graph = seeded_graph();
        let scope = NodesScope::new(&graph);
        let page = scope
            .query_page(NodesQuery {
                filter: Some("kind==sys.core.folder".into()),
                sort: Some("-path".into()),
                page: Some(1),
                size: Some(1),
            })
            .unwrap();
        assert_eq!(page.meta.total, 2);
        assert_eq!(page.meta.pages, 2);
        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].path, "/beta");
    }

    #[test]
    fn rejects_unknown_rsql_field() {
        let graph = seeded_graph();
        let scope = NodesScope::new(&graph);
        let err = scope
            .query(NodesQuery {
                filter: Some("bogus==1".into()),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, ScopeError::BadRequest(_)));
    }
}
