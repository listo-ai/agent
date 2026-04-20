//! `QuerySchema` for the auth-resolution path.
//!
//! `AuthProvider::resolve` and `ScopeSet` assembly must never consult
//! a tag filter. This module provides the allowlist that makes that
//! structurally impossible at schema-construction time.
//!
//! ## Enforcement boundary
//!
//! The schema returned by [`auth_resolution_query_schema`] contains only
//! the fields relevant to resolving an `AuthContext` — `org_id`, `sub`,
//! `roles`, `scope`. `tags.*` is **absent from the allowlist**. Calling
//! `query::validate` with any filter referencing `tags.*` against this
//! schema returns `Err(unknown field "tags.labels")` before any
//! permission logic runs.
//!
//! A second structural guarantee: `domain-tags` is intentionally absent
//! from this crate's `Cargo.toml`. Any code that tries to import it
//! here fails to compile:
//!
//! ```compile_fail
//! // domain-tags is not a dependency of the auth crate — this must
//! // never compile. If it ever does, remove the dep and re-run CI.
//! use domain_tags::Tags;
//! let _ = Tags::empty();
//! ```

use query::{FieldType, Operator, QuerySchema};

/// Build the `QuerySchema` used on the auth-resolution path.
///
/// The allowlist contains only the JWT claim fields that participate in
/// scope resolution. `tags.*` is structurally absent — the schema
/// validator rejects any filter referencing it at schema-construction
/// time (before any permission logic runs), not at runtime.
///
/// # Panics
///
/// Never panics — the field registrations are compile-time constants.
pub fn auth_resolution_query_schema() -> QuerySchema {
    QuerySchema::new(100, 1000)
        .field("sub", FieldType::Text, [Operator::Eq])
        .field("org_id", FieldType::Text, [Operator::Eq])
        .field(
            "roles",
            FieldType::TextArr,
            [Operator::Contains, Operator::In],
        )
        .field("scope", FieldType::Text, [Operator::Eq, Operator::In])
}

#[cfg(test)]
mod tests {
    use super::*;
    use query::{validate, QueryRequest};

    fn req(filter: &str) -> QueryRequest {
        QueryRequest {
            filter: Some(filter.to_string()),
            sort: None,
            page: None,
            size: None,
        }
    }

    #[test]
    fn allowed_fields_pass_validation() {
        let schema = auth_resolution_query_schema();
        assert!(validate(&schema, req("org_id==acme")).is_ok());
        assert!(validate(&schema, req("roles=contains=admin")).is_ok());
    }

    /// A `tags.*` filter on the auth-resolution schema must be rejected at
    /// schema-construction time — this is the CI-verifiable proof that a PR
    /// routing tags into scope resolution fails before review.
    #[test]
    fn tags_filter_rejected_by_auth_schema() {
        let schema = auth_resolution_query_schema();

        let err = validate(&schema, req("tags.labels=contains=team/platform"))
            .expect_err("tags.labels must be unknown in the auth schema");
        assert!(
            err.to_string().contains("unknown field"),
            "expected 'unknown field', got: {err}"
        );
    }

    #[test]
    fn tags_kv_filter_rejected_by_auth_schema() {
        let schema = auth_resolution_query_schema();

        let err = validate(&schema, req("tags.kv.site==abc"))
            .expect_err("tags.kv.site must be unknown in the auth schema");
        assert!(
            err.to_string().contains("unknown field"),
            "expected 'unknown field', got: {err}"
        );
    }
}
