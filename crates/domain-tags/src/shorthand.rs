//! Parser for the human-friendly shorthand syntax used by the CLI.
//!
//! Accepted forms:
//! - Labels only:   `[code, person, things]`
//! - KV only:       `{site:abc, zone:w1}`
//! - Combined:      `[code,person]{site:abc}`
//! - Either order is legal but combined is `[labels]{kv}` by convention.

use std::collections::BTreeMap;

use crate::normalize::{
    check_kv_count, check_label_count, dedup_labels, normalize_key, normalize_label,
    normalize_value,
};
use crate::tags::Tags;
use crate::TagsError;

/// Parse a shorthand tag expression into normalised [`Tags`].
///
/// Both the `[labels]` and `{kv}` sections are optional; an empty
/// string returns [`Tags::empty()`].
pub fn parse_shorthand(input: &str) -> Result<Tags, TagsError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Tags::empty());
    }

    let (labels_src, kv_src) = split_sections(input)?;

    let labels = match labels_src {
        Some(s) => parse_label_section(s)?,
        None => Vec::new(),
    };
    let kv = match kv_src {
        Some(s) => parse_kv_section(s)?,
        None => BTreeMap::new(),
    };

    Ok(Tags { labels, kv })
}

/// Split input into (`[...]` content, `{...}` content).
fn split_sections(input: &str) -> Result<(Option<&str>, Option<&str>), TagsError> {
    let mut labels_src: Option<&str> = None;
    let mut kv_src: Option<&str> = None;
    let mut rest = input;

    while !rest.is_empty() {
        if rest.starts_with('[') {
            let end = rest.find(']').ok_or_else(|| {
                TagsError::MalformedShorthand("unclosed `[`".to_string())
            })?;
            labels_src = Some(&rest[1..end]);
            rest = rest[end + 1..].trim_start();
        } else if rest.starts_with('{') {
            let end = rest.find('}').ok_or_else(|| {
                TagsError::MalformedShorthand("unclosed `{`".to_string())
            })?;
            kv_src = Some(&rest[1..end]);
            rest = rest[end + 1..].trim_start();
        } else {
            return Err(TagsError::MalformedShorthand(format!(
                "unexpected character `{}`",
                rest.chars().next().unwrap_or('?')
            )));
        }
    }

    Ok((labels_src, kv_src))
}

fn parse_label_section(src: &str) -> Result<Vec<String>, TagsError> {
    let normalised: Vec<String> = src
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(normalize_label)
        .collect::<Result<_, _>>()?;
    check_label_count(normalised.len())?;
    Ok(dedup_labels(normalised))
}

fn parse_kv_section(src: &str) -> Result<BTreeMap<String, String>, TagsError> {
    // Tokenise pairs, respecting quoted values.
    let pairs = split_kv_pairs(src)?;
    check_kv_count(pairs.len())?;
    pairs
        .into_iter()
        .map(|(k, v)| {
            let key = normalize_key(k.trim())?;
            let raw_val = strip_quotes(v.trim());
            let value = normalize_value(&key, raw_val)?;
            Ok((key, value))
        })
        .collect()
}

/// Split `key:val, key2:"val,with,commas"` into `(key, val)` pairs.
fn split_kv_pairs(src: &str) -> Result<Vec<(&str, &str)>, TagsError> {
    let mut pairs = Vec::new();
    let mut remaining = src;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }
        // Find `:` to separate key from value.
        let colon = remaining.find(':').ok_or_else(|| {
            TagsError::MalformedShorthand(format!("missing `:` in kv pair near `{remaining}`"))
        })?;
        let key = &remaining[..colon];
        let after_colon = &remaining[colon + 1..];

        let (val, consumed) = if after_colon.starts_with('"') {
            // Quoted value — find closing quote (handles `\"`).
            extract_quoted_value(after_colon)?
        } else {
            // Unquoted — ends at next `,` or end of string.
            match after_colon.find(',') {
                Some(pos) => (&after_colon[..pos], pos + 1),
                None => (after_colon, after_colon.len()),
            }
        };

        pairs.push((key, val));
        remaining = &after_colon[consumed..];
        remaining = remaining.trim_start_matches(',').trim_start();
    }

    Ok(pairs)
}

fn extract_quoted_value(s: &str) -> Result<(&str, usize), TagsError> {
    // s starts with `"`.
    let mut chars = s.char_indices().skip(1); // skip opening quote
    let mut escaped = false;
    for (i, c) in chars.by_ref() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            // i is the index of the closing quote.
            // consumed = closing quote + optional comma.
            let after = &s[i + 1..];
            let consumed = i + 1 + if after.starts_with(',') { 1 } else { 0 };
            return Ok((&s[1..i], consumed));
        }
    }
    Err(TagsError::MalformedShorthand(
        "unclosed quoted value".to_string(),
    ))
}

fn strip_quotes(s: &str) -> &str {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string() {
        assert_eq!(parse_shorthand("").unwrap(), Tags::empty());
    }

    #[test]
    fn labels_only() {
        let t = parse_shorthand("[code, person, things]").unwrap();
        assert_eq!(t.labels, vec!["code", "person", "things"]);
        assert!(t.kv.is_empty());
    }

    #[test]
    fn kv_only() {
        let t = parse_shorthand("{site:abc, zone:w1}").unwrap();
        assert_eq!(t.kv["site"], "abc");
        assert_eq!(t.kv["zone"], "w1");
        assert!(t.labels.is_empty());
    }

    #[test]
    fn combined() {
        let t = parse_shorthand("[code,person]{site:abc}").unwrap();
        assert_eq!(t.labels, vec!["code", "person"]);
        assert_eq!(t.kv["site"], "abc");
    }

    #[test]
    fn quoted_value_with_comma() {
        let t = parse_shorthand(r#"{note:"hello, world"}"#).unwrap();
        assert_eq!(t.kv["note"], "hello, world");
    }

    #[test]
    fn deduplicates_labels() {
        let t = parse_shorthand("[code, CODE, Code]").unwrap();
        assert_eq!(t.labels, vec!["code"]);
    }

    #[test]
    fn rejects_reserved_key() {
        let err = parse_shorthand("{sys.owner:me}").unwrap_err();
        assert!(matches!(err, TagsError::ReservedKey(_)));
    }

    #[test]
    fn unclosed_bracket_error() {
        assert!(parse_shorthand("[code").is_err());
    }
}
