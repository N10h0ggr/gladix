fn main() {
    let proto_files = &["proto/events.proto"];
    let includes = &["proto"];

    prost_build::Config::new()
        .out_dir("src/proto_gen")
        .compile_protos(proto_files, includes)
        .expect("Failed to compile proto files");
}