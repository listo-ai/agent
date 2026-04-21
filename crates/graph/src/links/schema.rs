//! RSQL schema for links.

use query::{FieldType, Operator, QuerySchema, SortField};

/// Queryable fields on a link row.
///
/// | Field              | Type | Ops            | Example                          |
/// |--------------------|------|----------------|----------------------------------|
/// | `id`               | Text | eq, ne, prefix | `id==…`                          |
/// | `source.node_id`   | Text | eq, ne, prefix | `source.node_id==…`              |
/// | `source.path`      | Text | eq, ne, prefix | `source.path=prefix=/flow1`      |
/// | `source.slot`      | Text | eq, ne, prefix | `source.slot==out`               |
/// | `target.node_id`   | Text | eq, ne, prefix | `target.node_id==…`              |
/// | `target.path`      | Text | eq, ne, prefix | `target.path=prefix=/flow1`      |
/// | `target.slot`      | Text | eq, ne, prefix | `target.slot==in`                |
/// | `scope_path`       | Text | eq, ne, prefix | `scope_path==/flow1`             |
pub fn link_query_schema() -> QuerySchema {
    let text = |ops| ops;
    QuerySchema::new(500, 5_000)
        .field(
            "id",
            FieldType::Text,
            text([Operator::Eq, Operator::Ne, Operator::Prefix]),
        )
        .field(
            "source.node_id",
            FieldType::Text,
            text([Operator::Eq, Operator::Ne, Operator::Prefix]),
        )
        .field(
            "source.path",
            FieldType::Text,
            text([Operator::Eq, Operator::Ne, Operator::Prefix]),
        )
        .field(
            "source.slot",
            FieldType::Text,
            text([Operator::Eq, Operator::Ne, Operator::Prefix]),
        )
        .field(
            "target.node_id",
            FieldType::Text,
            text([Operator::Eq, Operator::Ne, Operator::Prefix]),
        )
        .field(
            "target.path",
            FieldType::Text,
            text([Operator::Eq, Operator::Ne, Operator::Prefix]),
        )
        .field(
            "target.slot",
            FieldType::Text,
            text([Operator::Eq, Operator::Ne, Operator::Prefix]),
        )
        .field(
            "scope_path",
            FieldType::Text,
            text([Operator::Eq, Operator::Ne, Operator::Prefix]),
        )
        .default_sort([SortField::asc("id")])
}
