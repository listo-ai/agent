#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]
//! Generic query framework for list-style resources.
//!
//! This first slice keeps the execution backend intentionally small:
//! request params → parser → validator → in-memory executor over
//! serializable records. The AST and schema are transport-agnostic, so
//! REST/CLI/SDK surfaces can share them while persistence translation
//! catches up later.

mod ast;
mod error;
mod executor;
mod parser;
mod schema;
mod validator;

pub use ast::{FilterExpr, Operator, QueryRequest, SortDir, SortField};
pub use error::QueryError;
pub use executor::{execute, Page, PageMeta};
pub use schema::{FieldSpec, FieldType, QuerySchema};
pub use validator::{validate, ValidatedQuery};
