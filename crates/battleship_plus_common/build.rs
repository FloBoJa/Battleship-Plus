use std::env;
use std::fs;
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

    let messages_rust_source_path = std::env::var("OUT_DIR")
        .expect("OUT_DIR is provided for build scripts")
        + "/battleshipplus.messages.rs";

    let messages_rust_source =
        fs::read_to_string(messages_rust_source_path.clone()).expect("Could not read rust source");

    fs::write(
        messages_rust_source_path,
        format!(
            "::battleship_plus_macros::enhance!(\n\
                {messages_rust_source}\n\
             );"
        ),
    )
    .expect("Could not write modified rust source");

    Ok(())
}
