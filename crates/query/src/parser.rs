use crate::{FilterExpr, Operator, QueryError, QueryRequest, SortDir, SortField};

pub(crate) fn parse_filters(raw: Option<&str>) -> Result<Vec<FilterExpr>, QueryError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    raw.split(';')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(parse_filter)
        .collect()
}

pub(crate) fn parse_sort(raw: Option<&str>) -> Result<Vec<SortField>, QueryError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    raw.split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(parse_sort_field)
        .collect()
}

pub(crate) fn page(req: &QueryRequest) -> Result<Option<usize>, QueryError> {
    match req.page {
        Some(0) => Err(QueryError::InvalidPage),
        Some(page) => Ok(Some(page)),
        None => Ok(None),
    }
}

pub(crate) fn size(req: &QueryRequest) -> Result<Option<usize>, QueryError> {
    match req.size {
        Some(0) => Err(QueryError::InvalidSize),
        Some(size) => Ok(Some(size)),
        None => Ok(None),
    }
}

fn parse_filter(segment: &str) -> Result<FilterExpr, QueryError> {
    if let Some((field, value)) = segment.split_once("=prefix=") {
        return build_filter(field, Operator::Prefix, value, segment);
    }
    if let Some((field, value)) = segment.split_once("!=") {
        return build_filter(field, Operator::Ne, value, segment);
    }
    if let Some((field, value)) = segment.split_once("==") {
        return build_filter(field, Operator::Eq, value, segment);
    }
    Err(QueryError::InvalidFilter(segment.to_string()))
}

fn build_filter(
    field: &str,
    op: Operator,
    value: &str,
    raw: &str,
) -> Result<FilterExpr, QueryError> {
    let field = field.trim();
    if field.is_empty() || value.is_empty() {
        return Err(QueryError::InvalidFilter(raw.to_string()));
    }
    Ok(FilterExpr {
        field: field.to_string(),
        op,
        value: value.to_string(),
    })
}

fn parse_sort_field(raw: &str) -> Result<SortField, QueryError> {
    let (dir, field) = match raw.strip_prefix('-') {
        Some(field) => (SortDir::Desc, field),
        None => (SortDir::Asc, raw),
    };
    let field = field.trim();
    if field.is_empty() {
        return Err(QueryError::InvalidSort(raw.to_string()));
    }
    Ok(SortField {
        field: field.to_string(),
        dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rsql_like_filter_and_sort() {
        let filters = parse_filters(Some("kind==acme.core.station;path=prefix=/demo")).unwrap();
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].field, "kind");
        assert_eq!(filters[0].op, Operator::Eq);
        assert_eq!(filters[1].op, Operator::Prefix);

        let sort = parse_sort(Some("path,-kind")).unwrap();
        assert_eq!(sort.len(), 2);
        assert_eq!(sort[0].dir, SortDir::Asc);
        assert_eq!(sort[1].dir, SortDir::Desc);
    }
}
