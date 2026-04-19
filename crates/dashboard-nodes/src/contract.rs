//! Parameter-contract validation.
//!
//! A `ui.template.requires` slot declares named parameter holes; a
//! `ui.page.bound_args` slot must satisfy that contract. Validation runs
//! at page-save time and on template-version bumps (see DASHBOARD.md §
//! "Template versioning & migration").
//!
//! This module is deliberately a pure function over JSON — no graph
//! lookups, no I/O. Callers pass the already-loaded template `requires`
//! and page `bound_args` values. That keeps it trivially testable and
//! lets the resolver (dashboard-runtime) reuse it without circular
//! dependencies.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

/// Declared parameter type for a single template hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    /// `NodeRef` — an object `{ "id": "<node-id>" }`.
    Ref,
    String,
    Number,
    Bool,
}

/// One hole in a template's `requires` contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    #[serde(rename = "type")]
    pub ty: ParamType,
    /// Required by default. An optional hole may be omitted from
    /// `bound_args`; if omitted and `default` is set, the default is
    /// treated as the bound value.
    #[serde(default = "default_required")]
    pub required: bool,
    #[serde(default)]
    pub default: Option<JsonValue>,
}

fn default_required() -> bool {
    true
}

/// The parsed `ui.template.requires` contract.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Requires(pub BTreeMap<String, ParamSpec>);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("missing required hole `{0}`")]
    MissingRequired(String),
    #[error("unknown hole `{0}` in bound_args (template declares no such parameter)")]
    UnknownHole(String),
    #[error("hole `{hole}` expects {expected:?} but got {actual}")]
    TypeMismatch {
        hole: String,
        expected: ParamType,
        actual: String,
    },
    #[error("template `requires` is malformed: {0}")]
    MalformedRequires(String),
    #[error("page `bound_args` is malformed: expected a JSON object")]
    MalformedBoundArgs,
}

/// Validate `bound_args` against a template's `requires` contract.
///
/// Returns every error found (not just the first) so authoring UIs can
/// surface all problems at once. Empty vec == valid.
pub fn validate_bound_args(
    requires: &JsonValue,
    bound_args: &JsonValue,
) -> Result<Vec<ContractError>, ContractError> {
    let requires: Requires = if requires.is_null() {
        Requires::default()
    } else {
        serde_json::from_value(requires.clone())
            .map_err(|e| ContractError::MalformedRequires(e.to_string()))?
    };

    let args = bound_args
        .as_object()
        .ok_or(ContractError::MalformedBoundArgs)?;

    let mut errors = Vec::new();

    for (hole, spec) in &requires.0 {
        match args.get(hole) {
            Some(value) => {
                if let Some(err) = check_type(hole, spec.ty, value) {
                    errors.push(err);
                }
            }
            None => {
                if spec.required && spec.default.is_none() {
                    errors.push(ContractError::MissingRequired(hole.clone()));
                }
            }
        }
    }

    for name in args.keys() {
        if !requires.0.contains_key(name) {
            errors.push(ContractError::UnknownHole(name.clone()));
        }
    }

    Ok(errors)
}

fn check_type(hole: &str, expected: ParamType, value: &JsonValue) -> Option<ContractError> {
    let ok = match expected {
        ParamType::Ref => value
            .as_object()
            .and_then(|m| m.get("id"))
            .map(|v| v.is_string())
            .unwrap_or(false),
        ParamType::String => value.is_string(),
        ParamType::Number => value.is_number(),
        ParamType::Bool => value.is_boolean(),
    };
    if ok {
        return None;
    }
    Some(ContractError::TypeMismatch {
        hole: hole.to_string(),
        expected,
        actual: describe(value),
    })
}

fn describe(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => "null".into(),
        JsonValue::Bool(_) => "bool".into(),
        JsonValue::Number(_) => "number".into(),
        JsonValue::String(_) => "string".into(),
        JsonValue::Array(_) => "array".into(),
        JsonValue::Object(_) => "object".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn requires_two_holes() -> JsonValue {
        json!({
            "target": { "type": "ref" },
            "showHeader": { "type": "bool", "required": false, "default": true },
        })
    }

    #[test]
    fn happy_path_ref_and_optional_bool() {
        let errors = validate_bound_args(
            &requires_two_holes(),
            &json!({
                "target": { "id": "abc-123" },
                "showHeader": false,
            }),
        )
        .unwrap();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn optional_hole_may_be_omitted_when_default_present() {
        let errors =
            validate_bound_args(&requires_two_holes(), &json!({ "target": { "id": "abc" } }))
                .unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn missing_required_hole_reports_error() {
        let errors = validate_bound_args(&requires_two_holes(), &json!({})).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ContractError::MissingRequired(ref h) if h == "target"));
    }

    #[test]
    fn type_mismatch_reports_precise_error() {
        let errors =
            validate_bound_args(&requires_two_holes(), &json!({ "target": "not-a-ref" })).unwrap();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ContractError::TypeMismatch {
                hole,
                expected,
                actual,
            } => {
                assert_eq!(hole, "target");
                assert_eq!(*expected, ParamType::Ref);
                assert_eq!(actual, "string");
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn unknown_hole_reports_error() {
        let errors = validate_bound_args(
            &requires_two_holes(),
            &json!({
                "target": { "id": "x" },
                "stray": 1,
            }),
        )
        .unwrap();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ContractError::UnknownHole(ref h) if h == "stray"));
    }

    #[test]
    fn multiple_errors_all_reported() {
        let errors = validate_bound_args(
            &requires_two_holes(),
            &json!({
                "showHeader": 3,
                "stray": "x",
            }),
        )
        .unwrap();
        assert_eq!(errors.len(), 3, "got {errors:?}");
    }

    #[test]
    fn empty_requires_accepts_empty_args() {
        let errors = validate_bound_args(&json!({}), &json!({})).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn null_requires_accepts_empty_args() {
        let errors = validate_bound_args(&JsonValue::Null, &json!({})).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn malformed_bound_args_rejected() {
        let err = validate_bound_args(&json!({}), &json!("oops")).unwrap_err();
        assert_eq!(err, ContractError::MalformedBoundArgs);
    }

    #[test]
    fn malformed_requires_rejected() {
        let err = validate_bound_args(&json!({ "target": { "type": "unknown-type" } }), &json!({}))
            .unwrap_err();
        assert!(matches!(err, ContractError::MalformedRequires(_)));
    }
}
