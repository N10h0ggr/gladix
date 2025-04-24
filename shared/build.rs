fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/proto_gen")
        .compile_protos(
            &[
                "proto/events.proto",
                "proto/config.proto",
            ],
            &["proto"],
        )?;

    println!("cargo:rerun-if-changed=proto/events.proto");
    println!("cargo:rerun-if-changed=proto/config.proto");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}
