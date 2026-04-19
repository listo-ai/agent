fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = "../spi/proto/extension.proto";
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&[proto], &["../spi/proto"])?;
    println!("cargo:rerun-if-changed={proto}");
    Ok(())
}
