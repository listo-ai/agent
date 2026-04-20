//! Integration tests for `WasmSupervisor` using a hand-written WAT
//! fixture compiled at test time.
//!
//! Using WAT (not the real example block) keeps CI independent of
//! the wasm toolchain — the test always runs, even without
//! `wasm32-unknown-unknown` installed. The example block under
//! `blocks/com.acme.wasm-demo/` is verified end-to-end by an
//! integration workflow when the toolchain IS available.

#![allow(clippy::unwrap_used)]

use std::io::Write;

use blocks_host::wasm::{
    DescribeResponse, KindDecl, OutputMsg, WasmError, WasmLimits, WasmSupervisor,
};
use blocks_host::BlockId;

/// Build a minimal `.wasm` fixture in a temp file and return its path.
///
/// The fixture exports `describe`, `on_input`, `alloc`, and `memory`
/// with the host's expected ABI:
/// - `describe()` returns packed `(ptr << 32) | len` of a hardcoded
///   JSON `DescribeResponse`.
/// - `on_input(_, _)` echoes back a trivial `Ok([])` JSON literal.
/// - `alloc(size)` walks a simple bump pointer kept at address 0x1000.
fn build_fixture(extension_id: &str, kind_ids: &[&str]) -> tempfile::NamedTempFile {
    // Build the two payload JSONs as raw bytes laid out in linear
    // memory at fixed addresses. WAT data segments do the layout.
    let describe_json = serde_json::to_vec(&DescribeResponse {
        extension_id: extension_id.into(),
        version: "1.2.3".into(),
        kinds: kind_ids
            .iter()
            .map(|id| KindDecl {
                kind_id: (*id).into(),
                display_name: None,
                inputs: vec!["in".into()],
                outputs: vec!["out".into()],
            })
            .collect(),
    })
    .unwrap();
    let ok_empty = serde_json::to_vec(&Ok::<Vec<OutputMsg>, String>(vec![])).unwrap();

    let describe_ptr: u32 = 0x100;
    let on_input_ptr: u32 = describe_ptr + describe_json.len() as u32 + 16;

    // Pack (ptr << 32) | len as an i64 literal for each export.
    let describe_packed: i64 = ((describe_ptr as i64) << 32) | describe_json.len() as i64;
    let on_input_packed: i64 = ((on_input_ptr as i64) << 32) | ok_empty.len() as i64;

    let describe_data = bytes_to_wat_literal(&describe_json);
    let on_input_data = bytes_to_wat_literal(&ok_empty);

    // Minimal module. `alloc` returns a bump pointer starting at
    // 0x10000 so the host can write the envelope somewhere that
    // doesn't collide with our static data segments.
    let wat = format!(
        r#"
(module
  (memory (export "memory") 2)

  ;; Static JSON payloads.
  (data (i32.const {describe_ptr}) "{describe_data}")
  (data (i32.const {on_input_ptr}) "{on_input_data}")

  ;; Bump-pointer allocator — ignores `size`, just hands out a fresh
  ;; offset each call from a cursor stored at address 0.
  (func (export "alloc") (param $size i32) (result i32)
    (local $cur i32)
    (local.set $cur (i32.load (i32.const 0)))
    (if (i32.eq (local.get $cur) (i32.const 0))
      (then (local.set $cur (i32.const 0x10000))))
    (i32.store (i32.const 0) (i32.add (local.get $cur) (local.get $size)))
    (local.get $cur))

  (func (export "describe") (result i64)
    i64.const {describe_packed})

  (func (export "on_input") (param $p i32) (param $l i32) (result i64)
    i64.const {on_input_packed})
)
"#
    );
    let wasm = wat::parse_str(&wat).expect("WAT parses");
    let mut f = tempfile::Builder::new()
        .prefix("wasm-fixture-")
        .suffix(".wasm")
        .tempfile()
        .unwrap();
    f.write_all(&wasm).unwrap();
    f.flush().unwrap();
    f
}

/// Escape bytes for a WAT `(data "…")` string literal — printable
/// ASCII passes through, everything else becomes `\xx`.
fn bytes_to_wat_literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{b:02x}")),
        }
    }
    out
}

#[test]
fn load_describes_identity_and_kinds() {
    let pid = BlockId::parse("com.acme.wasm-demo").unwrap();
    let f = build_fixture(
        "com.acme.wasm-demo",
        &["com.acme.wasm-demo.double", "com.acme.wasm-demo.add"],
    );
    let sup = WasmSupervisor::load(&pid, f.path(), WasmLimits::default()).unwrap();
    let id = sup.identity();
    assert_eq!(id.extension_id, "com.acme.wasm-demo");
    assert_eq!(id.version, "1.2.3");
    assert_eq!(id.kinds.len(), 2);
    assert_eq!(id.kinds[0].kind_id, "com.acme.wasm-demo.double");
}

#[test]
fn identity_mismatch_is_rejected() {
    let pid = BlockId::parse("com.acme.wasm-demo").unwrap();
    // Fixture claims a different id.
    let f = build_fixture("com.other.block", &[]);
    let err = WasmSupervisor::load(&pid, f.path(), WasmLimits::default()).unwrap_err();
    assert!(matches!(err, WasmError::IdentityMismatch { .. }));
}

#[test]
fn namespace_violation_is_rejected() {
    let pid = BlockId::parse("com.acme.wasm-demo").unwrap();
    // Kind id lives outside the block's namespace.
    let f = build_fixture("com.acme.wasm-demo", &["sys.core.folder"]);
    let err = WasmSupervisor::load(&pid, f.path(), WasmLimits::default()).unwrap_err();
    assert!(matches!(err, WasmError::NamespaceViolation { .. }));
}

#[test]
fn on_input_returns_empty_vec() {
    let pid = BlockId::parse("com.acme.wasm-demo").unwrap();
    let f = build_fixture("com.acme.wasm-demo", &["com.acme.wasm-demo.double"]);
    let sup = WasmSupervisor::load(&pid, f.path(), WasmLimits::default()).unwrap();
    let out = sup
        .on_input("node-1", "com.acme.wasm-demo.double", "in", "3.0")
        .unwrap();
    assert!(out.is_empty());
}

#[test]
fn missing_export_is_reported() {
    // WAT module with no `describe` export — load should fail cleanly.
    let wat = r#"(module (memory (export "memory") 1))"#;
    let wasm = wat::parse_str(wat).unwrap();
    let mut f = tempfile::Builder::new().suffix(".wasm").tempfile().unwrap();
    f.write_all(&wasm).unwrap();
    let pid = BlockId::parse("com.acme.wasm-demo").unwrap();
    let err = WasmSupervisor::load(&pid, f.path(), WasmLimits::default()).unwrap_err();
    assert!(matches!(err, WasmError::MissingExport { .. }));
}
