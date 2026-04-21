//! Scope over the graph's link set.

use crate::links::dto::LinkDto;
use crate::links::schema::link_query_schema;
use crate::search::{ScopeError, ScopeHits, SearchScope};
use crate::store::GraphStore;

#[derive(Debug, Default, Clone)]
pub struct LinksQuery {
    pub filter: Option<String>,
    pub sort: Option<String>,
}

pub struct LinksScope<'g> {
    graph: &'g GraphStore,
}

impl<'g> LinksScope<'g> {
    pub fn new(graph: &'g GraphStore) -> Self {
        Self { graph }
    }
}

impl<'g> SearchScope for LinksScope<'g> {
    type Query = LinksQuery;
    type Hit = LinkDto;

    fn id(&self) -> &'static str {
        "links"
    }

    fn query(&self, q: Self::Query) -> Result<ScopeHits<Self::Hit>, ScopeError> {
        let mut rows: Vec<LinkDto> = self
            .graph
            .links()
            .into_iter()
            .map(|l| LinkDto::from_link(self.graph, l))
            .collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));

        if q.filter.is_none() && q.sort.is_none() {
            let total = rows.len();
            return Ok(ScopeHits::new(rows, total));
        }

        let schema = link_query_schema();
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
        let page = query::execute(rows, &validated)
            .map_err(|e| ScopeError::bad_request(e.to_string()))?;
        let total = page.meta.total;
        Ok(ScopeHits::new(page.data, total))
    }
}
