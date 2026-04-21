//! Flows search scope — hits `GET /api/v1/search?scope=flows` and the
//! matching fleet subject. The scope fetches from the repo through
//! [`FlowService`], projects to [`FlowDto`], and runs the shared RSQL
//! pipeline.
//!
//! Full-text search on the flow JSON document is deliberately out of
//! scope for this surface — that's what the analytics `/analyze`
//! endpoint is for. The palette should answer "find flow named X" in
//! sub-50ms, not run a JSON walk.

use data_entities::FlowDocument;
use graph::{ScopeError, ScopeHits, SearchScope};
use query::{FieldType, Operator, QuerySchema, SortField};
use serde::Serialize;

use crate::{FlowError, FlowService};

#[derive(Debug, Default, Clone)]
pub struct FlowsQuery {
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub page: Option<usize>,
    pub size: Option<usize>,
}

/// Wire shape for a flow palette row. Intentionally *excludes* the
/// full JSON document — the palette is a lookup surface, not a content
/// fetch. Callers who need the document hit `GET /api/v1/flows/:id`.
#[derive(Debug, Clone, Serialize)]
pub struct FlowDto {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_revision_id: Option<String>,
    pub head_seq: i64,
}

impl From<FlowDocument> for FlowDto {
    fn from(f: FlowDocument) -> Self {
        Self {
            id: f.id.to_string(),
            name: f.name,
            head_revision_id: f.head_revision_id.map(|r| r.to_string()),
            head_seq: f.head_seq,
        }
    }
}

/// RSQL fields exposed on the flows scope.
pub fn flows_query_schema() -> QuerySchema {
    QuerySchema::new(100, 10_000)
        .field(
            "id",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "name",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .default_sort([SortField::asc("name")])
}

/// Scope over a [`FlowService`].
pub struct FlowsScope<'s> {
    svc: &'s FlowService,
}

impl<'s> FlowsScope<'s> {
    pub fn new(svc: &'s FlowService) -> Self {
        Self { svc }
    }

    /// Paginated variant — returns the full `query::Page` meta so the
    /// transport can populate `{page, size, pages}` in the envelope.
    pub fn query_page(&self, q: FlowsQuery) -> Result<query::Page<FlowDto>, ScopeError> {
        // Pull every flow once; the palette is bounded and flows rarely
        // number in the thousands. Post-filter / sort / paginate in
        // memory via the shared RSQL executor — one code path for every
        // scope.
        let all: Vec<FlowDto> = self
            .svc
            .list_flows(u32::MAX, 0)
            .map_err(flow_err_to_scope)?
            .into_iter()
            .map(FlowDto::from)
            .collect();
        let validated = query::validate(
            &flows_query_schema(),
            query::QueryRequest {
                filter: q.filter,
                sort: q.sort,
                page: q.page,
                size: q.size,
            },
        )
        .map_err(|e| ScopeError::bad_request(e.to_string()))?;
        query::execute(all, &validated).map_err(|e| ScopeError::bad_request(e.to_string()))
    }
}

impl<'s> SearchScope for FlowsScope<'s> {
    type Query = FlowsQuery;
    type Hit = FlowDto;

    fn id(&self) -> &'static str {
        "flows"
    }

    fn query(&self, q: Self::Query) -> Result<ScopeHits<Self::Hit>, ScopeError> {
        let page = self.query_page(q)?;
        let total = page.meta.total;
        Ok(ScopeHits::new(page.data, total))
    }
}

fn flow_err_to_scope(err: FlowError) -> ScopeError {
    match err {
        FlowError::NotFound(_) | FlowError::RevisionNotFound(_) => {
            ScopeError::not_found(err.to_string())
        }
        _ => ScopeError::bad_request(err.to_string()),
    }
}
