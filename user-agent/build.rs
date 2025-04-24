fn main() {
    let proto_files = &["resources/event.proto"];
    let includes = &["resources"];

    prost_build::Config::new()
        .out_dir("src/proto_gen")
        .compile_protos(proto_files, includes)
        .expect("Failed to compile proto files");
}