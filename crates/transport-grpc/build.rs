//! Compiles the extension gRPC schema. The `.proto` lives **inside**
//! this crate (not in `contracts/spi`) so that when the crate is pulled
//! as a git/crates.io dependency — by `agent-sdk`, by block authors,
//! etc. — the file is guaranteed to be in the tarball. Previously we
//! reached sideways with a relative path, which only worked from the
//! local dev checkout.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"),
    );
    let proto_dir = manifest.join("proto");
    let proto = proto_dir.join("extension.proto");
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&[&proto], &[&proto_dir])?;
    println!("cargo:rerun-if-changed={}", proto.display());
    Ok(())
}
