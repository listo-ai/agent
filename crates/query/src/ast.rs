use serde::{Deserialize, Serialize};

/// Transport-neutral raw query request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRequest {
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub page: Option<usize>,
    pub size: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Eq,
    Ne,
    Prefix,
    /// Comma-separated membership test: `field=in=a,b,c`.
    In,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterExpr {
    pub field: String,
    pub op: Operator,
    /// Raw filter value. For `In`, this is a comma-separated list.
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortField {
    pub field: String,
    pub dir: SortDir,
}
