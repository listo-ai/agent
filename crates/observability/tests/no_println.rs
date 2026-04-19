#![allow(clippy::unwrap_used, clippy::panic)]
//! CI lint: no `println!` / `eprintln!` in library code.
//!
//! Per `docs/design/LOGGING.md` § "Shared rules", library code must
//! go through the canonical logger. This test scans the workspace's
//! `crates/` tree for offending macros.
//!
//! # Exemptions
//!
//! - Any file under `tests/` (integration tests may print for
//!   debugging; they are not library code).
//! - Any file under `src/bin/` or a crate's `examples/`.
//! - Files inside a `#[cfg(test)]` unit-test module would normally be
//!   exempt, but the scan is file-level; authors keep test-only
//!   macros inside `#[cfg(test)] mod tests { ... }` and this test
//!   does not flag them because the *file* may still contain
//!   non-test code. We treat unit-test modules as library code: no
//!   `println!` there either. Tests that need to print output should
//!   live in `tests/`.
//! - `apps/agent/src/main.rs` is allowed to use `eprintln!` only
//!   before [`observability::init`] returns — i.e. the
//!   pre-subscriber bootstrap error path.
//! - CLI interactive paths (to be added under
//!   `crates/transport-cli/` in a later stage) are exempt via the
//!   `// NO_PRINTLN_LINT:allow` marker on the same line.

use std::fs;
use std::path::{Path, PathBuf};

fn crates_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is the observability crate; walk up one.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().expect("crate has parent").to_path_buf()
}

fn is_exempt_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/tests/") || s.contains("/benches/") || s.contains("/examples/") {
        return true;
    }
    if s.ends_with("/apps/agent/src/main.rs") {
        return true;
    }
    // Cargo build scripts *must* use `println!("cargo:...")` to emit
    // directives — the print macro is the protocol. Exempt.
    if s.ends_with("/build.rs") {
        return true;
    }
    // Skip target/ if it somehow lives under crates/ (it shouldn't).
    if s.contains("/target/") {
        return true;
    }
    false
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_dir() {
            walk(&path, out);
        } else if ft.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn library_code_does_not_use_println_or_eprintln() {
    let root = crates_root();
    let mut files = Vec::new();
    walk(&root, &mut files);

    let mut offenders: Vec<String> = Vec::new();
    for path in files {
        if is_exempt_path(&path) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (lineno, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            // Skip commented-out lines and docs.
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("//!")
            {
                continue;
            }
            if line.contains("NO_PRINTLN_LINT:allow") {
                continue;
            }
            if line.contains("println!") || line.contains("eprintln!") || line.contains("print!") {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.strip_prefix(crates_root()).unwrap_or(&path).display(),
                    lineno + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "library code must not use println!/eprintln!/print! — route through observability::prelude:\n{}",
        offenders.join("\n")
    );
}
