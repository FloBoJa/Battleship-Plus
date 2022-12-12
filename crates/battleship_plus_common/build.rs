use std::io::Result;

fn main() -> Result<()> {
    let resource_directory = String::from(env!("RESOURCE_DIR"));
    let proto_file = resource_directory.clone() + "/rfc/encoding/messages.proto";

    prost_build::compile_protos(&[proto_file], &[resource_directory])?;

    Ok(())
}
