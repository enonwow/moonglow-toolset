//! Keep the campaign's copyable inputs tied to discovery and command semantics.
use super::{Handler, Policy, table, validate};
use crate::store::Command;
use serde_json::Value;
use std::collections::BTreeSet;

fn check(doc: &str) -> Result<Vec<String>, String> {
    let operations = table(Policy { scripts: true, chunks: true, players: true });
    let lines: Vec<_> = doc.lines().collect();
    let mut names = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        let Some(marker) = line.strip_prefix("<!-- campaign-input: ") else {
            if line.starts_with("```json") {
                return Err(format!("Line {}: JSON input has no operation tag", i + 1));
            }
            i += 1;
            continue;
        };
        let name = marker
            .strip_suffix(" -->")
            .ok_or_else(|| format!("Line {}: malformed operation tag", i + 1))?;
        let at = i + 1;
        i += 1;
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        if lines.get(i).map(|l| l.trim()) != Some("```json") {
            return Err(format!("Line {at}: {name} must tag a JSON input fence"));
        }
        i += 1;
        let start = i;
        while i < lines.len() && lines[i].trim() != "```" {
            i += 1;
        }
        if i == lines.len() {
            return Err(format!("Line {at}: unterminated {name} input"));
        }
        let mut input: Value = serde_json::from_str(&lines[start..i].join("\n"))
            .map_err(|e| format!("Line {at}: {name}: {e}"))?;
        let op = operations
            .iter()
            .find(|op| op.descriptor["name"] == name)
            .ok_or_else(|| format!("Line {at}: unknown operation {name}"))?;
        validate(&op.descriptor["input"], &input)
            .map_err(|e| format!("Line {at}: {name} schema: {e}"))?;
        if let Handler::Command(kind) | Handler::Query(kind) = op.handler {
            input["op"] = kind.into();
            let command: Command = serde_json::from_value(input)
                .map_err(|e| format!("Line {at}: {name} command: {e}"))?;
            command.validate().map_err(|e| format!("Line {at}: {name} semantics: {e}"))?;
        }
        names.push(name.to_string());
        i += 1;
    }
    Ok(names)
}

#[test]
fn campaign_examples_match_discovery_and_commands() {
    let doc = include_str!("../../../docs/live-campaign.md");
    let names = check(doc).unwrap_or_else(|e| panic!("live-campaign.md: {e}"));
    // Prose can also name a tool without providing an input block.
    let catalogue = table(Policy { scripts: true, chunks: true, players: true });
    for code in doc.split('`').skip(1).step_by(2) {
        if code.starts_with("live_") && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            assert!(
                catalogue.iter().any(|op| op.descriptor["name"] == code),
                "Recipe names an unknown operation in prose: {code}"
            );
        }
    }
    let covered: BTreeSet<_> = names.iter().map(String::as_str).collect();
    for required in [
        "live_status",
        "live_area_state",
        "live_find",
        "live_events",
        "live_wait",
        "live_npc_say",
        "live_pc_message",
        "live_skill_check",
        "live_spawn",
        "live_npc_attack",
        "live_stop_combat",
        "live_npc_move",
        "live_teleport",
        "live_patrol",
        "live_stop_patrol",
        "live_set_time",
        "live_pc_give_gold",
        "live_take_gold",
        "live_pc_give_xp",
        "live_take_xp",
        "live_set_xp",
        "live_give_level",
        "live_remove_level",
        "live_view_inventory",
        "live_give_item",
        "live_take_item",
        "live_apply_vfx",
        "live_clear_vfx",
        "live_play_creature_animation",
        "live_play_animation",
        "live_run_chunk",
    ] {
        assert!(covered.contains(required), "Recipe lost its {required} example");
    }
}

#[test]
fn campaign_examples_reject_broken_tags_schemas_and_semantics() {
    let example = |name: &str, input: &str| {
        format!("<!-- campaign-input: {name} -->\n```json\n{input}\n```\n")
    };
    assert!(check("```json\n{}\n```").unwrap_err().contains("no operation tag"));
    assert!(
        check("<!-- campaign-input: live_status -->\n```json\n{}")
            .unwrap_err()
            .contains("unterminated")
    );
    assert!(
        check(&example("live_invented_scene_run", "{}")).unwrap_err().contains("unknown operation")
    );
    assert!(check(&example("live_status", r#"{"invented":true}"#)).unwrap_err().contains("schema"));
    assert!(
        check(&example(
            "live_stop_combat",
            r#"{"area":"field","target":"7","x":10,"y":10,"radius":5}"#,
        ))
        .unwrap_err()
        .contains("semantics")
    );
}
