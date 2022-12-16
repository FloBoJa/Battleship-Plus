use serde_yaml::Value;

use crate::messages::OpCode;

#[test]
fn op_code_into_test() {
    let resource_directory = String::from(env!("RESOURCE_DIR"));
    let op_codes_file = resource_directory + "/rfc/encoding/OpCodes.yaml";
    // load op codes
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

    let op_codes = match op_codes_yaml {
        Value::Mapping(m) => m["OpCodes"]
            .as_mapping()
            .unwrap_or_else(|| panic!("unable to fine OpCodes in {}", op_codes_file.as_str()))
            .clone(),
        _ => panic!("expected a mapping named OpCodes"),
    };

    let mut valid_values = Vec::with_capacity(op_codes.len());

    // match op codes from rfc to generated enum
    op_codes.iter().for_each(|(key, value)| {
        let key = key.as_str().unwrap();
        let value = value.as_i64().unwrap() as u8;
        valid_values.push(value);

        // u8 -> OpCode
        let op_code = OpCode::try_from(value);
        assert!(op_code.is_ok());
        let op_code = op_code.unwrap();
        assert_eq!(key, format!("{:?}", op_code));

        // OpCode -> u8
        assert_eq!(value, op_code.into());
    });

    // assert that try_from fails with an error for an invalid u8
    for i in u8::MIN..u8::MAX {
        if valid_values.contains(&i) {
            continue;
        }

        let op_code = OpCode::try_from(i);
        assert!(op_code.is_err());
        assert_eq!(op_code.unwrap_err(), "Unknown OpCode")
    }
}
