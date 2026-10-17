//! Parse actual private-engine look packets with the same GFF reader as the view.
use mg_core::Codepage;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn native_look_reports_parse() {
    let session = std::env::var("MGLIVE_LOOK_PROOF_SESSION").ok();
    let Some(run) = session.clone().or_else(|| std::env::var("MGLIVE_LOOK_PROOF_RUN").ok()) else {
        return;
    };
    assert!(!run.is_empty() && run.len() <= 48);
    assert!(run.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output/live-measure");
    assert_eq!(fs::read_to_string(root.join(".owner")).unwrap().trim(), "live-measure-v1");
    let dir = if session.is_some() {
        let parent = root.parent().unwrap().join("live-stress");
        assert_eq!(fs::read_to_string(parent.join(".owner")).unwrap().trim(), "live-stress-v1");
        parent.join(run)
    } else {
        root.join("runs").join(run)
    };
    let cold = dir.join("cold-reports.json").exists();
    let walking = dir.join("three_look_surrogates-reports.json").exists();
    let scale = session.is_some() || dir.join("full-scale-reports.json").exists();
    let summary: Value = if session.is_some() {
        json!({})
    } else {
        serde_json::from_slice(&fs::read(dir.join("engine.json")).unwrap()).unwrap()
    };
    let surrogate_ids: Vec<&str> = summary["pc_path_surrogates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .chain(summary["surrogate"].as_str())
        .collect();
    let reports: Vec<Value> = serde_json::from_slice(
        &fs::read(dir.join(if session.is_some() {
            "sample-reports.json"
        } else if cold {
            "cold-reports.json"
        } else if scale {
            "full-scale-reports.json"
        } else if walking {
            "three_look_surrogates-reports.json"
        } else {
            "one_look_surrogate-reports.json"
        }))
        .unwrap(),
    )
    .unwrap();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut errors = BTreeMap::<String, String>::new();
    for packet in reports {
        for area in packet["areas"].as_array().unwrap() {
            for object in area["cre"].as_array().unwrap() {
                let Some(look) = object.get("look") else { continue };
                let surrogate = surrogate_ids.contains(&object["id"].as_str().unwrap_or_default())
                    || walking
                        && matches!(
                            object["tag"].as_str(),
                            Some("C15_ACTOR" | "C15_EXTRA1" | "C15_EXTRA2")
                        );
                let surrogate = surrogate
                    || session.is_some()
                        && matches!(
                            object["tag"].as_str(),
                            Some("ST_0_0" | "ST_0_1" | "ST_1_0" | "ST_2_0" | "ST_3_0" | "ST_4_0")
                        );
                let path = if surrogate || look.get("CombatRoundData").is_some() {
                    "pc_path"
                } else {
                    "npc"
                };
                for (name, cp) in
                    [("default", Codepage::default()), ("cp1250", Codepage::WINDOWS_1250)]
                {
                    let key = format!("{path}.{name}");
                    *counts.entry(key.clone()).or_default() += 1;
                    match mg_gff::from_json(look, cp) {
                        Ok(gff) => {
                            gff.to_bytes().unwrap();
                        }
                        Err(error) => {
                            errors.entry(key).or_insert_with(|| error.to_string());
                        }
                    }
                }
            }
        }
    }
    if !cold {
        assert!(counts.get("pc_path.default").is_some_and(|n| *n > 0));
    }
    assert!(counts.get("npc.default").is_some_and(|n| *n > 0));
    let expect_invalid = std::env::var("MGLIVE_LOOK_EXPECT_INVALID").as_deref() == Ok("1");
    fs::write(dir.join("look-parse.json"), serde_json::to_vec_pretty(&json!({
        "counts": counts, "errors": errors, "expected_invalid":expect_invalid,
        "scope":if cold { "Actual private cold-start NPC snapshots; no connected-PC or rendered-client claim." } else { "Actual private NPC and uncached PC-path snapshots; no connected-PC or rendered-client claim." }
    })).unwrap()).unwrap();
    if expect_invalid {
        assert!(errors.get("npc.default").is_some_and(|e| e.contains("__data_type")));
    } else {
        assert!(errors.is_empty(), "Engine look packets must parse and serialize: {errors:?}");
        if cold || walking || scale {
            return;
        }
        let probe: Value =
            serde_json::from_slice(&fs::read(dir.join("look-text-probe.json")).unwrap()).unwrap();
        assert!(mg_gff::from_json(&probe["before"], Codepage::default()).is_err());
        for cp in [Codepage::default(), Codepage::WINDOWS_1250] {
            mg_gff::from_json(&probe["after"], cp).unwrap().to_bytes().unwrap();
        }
        assert_eq!(probe["before"]["ScriptSpawn"], probe["after"]["ScriptSpawn"]);
        assert_eq!(probe["after"]["__data_type"], "UTC ");
        assert!(probe["after"].get("DescriptionOverr").is_none());
        assert!(probe["after"]["Equip_ItemList"]["value"][0].get("DescriptionOverr").is_none());
        assert!(probe["unknown_after"]["Nested"]["value"].get("DescriptionOverr").is_some());
    }
}
