use std::collections::{BTreeMap, BTreeSet};

use crate::{Operator, SortDir, SortField};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Text,
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

#[derive(Debug, Clone)]
pub struct QuerySchema {
    fields: BTreeMap<String, FieldSpec>,
    default_sort: Vec<SortField>,
    default_page_size: usize,
    max_page_size: usize,
}

impl QuerySchema {
    pub fn new(default_page_size: usize, max_page_size: usize) -> Self {
        Self {
            fields: BTreeMap::new(),
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

    pub fn field_spec(&self, field: &str) -> Option<&FieldSpec> {
        self.fields.get(field)
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
