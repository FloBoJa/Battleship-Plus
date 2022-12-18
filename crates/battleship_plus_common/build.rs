use std::env;
use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");

    let specification_directory = String::from(env!("RESOURCE_DIR")) + "/rfc/encoding";
    let proto_file_messages = specification_directory.clone() + "/messages.proto";
    let proto_file_types = specification_directory.clone() + "/datatypes.proto";
    println!("cargo:rerun-if-changed={}", proto_file_messages.as_str());
    println!("cargo:rerun-if-changed={}", proto_file_types.as_str());

    // build protobuf structs from rfc
    prost_build::compile_protos(
        &[proto_file_messages.as_str()],
        &[specification_directory.as_str()],
    )?;

    Ok(())
}
