use serde_json::Value;
use std::process::Command;
fn schema(value: &Value) {
    let allowed = [
        "type",
        "properties",
        "required",
        "items",
        "enum",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "description",
        "default",
        "additionalProperties",
    ];
    for key in value.as_object().unwrap().keys() {
        assert!(allowed.contains(&key.as_str()), "unsupported {key}");
    }
    match value["type"].as_str().unwrap() {
        "object" => {
            assert_eq!(value["additionalProperties"], false);
            for property in value["properties"].as_object().unwrap().values() {
                assert!(property["description"].is_string());
                schema(property);
            }
        }
        "array" => schema(&value["items"]),
        "string" | "number" | "integer" | "boolean" => {}
        other => panic!("unsupported {other}"),
    }
}
#[test]
fn describe_binary_returns_portable_command_schemas() {
    let output =
        Command::new(env!("CARGO_BIN_EXE_mg")).args(["describe", "--json"]).output().unwrap();
    assert!(output.status.success());
    let document: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(document["protocol"], "moonglow-cli/1");
    let commands = document["commands"].as_array().unwrap();
    assert!(commands.len() > 40);
    for command in commands {
        schema(&command["input"]);
        assert!(command["writes"].is_boolean());
        assert!(command["longRunning"].is_boolean());
        for name in command["input"]["properties"].as_object().unwrap().keys() {
            assert!(
                command["argv"].as_array().unwrap().iter().any(|a| a["input"] == *name),
                "missing argv {name}"
            );
        }
    }
}
