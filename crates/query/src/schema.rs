use std::collections::{BTreeMap, BTreeSet};

use crate::{Operator, SortDir, SortField};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Plain scalar text — supports `eq`, `ne`, `prefix`, `in`.
    Text,
    /// JSON array of text — supports `contains`, `in`, `exists`.
    TextArr,
    /// Enumerated value — supports `eq`, `ne`, `in`.
    Enum,
}

#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub ty: FieldType,
    pub ops: BTreeSet<Operator>,
}

impl FieldSpec {
    pub fn new(ty: FieldType, ops: impl IntoIterator<Item = Operator>) -> Self {
        Self {
            ty,
            ops: ops.into_iter().collect(),
        }
    }
}

/// A pattern-field rule covering all `tags.kv.<key>` lookups.
#[derive(Debug, Clone)]
struct PatternField {
    /// Field name prefix to match, e.g. `"tags.kv."`.
    prefix: String,
    spec: FieldSpec,
}

#[derive(Debug, Clone)]
pub struct QuerySchema {
    fields: BTreeMap<String, FieldSpec>,
    /// Pattern fields — tried after exact lookup fails.
    pattern_fields: Vec<PatternField>,
    default_sort: Vec<SortField>,
    default_page_size: usize,
    max_page_size: usize,
}

impl QuerySchema {
    pub fn new(default_page_size: usize, max_page_size: usize) -> Self {
        Self {
            fields: BTreeMap::new(),
            pattern_fields: Vec::new(),
            default_sort: Vec::new(),
            default_page_size,
            max_page_size,
        }
    }

    pub fn field(
        mut self,
        name: impl Into<String>,
        ty: FieldType,
        ops: impl IntoIterator<Item = Operator>,
    ) -> Self {
        self.fields.insert(name.into(), FieldSpec::new(ty, ops));
        self
    }

    /// Register a pattern field: any request field that starts with
    /// `prefix` resolves to the given type and operator set.
    ///
    /// Example:
    /// ```ignore
    /// schema.pattern_field("tags.kv.", FieldType::Text,
    ///     [Operator::Eq, Operator::Ne, Operator::In, Operator::Exists])
    /// ```
    pub fn pattern_field(
        mut self,
        prefix: impl Into<String>,
        ty: FieldType,
        ops: impl IntoIterator<Item = Operator>,
    ) -> Self {
        self.pattern_fields.push(PatternField {
            prefix: prefix.into(),
            spec: FieldSpec::new(ty, ops),
        });
        self
    }

    pub fn default_sort(mut self, fields: impl IntoIterator<Item = SortField>) -> Self {
        self.default_sort = fields.into_iter().collect();
        self
    }

    pub fn default_page_size(&self) -> usize {
        self.default_page_size
    }

    pub fn max_page_size(&self) -> usize {
        self.max_page_size
    }

    /// Look up a field, falling back to pattern-field rules.
    pub fn field_spec(&self, field: &str) -> Option<&FieldSpec> {
        if let Some(spec) = self.fields.get(field) {
            return Some(spec);
        }
        for pf in &self.pattern_fields {
            if field.starts_with(&pf.prefix) {
                return Some(&pf.spec);
            }
        }
        None
    }

    pub fn default_sort_fields(&self) -> &[SortField] {
        &self.default_sort
    }
}

impl SortField {
    pub fn asc(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            dir: SortDir::Asc,
        }
    }

    pub fn desc(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            dir: SortDir::Desc,
        }
    }
}
