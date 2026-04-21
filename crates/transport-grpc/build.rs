fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Proto path is relative to this crate's manifest dir (crates/transport-grpc/).
    // spi was extracted to listo-ai/contracts; during local development it lives at
    // ../../listo-repos/contracts/spi relative to the workspace root, which is
    // ../../../listo-repos/contracts/spi relative to this crate dir.
    let manifest = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"),
    );
    let proto_dir = manifest
        .join("../../../../listo-repos/contracts/spi/proto")
        .canonicalize()
        .expect("spi proto dir not found — is listo-repos/contracts checked out alongside the workspace?");
    let proto = proto_dir.join("extension.proto");
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&[&proto], &[&proto_dir])?;
    println!("cargo:rerun-if-changed={}", proto.display());
    Ok(())
}
