//! Blocks search scope — the loaded-plugin registry exposed through
//! [`graph::SearchScope`].
//!
//! Same ownership rule as every other scope: DTO, RSQL schema and the
//! scope impl live next to the data (the [`BlockRegistry`]), not in
//! `transport-rest`. REST, CLI, MCP and fleet all dispatch to this
//! single implementation.

use graph::{ScopeError, ScopeHits, SearchScope};
use query::{FieldType, Operator, QuerySchema, SortField};
use serde::Serialize;

use crate::manifest::BlockId;
use crate::registry::{BlockRegistry, LoadedPluginSummary, PluginLifecycle};

/// One palette request.
#[derive(Debug, Default, Clone)]
pub struct BlocksQuery {
    /// RSQL filter over the fields in [`blocks_query_schema`].
    pub filter: Option<String>,
    /// Comma-separated sort fields; `-field` for descending.
    pub sort: Option<String>,
}

/// Wire shape of one loaded-plugin entry — a flattened projection of
/// [`LoadedPluginSummary`] with `id` rendered as a string so RSQL can
/// match it with `eq` / `prefix`.
#[derive(Debug, Clone, Serialize)]
pub struct BlockDto {
    pub id: String,
    pub version: String,
    pub lifecycle: PluginLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub has_ui: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_entry: Option<String>,
    pub kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_errors: Vec<String>,
}

impl From<LoadedPluginSummary> for BlockDto {
    fn from(s: LoadedPluginSummary) -> Self {
        let LoadedPluginSummary {
            id,
            version,
            lifecycle,
            display_name,
            description,
            has_ui,
            ui_entry,
            kinds,
            load_errors,
        } = s;
        Self {
            id: id_to_string(&id),
            version,
            lifecycle,
            display_name,
            description,
            has_ui,
            ui_entry,
            kinds,
            load_errors,
        }
    }
}

fn id_to_string(id: &BlockId) -> String {
    // BlockId's Display is the canonical rendering.
    format!("{id}")
}

/// RSQL fields exposed on the blocks scope.
pub fn blocks_query_schema() -> QuerySchema {
    QuerySchema::new(1_000, 1_000)
        .field(
            "id",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix, Operator::In],
        )
        .field(
            "version",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "lifecycle",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::In],
        )
        .field(
            "display_name",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "kinds",
            FieldType::TextArr,
            [Operator::Contains, Operator::In],
        )
        .default_sort([SortField::asc("id")])
}

/// Scope over a [`BlockRegistry`].
pub struct BlocksScope<'r> {
    registry: &'r BlockRegistry,
}

impl<'r> BlocksScope<'r> {
    pub fn new(registry: &'r BlockRegistry) -> Self {
        Self { registry }
    }
}

impl<'r> SearchScope for BlocksScope<'r> {
    type Query = BlocksQuery;
    type Hit = BlockDto;

    fn id(&self) -> &'static str {
        "blocks"
    }

    fn query(&self, q: Self::Query) -> Result<ScopeHits<Self::Hit>, ScopeError> {
        let rows: Vec<BlockDto> = self
            .registry
            .list()
            .into_iter()
            .map(BlockDto::from)
            .collect();

        if q.filter.is_none() && q.sort.is_none() {
            let total = rows.len();
            return Ok(ScopeHits::new(rows, total));
        }

        let schema = blocks_query_schema();
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
