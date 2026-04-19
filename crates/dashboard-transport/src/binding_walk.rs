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
