//! Explicit test-only fixture preparation. Never starts a server or builds apps.
#[path = "support/dialogue_capture.rs"]
mod dialogue_capture;
#[path = "support/stress_fixture.rs"]
mod stress_fixture;
use mg_core::{Codepage, ResType};
use mg_resman::{GameInstall, ResMan};
use serde_json::json;
use std::{fs, path::PathBuf};

#[test]
fn prepare_private_events_fixture() {
    if std::env::var("MGLIVE_PREPARE_EVENTS").as_deref() != Ok("1") {
        return;
    }
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-events")
        .canonicalize()
        .unwrap();
    assert_eq!(
        fs::read_to_string(out.join(".mglive-events-fixture")).unwrap().trim(),
        "moonglow-live-events-v1"
    );
    let dir = out.join("module");
    assert!(!dir.symlink_metadata().unwrap().file_type().is_symlink());
    for name in [
        "module.ifo",
        "events.are",
        "events.git",
        "events.gic",
        "events2.are",
        "events2.git",
        "events2.gic",
        "repute.fac",
        "mget_chest.utp",
        "mget_door.utd",
        "mget_polish.dlg",
        "mget_focus.dlg",
        "mget_polish.utp",
        "mget_waypoint.utw",
        "mget_encounter.ute",
        "mget_store.utm",
        "mget_trigger.utt",
    ] {
        let value =
            serde_json::from_slice(&fs::read(out.join(format!("{name}.json"))).unwrap()).unwrap();
        let cp =
            if name == "mget_polish.utp" { Codepage::WINDOWS_1250 } else { Codepage::default() };
        let gff = mg_gff::from_json(&value, cp).unwrap();
        fs::write(dir.join(name), gff.to_bytes().unwrap()).unwrap();
    }
    let root = PathBuf::from(
        std::env::var_os("MGLIVE_RESOURCE_ROOT").expect("explicit read-only game root"),
    );
    let install = GameInstall::new(root, Some(out.join("user")), "en");
    let resman = ResMan::for_game(&install).unwrap();
    // Keep just the installed native declarations used by the operation proofs.
    let native = resman.get(&mg_resman::ResKey::parse("nwscript", ResType::NSS).unwrap()).unwrap();
    let text = String::from_utf8_lossy(&native);
    let lines: Vec<_> = text.lines().collect();
    let mut selected = std::collections::BTreeSet::new();
    for (i, line) in lines.iter().enumerate() {
        if [
            "ReplaceObjectAnimation",
            "JsonString(",
            "JsonDump(",
            "JsonParse(",
            "GetStringByStrRef(",
            "GetSkillRank(",
            "GetStringByte(",
            "PlayAnimation(",
            "PlayCustomAnimation(",
            "ANIMATION_",
            "GetPortraitResRef(",
            "GetPortraitId(",
            "TakeGoldFromCreature(",
            "SetXP(",
            "GetHitDice(",
            "ActionEquipItem(",
            "ActionUnequipItem(",
            "ActionPutDownItem(",
            "CopyObject(",
            "EffectVisualEffect(",
            "GetEffectFloat(",
            "ActionUseItem(",
            "Talent",
            "FactionReputation(",
            "AdjustReputation(",
            "GetItemPropertyUses",
            "GetEffectVector(",
            "ActionGiveItem(",
        ]
        .iter()
        .any(|name| line.contains(name))
        {
            for n in i.saturating_sub(9)..=(i + 2).min(lines.len() - 1) {
                selected.insert(n);
            }
        }
    }
    fs::write(
        out.join("native-script-api.txt"),
        selected.into_iter().map(|i| lines[i]).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();

    fs::write(
        out.join("native-inventory-api.txt"),
        lines
            .iter()
            .filter(|line| {
                ["Inventory", "Repos", "ItemPosition", "Droppable", "ItemAppearance", "Cursed"]
                    .iter()
                    .any(|key| line.contains(key))
            })
            .copied()
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();

    let nui = resman.get(&mg_resman::ResKey::parse("nw_inc_nui", ResType::NSS).unwrap()).unwrap();
    let nui = String::from_utf8_lossy(&nui);
    fs::write(
        out.join("native-nui-api.txt"),
        nui.lines()
            .filter(|l| {
                [
                    "NuiTextEdit(",
                    "NuiEnabled(",
                    "NuiBind(",
                    "NuiSetBind(",
                    "NuiLabel(",
                    "NuiColor(",
                    "NuiStyleForegroundColor(",
                    "NuiVisible(",
                ]
                .iter()
                .any(|name| l.contains(name))
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();

    let mut module = mg_module::Module::open(&dir).unwrap();
    if std::env::var("MGLIVE_CAPTURE_PROOF").as_deref() == Ok("1") {
        dialogue_capture::prepare(&mut module, &out);
    }
    if std::env::var("MGLIVE_STRESS_PREPARE").as_deref() == Ok("1") {
        stress_fixture::populate(&mut module, &resman);
    }
    let scripts: Vec<_> = module.keys_of(ResType::NSS).copied().collect();
    let mut errors = vec![];
    // Compile one script per call: only one compiler worker at a time.
    for script in &scripts {
        for result in
            mg_module::build::compile_named_scripts(&mut module, &resman, &[*script], None)
        {
            if let Err(error) = result.result {
                errors.push(format!("{}: {}", result.script, error));
            }
        }
    }
    let compiled: Vec<_> = module.keys_of(ResType::NCS).copied().collect();
    let result = json!({"failed":errors.len(),"errors":errors,"compiled":compiled.len(),"compiler_binary":std::env::current_exe().unwrap()});
    fs::write(out.join("compile.json"), serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    assert!(errors.is_empty(), "{}", result);
    for key in compiled {
        fs::write(dir.join(key.to_string()), module.get(&key).unwrap()).unwrap();
    }
    fs::write(out.join("events.mod"), module.to_archive_bytes().unwrap()).unwrap();
}
