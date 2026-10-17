//! Machine-readable command discovery, generated from the real clap tree.
use clap::{ArgAction, Command, CommandFactory};
use serde_json::{Value, json};
use std::{any::TypeId, path::PathBuf};

/// Conservative command-wide classification: optional output still counts as a write.
/// A new command must make an explicit choice here rather than silently becoming writable.
fn effects(path: &str) -> Result<(bool, bool), String> {
    match path {
        "describe" | "ls" | "which" | "cat" | "layers" | "tlk" | "verify" | "checks" | "haks"
        | "refs" | "find" | "info" | "plugin list" | "plugin check" | "nui validate"
        | "2da-check" => Ok((false, false)),
        "unpack" | "pack" | "gff" | "export" | "compile" | "live-build" | "2da-diff"
        | "tlk-diff" | "tlk-alloc" | "rename" | "dialog-export" | "dialog-import"
        | "tlk-export" | "tlk-import" | "replace" | "areas" | "update-instances" | "build"
        | "init" | "attach" | "minimap" | "model-files" | "roundtrip" | "tileset-palette"
        | "nwsync" | "import" | "set" | "apply" | "plugin run" | "plugin pack"
        | "plugin install" | "plugin remove" | "nui new" | "nui generate" => Ok((true, false)),
        "serve" | "test" | "watch" => Ok((true, true)),
        "lsp" => Ok((false, true)),
        other => Err(format!("Missing discovery effects classification for {other}")),
    }
}

fn leaf(command: &Command, path: &[String]) -> Result<Value, String> {
    let (writes, long_running) = effects(&path.join(" "))?;
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    let mut argv = Vec::new();
    for arg in command.get_arguments() {
        let name = arg.get_id().as_str();
        if matches!(name, "help" | "version" | "json") {
            continue;
        }
        let id = arg.get_value_parser().type_id();
        let is_path = id == TypeId::of::<PathBuf>();
        let integer = [
            TypeId::of::<u8>(),
            TypeId::of::<u16>(),
            TypeId::of::<u32>(),
            TypeId::of::<u64>(),
            TypeId::of::<usize>(),
            TypeId::of::<i8>(),
            TypeId::of::<i16>(),
            TypeId::of::<i32>(),
            TypeId::of::<i64>(),
            TypeId::of::<isize>(),
        ]
        .iter()
        .any(|t| id == *t);
        let kind = if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) {
            "boolean"
        } else if integer || matches!(arg.get_action(), ArgAction::Count) {
            "integer"
        } else if id == TypeId::of::<f32>() || id == TypeId::of::<f64>() {
            "number"
        } else {
            "string"
        };
        let description =
            arg.get_help().map(ToString::to_string).unwrap_or_else(|| format!("Value for {name}."));
        let mut schema = json!({"type":kind,"description":description});
        let values = arg.get_possible_values();
        if !values.is_empty() {
            schema["enum"] = json!(
                values
                    .iter()
                    .filter(|v| !v.is_hide_set())
                    .map(|v| v.get_name())
                    .collect::<Vec<_>>()
            );
        }
        let repeated = matches!(arg.get_action(), ArgAction::Append)
            || arg.get_num_args().is_some_and(|n| n.max_values() > 1);
        if repeated {
            schema = json!({"type":"array","description":description,"items":schema});
        }
        if !repeated
            && let Some(default) = arg.get_default_values().first().and_then(|v| v.to_str())
        {
            schema["default"] = if kind == "string" {
                json!(default)
            } else {
                serde_json::from_str(default).map_err(|e| e.to_string())?
            };
        }
        if arg.is_required_set() {
            required.push(name);
        }
        properties.insert(name.to_owned(), schema);
        let action = match arg.get_action() {
            ArgAction::SetTrue => "set_true",
            ArgAction::SetFalse => "set_false",
            ArgAction::Count => "count",
            _ => "value",
        };
        argv.push(json!({"input":name,"flag":arg.get_long().map(|l|format!("--{l}")).or_else(||arg.get_short().map(|s|format!("-{s}"))),
            "position":arg.get_index(),"repeat":repeated,"path":is_path,"last":arg.is_last_set(),"action":action}));
    }
    Ok(json!({"name":path.join("_").replace('-',"_"),"path":path,
        "description":command.get_about().map(ToString::to_string).unwrap_or_else(||path.join(" ")),
        "writes":writes,"longRunning":long_running,"input":{"type":"object","additionalProperties":false,"required":required,"properties":properties},"argv":argv}))
}

fn walk(command: &Command, path: &mut Vec<String>, out: &mut Vec<Value>) -> Result<(), String> {
    if command.get_subcommands().next().is_none() {
        out.push(leaf(command, path)?);
    } else {
        for child in command.get_subcommands().filter(|c| c.get_name() != "help") {
            path.push(child.get_name().to_owned());
            walk(child, path, out)?;
            path.pop();
        }
    }
    Ok(())
}
pub(crate) fn document() -> Result<Value, String> {
    let mut command = crate::Cli::command();
    command.build();
    let mut commands = Vec::new();
    walk(&command, &mut Vec::new(), &mut commands)?;
    Ok(json!({"protocol":"moonglow-cli/1","version":env!("CARGO_PKG_VERSION"),"commands":commands}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_command_is_classified_and_schemas_are_flat() {
        let document = document().unwrap();
        let commands = document["commands"].as_array().unwrap();
        assert!(commands.len() > 40);
        for c in commands {
            assert!(c["writes"].is_boolean() && c["longRunning"].is_boolean());
            assert_eq!(c["input"]["additionalProperties"], false);
            for p in c["input"]["properties"].as_object().unwrap().values() {
                assert!(p["description"].is_string());
                assert!(!p.to_string().contains("$ref"));
                assert!(!p.to_string().contains("oneOf"));
                assert!(!p.to_string().contains("anyOf"));
            }
        }
        let verify = commands.iter().find(|c| c["name"] == "verify").unwrap();
        assert!(
            verify["argv"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["input"] == "module" && a["path"] == true)
        );
        assert!(effects("new-command-without-a-decision").is_err());
    }
}
