use std::env;
use std::io::{Result, Write};
use std::path::PathBuf;

use codegen::{Block, Enum, Impl, Scope, Type};
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

    // generate Enum from OpCodes
    const OP_CODES_ENUM: &'static str = "OpCode";
    let mut op_codes_scope = Scope::new();

    let mut op_codes = Enum::new(OP_CODES_ENUM);
    op_codes
        .vis("pub")
        .derive("Debug")
        .derive("Clone")
        .derive("Copy");

    let mut try_from = Impl::new(Type::new(OP_CODES_ENUM));
    try_from
        .impl_trait(Type::new("TryFrom<u8>"))
        .associate_type("Error", Type::new("&'static str"));
    let try_from_fn = try_from
        .new_fn("try_from")
        .arg("value", Type::new("u8"))
        .ret(Type::new("std::result::Result<Self, Self::Error>"));

    let mut into = Impl::new(Type::new(OP_CODES_ENUM));
    into
        .impl_trait(Type::new("Into<u8>"));
    let into_fn = into
        .new_fn("into")
        .arg_self()
        .ret(Type::new("u8"));

    let mut try_from_fn_match = Block::new("match value");
    let mut into_fn_match = Block::new("match self");

    op_codes_yaml.iter().for_each(|(key, value)| {
        let key = key.as_str().unwrap();
        let value = value.as_i64().unwrap();
        op_codes.new_variant(format!("{} = {}", key, value));

        try_from_fn_match.line(format!("{} => Ok({}::{}),", value, OP_CODES_ENUM, key));
        into_fn_match.line(format!("{}::{} => {},", OP_CODES_ENUM, key, value));
    });

    try_from_fn_match.line("_ => Err(\"Unknown OpCode\"),");

    try_from_fn.push_block(try_from_fn_match);
    into_fn.push_block(into_fn_match);

    op_codes_scope.push_enum(op_codes.clone());
    op_codes_scope.push_impl(try_from.clone());
    op_codes_scope.push_impl(into.clone());

    // write generated code
    let target: PathBuf = env::var_os("OUT_DIR")
        .expect("OUT_DIR environment variable is not set")
        .into();
    let target = target.join("battleshipplus_op_codes.rs");

    {
        let mut f = std::fs::File::create(&target)
            .expect(&format!("unable to write file {:?}.", &target));

        f.write(op_codes_scope.to_string().as_bytes())
            .expect(&format!("unable to write file {:?}.", &target));

        f.sync_all().unwrap();
    }

    Ok(())
}
