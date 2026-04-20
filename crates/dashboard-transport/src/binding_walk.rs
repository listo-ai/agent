//! Shared tree-walking helpers for `{{...}}` binding expressions.
//!
//! Both render-time substitution and resolve-time dry-run validation
//! need to find every string field in a component tree that carries a
//! `{{...}}` expression. Centralising the walker lets the two paths
//! stay in lockstep.

use serde_json::Value as JsonValue;

/// Visit every string leaf in a JSON value. `path` carries a dot/bracket
/// path to the current leaf (e.g. `root.children[0].label`).
pub fn walk_string_leaves(v: &JsonValue, path: &str, f: &mut dyn FnMut(&str, &str)) {
    match v {
        JsonValue::String(s) => f(path, s),
        JsonValue::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                let p = format!("{path}[{i}]");
                walk_string_leaves(item, &p, f);
            }
        }
        JsonValue::Object(map) => {
            // If the object has an `id` field, use it to keep locations
            // human-readable (e.g. `root.widget-5.label` vs `root[3].label`).
            for (k, v) in map.iter() {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk_string_leaves(v, &p, f);
            }
        }
        _ => {}
    }
}

/// Substitute every `{{binding}}` occurrence in `s` using `eval`.
/// Whole-string expressions are replaced with the raw JSON value
/// rendering (preserving numeric/boolean types as their JSON form);
/// interpolated expressions are stringified. On evaluation failure
/// the original `{{...}}` text is left in place so RSQL downstream
/// sees it as a literal — authors get an empty-result row rather
/// than a hard error, and the builder's dry-run already flagged it.
pub fn substitute_bindings<F>(s: &str, mut eval: F) -> String
where
    F: FnMut(&str) -> Option<JsonValue>,
{
    // Whole-string fast path — preserves the JSON type (number, bool,
    // object) rather than forcing everything through .to_string().
    let trimmed = s.trim();
    if let Some(inner) = trimmed
        .strip_prefix("{{")
        .and_then(|r| r.strip_suffix("}}"))
    {
        if !inner.contains("{{") && !inner.contains("}}") {
            if let Some(v) = eval(inner.trim()) {
                return match v {
                    JsonValue::String(x) => x,
                    other => other.to_string(),
                };
            }
            return s.to_string();
        }
    }
    // Embedded — every `{{...}}` becomes its stringified form.
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = s[i + 2..].find("}}") {
                let expr = s[i + 2..i + 2 + end].trim();
                let rendered = eval(expr)
                    .map(|v| match v {
                        JsonValue::String(x) => x,
                        other => other.to_string(),
                    })
                    .unwrap_or_else(|| format!("{{{{{expr}}}}}"));
                out.push_str(&rendered);
                i += 2 + end + 2;
                continue;
            }
        }
        out.push(s.as_bytes()[i] as char);
        i += 1;
    }
    out
}

/// Iterate `{{...}}` expressions in `s` — yields the trimmed inner text
/// of every well-formed binding. Malformed `{{` with no closing `}}` is
/// reported by `on_unterminated` so dry-run callers can flag it.
pub fn for_each_binding_expr(
    s: &str,
    on_expr: &mut dyn FnMut(&str),
    on_unterminated: &mut dyn FnMut(),
) {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            match s[i + 2..].find("}}") {
                Some(end) => {
                    let expr = s[i + 2..i + 2 + end].trim();
                    on_expr(expr);
                    i += 2 + end + 2;
                }
                None => {
                    on_unterminated();
                    return;
                }
            }
        } else {
            i += 1;
        }
    }
}
