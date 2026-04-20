//! The canonical `Tags` type and its JSON validator.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::normalize::{
    check_kv_count, check_label_count, dedup_labels, normalize_key, normalize_label,
    normalize_value,
};
use crate::TagsError;

/// Canonical, normalised tag set attached to a node/resource.
///
/// Persisted in the `config.tags` slot as JSON:
/// ```json
/// { "labels": ["code", "person"], "kv": { "site": "abc" } }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tags {
    /// Unique, lowercased label strings.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Lowercased keys mapped to trimmed UTF-8 values.
    #[serde(default)]
    pub kv: BTreeMap<String, String>,
}

impl Tags {
    /// Return an empty `Tags` value — the zero/absent state.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Validate and normalise a `config.tags` slot value arriving as raw JSON.
///
/// Accepts:
/// - `null` → empty tags
/// - `{}` → empty tags
/// - `{"labels": [...], "kv": {...}}`
///
/// Rejects invalid characters, reserved keys, and out-of-range lengths.
pub fn validate_tags(raw: &JsonValue) -> Result<Tags, TagsError> {
    // Treat null and absent as empty.
    if raw.is_null() {
        return Ok(Tags::empty());
    }

    let obj = raw.as_object().ok_or(TagsError::MalformedJson)?;

    let labels = parse_labels(obj.get("labels").unwrap_or(&JsonValue::Null))?;
    let kv = parse_kv(obj.get("kv").unwrap_or(&JsonValue::Null))?;

    Ok(Tags { labels, kv })
}

fn parse_labels(val: &JsonValue) -> Result<Vec<String>, TagsError> {
    if val.is_null() {
        return Ok(Vec::new());
    }
    let arr = val.as_array().ok_or(TagsError::MalformedJson)?;
    let normalised: Vec<String> = arr
        .iter()
        .map(|v| {
            let s = v.as_str().ok_or(TagsError::MalformedJson)?;
            normalize_label(s)
        })
        .collect::<Result<_, _>>()?;
    check_label_count(normalised.len())?;
    Ok(dedup_labels(normalised))
}

fn parse_kv(val: &JsonValue) -> Result<BTreeMap<String, String>, TagsError> {
    if val.is_null() {
        return Ok(BTreeMap::new());
    }
    let map = val.as_object().ok_or(TagsError::MalformedJson)?;
    check_kv_count(map.len())?;
    map.iter()
        .map(|(k, v)| {
            let key = normalize_key(k)?;
            let raw_val = v.as_str().ok_or(TagsError::MalformedJson)?;
            let value = normalize_value(&key, raw_val)?;
            Ok((key, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn null_is_empty() {
        assert_eq!(validate_tags(&JsonValue::Null).unwrap(), Tags::empty());
    }

    #[test]
    fn empty_object_is_empty() {
        assert_eq!(validate_tags(&json!({})).unwrap(), Tags::empty());
    }

    #[test]
    fn normalises_labels() {
        let tags = validate_tags(&json!({ "labels": ["Code", " person ", "CODE"] })).unwrap();
        assert_eq!(tags.labels, vec!["code", "person"]);
    }

    #[test]
    fn rejects_reserved_key() {
        let err = validate_tags(&json!({ "kv": { "sys.owner": "x" } })).unwrap_err();
        assert!(matches!(err, TagsError::ReservedKey(_)));
    }

    #[test]
    fn rejects_empty_value() {
        let err = validate_tags(&json!({ "kv": { "site": "" } })).unwrap_err();
        assert!(matches!(err, TagsError::EmptyValue(_)));
    }

    #[test]
    fn round_trips_serde() {
        let tags = Tags {
            labels: vec!["code".into(), "ops".into()],
            kv: BTreeMap::from([("site".into(), "abc".into())]),
        };
        let json = serde_json::to_value(&tags).unwrap();
        let back: Tags = serde_json::from_value(json).unwrap();
        assert_eq!(tags, back);
    }
}
