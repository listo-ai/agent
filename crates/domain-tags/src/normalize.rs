//! Normalization helpers shared by JSON validation and shorthand parsing.

use crate::TagsError;

const MAX_LABELS: usize = 64;
const MAX_KV: usize = 64;
const MAX_KEY_LEN: usize = 64;
const MAX_VALUE_LEN: usize = 128;

/// Characters allowed in label text and KV keys after lowercasing.
fn is_valid_identifier_char(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9' | '.' | '_' | '-' | '/')
}

/// Lowercase + trim a label and validate characters.
pub(crate) fn normalize_label(raw: &str) -> Result<String, TagsError> {
    let lower = raw.trim().to_lowercase();
    if lower.chars().any(|c| !is_valid_identifier_char(c)) {
        return Err(TagsError::InvalidLabelChars(raw.to_string()));
    }
    Ok(lower)
}

/// Lowercase + trim a key and validate characters + reserved namespace.
pub(crate) fn normalize_key(raw: &str) -> Result<String, TagsError> {
    let lower = raw.trim().to_lowercase();
    if lower.len() > MAX_KEY_LEN {
        return Err(TagsError::KeyTooLong(raw.to_string()));
    }
    if lower.chars().any(|c| !is_valid_identifier_char(c)) {
        return Err(TagsError::InvalidKeyChars(raw.to_string()));
    }
    if lower.starts_with("sys.") || lower == "sys" {
        return Err(TagsError::ReservedKey(raw.to_string()));
    }
    Ok(lower)
}

/// Trim a value and validate length / control chars.
pub(crate) fn normalize_value(key: &str, raw: &str) -> Result<String, TagsError> {
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Err(TagsError::EmptyValue(key.to_string()));
    }
    if trimmed.len() > MAX_VALUE_LEN {
        return Err(TagsError::ValueTooLong(key.to_string()));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(TagsError::ControlCharInValue(key.to_string()));
    }
    Ok(trimmed)
}

/// Deduplicate a label list preserving first-occurrence order.
pub(crate) fn dedup_labels(labels: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    labels
        .into_iter()
        .filter(|l| seen.insert(l.clone()))
        .collect()
}

pub(crate) fn check_label_count(n: usize) -> Result<(), TagsError> {
    if n > MAX_LABELS {
        return Err(TagsError::TooManyLabels(n));
    }
    Ok(())
}

pub(crate) fn check_kv_count(n: usize) -> Result<(), TagsError> {
    if n > MAX_KV {
        return Err(TagsError::TooManyKv(n));
    }
    Ok(())
}
