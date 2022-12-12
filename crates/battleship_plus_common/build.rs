use std::env;
use std::io::{Result, Write};
use std::path::PathBuf;

use codegen::Scope;
use serde_yaml::Value;

fn main() -> Result<()> {
    let resource_directory = String::from(env!("RESOURCE_DIR"));
    let proto_file = resource_directory.clone() + "/rfc/encoding/messages.proto";

    // build protobuf structs from rfc
    prost_build::compile_protos(&[proto_file.as_str()], &[resource_directory.as_str()])?;

    // build op codes from rfc
    let op_codes_file = resource_directory.clone() + "/rfc/encoding/OpCodes.yaml";
    let op_codes_yaml =
        serde_yaml::from_reader(std::fs::File::open(op_codes_file.as_str())
            .expect(&format!("unable to open file: {}", op_codes_file.as_str())))
            .expect(&format!("unable to read op codes from {} file", op_codes_file.as_str()));

    let op_codes_yaml = match op_codes_yaml {
        Value::Mapping(m) => {
            m["OpCodes"].as_mapping().expect(&format!("unable to fine OpCodes in {}", op_codes_file.as_str())).clone()
        }
        _ => panic!("expected a mapping named OpCodes")
    };

    let mut op_codes = Scope::new();
    {
        let op_codes = op_codes
            .new_enum("OpCodes")
            .vis("pub")
            .derive("Debug")
            .derive("Clone")
            .derive("Copy");

        op_codes_yaml.iter().for_each(|(key, value)| {
            let key = key.as_str().unwrap();
            let value = value.as_i64().unwrap();
            op_codes.new_variant(format!("{} = {:x}", key, value));
        });
    }

    let target: PathBuf = env::var_os("OUT_DIR")
        .expect("OUT_DIR environment variable is not set")
        .into();
    let target = target.join("battleshipplus_op_codes.rs");

    {
        let mut f = std::fs::File::create(&target)
            .expect(&format!("unable to write file {:?}.", &target));

        f.write(op_codes.to_string().as_bytes())
            .expect(&format!("unable to write file {:?}.", &target));

        f.sync_all().unwrap();
    }

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
