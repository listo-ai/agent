use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{FilterExpr, Operator, QueryError, SortDir, ValidatedQuery};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageMeta {
    pub total: usize,
    pub page: usize,
    pub size: usize,
    pub pages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub meta: PageMeta,
}

pub fn execute<T>(
    items: impl IntoIterator<Item = T>,
    query: &ValidatedQuery,
) -> Result<Page<T>, QueryError>
where
    T: Serialize,
{
    let mut rows = items
        .into_iter()
        .map(|item| {
            let json = serde_json::to_value(&item)?;
            Ok(Row { item, json })
        })
        .collect::<Result<Vec<_>, QueryError>>()?;

    rows.retain(|row| {
        query
            .filters
            .iter()
            .all(|filter| matches_filter(&row.json, filter))
    });
    rows.sort_by(|a, b| compare_rows(&a.json, &b.json, query));

    let total = rows.len();
    let pages = if total == 0 {
        0
    } else {
        total.div_ceil(query.size)
    };
    let start = query.size.saturating_mul(query.page.saturating_sub(1));
    let data = rows
        .into_iter()
        .skip(start)
        .take(query.size)
        .map(|row| row.item)
        .collect();

    Ok(Page {
        data,
        meta: PageMeta {
            total,
            page: query.page,
            size: query.size,
            pages,
        },
    })
}

struct Row<T> {
    item: T,
    json: Value,
}

fn matches_filter(json: &Value, filter: &FilterExpr) -> bool {
    match filter.op {
        Operator::Exists => {
            let present = field_value(json, &filter.field).is_some();
            // value is "true" or "false"
            let want = filter.value != "false";
            present == want
        }
        _ => {
            let Some(actual) = field_value(json, &filter.field) else {
                return false;
            };
            match filter.op {
                Operator::Eq => scalar_text(actual) == Some(filter.value.as_str()),
                Operator::Ne => scalar_text(actual).is_some_and(|v| v != filter.value),
                Operator::Prefix => {
                    scalar_text(actual).is_some_and(|v| v.starts_with(&filter.value))
                }
                Operator::In => {
                    let needle = scalar_text(actual);
                    filter
                        .value
                        .split(',')
                        .any(|c| needle == Some(c.trim()))
                }
                Operator::Contains => array_contains(actual, &filter.value),
                Operator::Exists => unreachable!(),
            }
        }
    }
}

fn compare_rows(left: &Value, right: &Value, query: &ValidatedQuery) -> Ordering {
    for field in &query.sort {
        let ord = compare_field(
            field_value(left, &field.field),
            field_value(right, &field.field),
        );
        let ord = match field.dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        };
        if !ord.is_eq() {
            return ord;
        }
    }
    Ordering::Equal
}

fn compare_field(left: Option<&Value>, right: Option<&Value>) -> Ordering {
    match (left.and_then(scalar_text), right.and_then(scalar_text)) {
        (Some(left), Some(right)) => left.cmp(right),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn field_value<'a>(json: &'a Value, field: &str) -> Option<&'a Value> {
    let mut current = json;
    for part in field.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

fn scalar_text(value: &Value) -> Option<&str> {
    value.as_str().or(match value {
        Value::Null => Some("null"),
        Value::Bool(true) => Some("true"),
        Value::Bool(false) => Some("false"),
        _ => None,
    })
}

/// Returns true if `arr` is a JSON array that contains `needle` as a string element.
fn array_contains(arr: &Value, needle: &str) -> bool {
    arr.as_array()
        .is_some_and(|items| items.iter().any(|v| v.as_str() == Some(needle)))
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::*;
    use crate::{validate, FieldType, Operator, QueryRequest, QuerySchema, SortField};

    #[derive(Serialize)]
    struct Device<'a> {
        kind: &'a str,
        path: &'a str,
        lifecycle: &'a str,
    }

    fn schema() -> QuerySchema {
        QuerySchema::new(2, 10)
            .field("kind", FieldType::Text, [Operator::Eq, Operator::Ne])
            .field(
                "path",
                FieldType::Text,
                [Operator::Eq, Operator::Ne, Operator::Prefix],
            )
            .field("lifecycle", FieldType::Text, [Operator::Eq, Operator::Ne])
            .default_sort([SortField::asc("path")])
    }

    #[test]
    fn filters_sorts_and_pages() {
        let query = validate(
            &schema(),
            QueryRequest {
                filter: Some("path=prefix=/demo".into()),
                sort: Some("-path".into()),
                page: Some(1),
                size: Some(1),
            },
        )
        .unwrap();
        let page = execute(
            [
                Device {
                    kind: "a",
                    path: "/demo/a",
                    lifecycle: "created",
                },
                Device {
                    kind: "a",
                    path: "/demo/z",
                    lifecycle: "created",
                },
                Device {
                    kind: "b",
                    path: "/other",
                    lifecycle: "active",
                },
            ],
            &query,
        )
        .unwrap();
        assert_eq!(page.meta.total, 2);
        assert_eq!(page.meta.pages, 2);
        assert_eq!(page.data.len(), 1);
        let top = serde_json::to_value(&page.data[0]).unwrap();
        assert_eq!(top.get("path").unwrap(), "/demo/z");
    }
}
