use std::env;
use std::io::{Result, Write};
use std::path::PathBuf;

use codegen::{Block, Enum, Impl, Scope, Type};
use serde_yaml::Value;

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

    // build op codes from rfc
    let op_codes_file = specification_directory + "/OpCodes.yaml";
    println!("cargo:rerun-if-changed={}", op_codes_file.as_str());
    let op_codes_yaml = serde_yaml::from_reader(
        std::fs::File::open(op_codes_file.as_str())
            .unwrap_or_else(|_| panic!("unable to open file: {}", op_codes_file.as_str())),
    )
    .unwrap_or_else(|_| {
        panic!(
            "unable to read op codes from {} file",
            op_codes_file.as_str()
        )
    });

    let op_codes_yaml = match op_codes_yaml {
        Value::Mapping(m) => m["OpCodes"]
            .as_mapping()
            .unwrap_or_else(|| panic!("unable to fine OpCodes in {}", op_codes_file.as_str()))
            .clone(),
        _ => panic!("expected a mapping named OpCodes"),
    };

    // generate Enum from OpCodes
    const OP_CODES_ENUM: &str = "OpCode";
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

    // https://rust-lang.github.io/rust-clippy/master/index.html#from_over_into
    let mut into = Impl::new(Type::new("u8"));
    into.impl_trait(Type::new(format!("From<{}>", OP_CODES_ENUM)));
    let into_fn = into
        .new_fn("from")
        .arg("value", Type::new(OP_CODES_ENUM))
        .ret(Type::new("u8"));

    let mut try_from_fn_match = Block::new("match value");
    let mut into_fn_match = Block::new("match value");

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
            .unwrap_or_else(|_| panic!("unable to write file {:?}.", &target));

        let _ = f
            .write(op_codes_scope.to_string().as_bytes())
            .unwrap_or_else(|_| panic!("unable to write file {:?}.", &target));

        f.sync_all().unwrap();
    }

    Ok(())
}
