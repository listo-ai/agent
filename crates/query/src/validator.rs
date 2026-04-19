use crate::parser::{page, parse_filters, parse_sort, size};
use crate::{FieldType, FilterExpr, QueryError, QueryRequest, QuerySchema, SortField};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedQuery {
    pub filters: Vec<FilterExpr>,
    pub sort: Vec<SortField>,
    pub page: usize,
    pub size: usize,
}

pub fn validate(schema: &QuerySchema, req: QueryRequest) -> Result<ValidatedQuery, QueryError> {
    let filters = parse_filters(req.filter.as_deref())?;
    let sort = parse_sort(req.sort.as_deref())?;

    for filter in &filters {
        let spec = schema
            .field_spec(&filter.field)
            .ok_or_else(|| QueryError::UnknownField(filter.field.clone()))?;
        if !spec.ops.contains(&filter.op) {
            return Err(QueryError::UnsupportedOperator {
                field: filter.field.clone(),
                op: filter.op,
            });
        }
        if matches!(filter.op, crate::Operator::Prefix | crate::Operator::In)
            && spec.ty != FieldType::Text
        {
            return Err(QueryError::OperatorTypeMismatch {
                field: filter.field.clone(),
                op: filter.op,
            });
        }
    }

    for sort_field in &sort {
        if schema.field_spec(&sort_field.field).is_none() {
            return Err(QueryError::UnknownField(sort_field.field.clone()));
        }
    }

    let page = page(&req)?.unwrap_or(1);
    let size = size(&req)?.unwrap_or(schema.default_page_size());
    if size > schema.max_page_size() {
        return Err(QueryError::PageSizeTooLarge {
            requested: size,
            max: schema.max_page_size(),
        });
    }

    Ok(ValidatedQuery {
        filters,
        sort: if sort.is_empty() {
            schema.default_sort_fields().to_vec()
        } else {
            sort
        },
        page,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Operator, SortField};

    fn schema() -> QuerySchema {
        QuerySchema::new(25, 100)
            .field("kind", FieldType::Text, [Operator::Eq, Operator::Ne])
            .field(
                "path",
                FieldType::Text,
                [Operator::Eq, Operator::Ne, Operator::Prefix],
            )
            .default_sort([SortField::asc("path")])
    }

    #[test]
    fn applies_defaults() {
        let out = validate(&schema(), QueryRequest::default()).unwrap();
        assert_eq!(out.page, 1);
        assert_eq!(out.size, 25);
        assert_eq!(out.sort, vec![SortField::asc("path")]);
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = validate(
            &schema(),
            QueryRequest {
                filter: Some("oops==x".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(err, QueryError::UnknownField(_)));
    }
}
