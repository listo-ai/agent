//! RSQL schema for node snapshots — one definition, consumed by every
//! transport.

use query::{FieldType, Operator, QuerySchema, SortField};

/// Queryable fields exposed on node snapshots.
///
/// | Field         | Type  | Ops                | Example                               |
/// |---------------|-------|--------------------|---------------------------------------|
/// | `id`          | Text  | eq, ne, prefix, in | `id==…`                               |
/// | `kind`        | Text  | eq, ne, prefix     | `kind==sys.core.folder`               |
/// | `path`        | Text  | eq, ne, prefix     | `path=prefix=/station/floor1`         |
/// | `parent_id`   | Text  | eq, ne, prefix     | `parent_id==…`                        |
/// | `parent_path` | Text  | eq, ne, prefix     | `parent_path==/station/floor1`        |
/// | `lifecycle`   | Text  | eq, ne             | `lifecycle==active`                   |
///
/// Default sort: ascending `path`.
pub fn node_query_schema() -> QuerySchema {
    QuerySchema::new(100, 1000)
        .field(
            "id",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix, Operator::In],
        )
        .field(
            "kind",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "path",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "parent_id",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "parent_path",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field("lifecycle", FieldType::Text, [Operator::Eq, Operator::Ne])
        .default_sort([SortField::asc("path")])
}
