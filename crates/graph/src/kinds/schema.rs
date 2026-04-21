//! RSQL schema for the kind palette — one definition, consumed by every
//! transport.

use query::{FieldType, Operator, QuerySchema, SortField};

/// Queryable fields exposed on the palette.
///
/// | Field             | Type    | Ops                 | Example                        |
/// |-------------------|---------|---------------------|--------------------------------|
/// | `id`              | Text    | eq, ne, prefix, in  | `id==sys.logic.function`       |
/// | `org`             | Text    | eq, ne, prefix, in  | `org==com.listo`               |
/// | `display_name`    | Text    | eq, ne, prefix      | `display_name=prefix=MQTT`     |
/// | `facets`          | TextArr | contains, in        | `facets=contains=isCompute`    |
/// | `placement_class` | Text    | eq, ne              | `placement_class==free`        |
///
/// Default sort: ascending `id`. `max_page_size` is pinned high — the
/// palette is bounded in practice and we never paginate it.
pub fn kinds_query_schema() -> QuerySchema {
    QuerySchema::new(10_000, 10_000)
        .field(
            "id",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix, Operator::In],
        )
        .field(
            "org",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix, Operator::In],
        )
        .field(
            "display_name",
            FieldType::Text,
            [Operator::Eq, Operator::Ne, Operator::Prefix],
        )
        .field(
            "facets",
            FieldType::TextArr,
            [Operator::Contains, Operator::In],
        )
        .field(
            "placement_class",
            FieldType::Text,
            [Operator::Eq, Operator::Ne],
        )
        .default_sort([SortField::asc("id")])
}
