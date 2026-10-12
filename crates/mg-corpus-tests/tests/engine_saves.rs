//! A creature's saving throws as the game has them, against the ones
//! Moonglow's Statistics page shows: single classes, several classes, and
//! past level 20 (Aurora computes an epic multiclass creature's wrongly:
//! nwn-issues 816, and so did Moonglow until this measured it). The
//! creatures are the Creature Wizard's, whose epic ones have the Great
//! ability feats, which the game counts into the scores.

use std::time::Duration;

use mg_core::{ResRef, ResType};
use mg_module::blueprints::{CreatureSpec, creature};
use mg_resman::{GameInstall, ResKey};
use mg_rules::{CreatureSheet, GameData};
use mg_testkit::engine::{run_server, server_binary};
use mg_testkit::{corpus, oracle_tool, scratch_dir};

use crate::common::probe_module;

/// (fighter 4, wizard 10, rogue 8, cleric 2 of classes.2da.)
const CASES: [(&str, &[(u32, u32)]); 13] = [
    ("f10", &[(4, 10)]),
    ("f20", &[(4, 20)]),
    ("f30", &[(4, 30)]),
    ("w25", &[(10, 25)]),
    ("f10w10", &[(4, 10), (10, 10)]),
    ("f20w20", &[(4, 20), (10, 20)]),
    ("f10w10r10", &[(4, 10), (10, 10), (8, 10)]),
    ("w25c15", &[(10, 25), (2, 15)]),
    ("c15w25", &[(2, 15), (10, 25)]),
    ("w20c20", &[(10, 20), (2, 20)]),
    ("w10c30", &[(10, 10), (2, 30)]),
    ("r5f20", &[(8, 5), (4, 20)]),
    ("f5r5w15", &[(4, 5), (8, 5), (10, 15)]),
];

#[test]
fn saving_throws_match_the_engine() {
    let root = corpus!();
    if server_binary(&root).is_none() {
        eprintln!("skipped: no nwserver for this platform");
        return;
    }
    let _ = oracle_tool!("nwn_script_comp");
    let dir = scratch_dir("engine_saves");
    let game = GameData::open(&GameInstall::new(&root, None, "en")).unwrap();
    let mut extra = Vec::new();
    let mut ours = Vec::new();
    let mut probes = String::new();
    for (name, classes) in CASES {
        let resref = ResRef::from_str(&format!("mg_{name}")).unwrap();
        let spec = CreatureSpec {
            resref,
            first_name: name.to_string(),
            last_name: String::new(),
            race: 6,
            gender: 0,
            appearance: 6,
            portrait: 1,
            faction: 1,
            classes: classes.to_vec(),
            category: 0,
        };
        let utc = creature(&game, &spec, &|_| None);
        let stats = game.creature_stats(&CreatureSheet::from_gff(&utc.root));
        ours.push((name, stats.saves));
        extra.push((ResKey::new(resref, ResType::UTC), utc.to_bytes().unwrap()));
        probes += &format!(
            "    o = CreateObject(OBJECT_TYPE_CREATURE, \"mg_{name}\", l);\n    \
             WriteTimestampedLogEntry(\"MG_SAVE {name}=\" + IntToString(GetIsObjectValid(o)) + \
             \",\" + IntToString(GetFortitudeSavingThrow(o)) + \",\" + \
             IntToString(GetReflexSavingThrow(o)) + \",\" + IntToString(GetWillSavingThrow(o)));\n"
        );
    }
    let script = format!(
        "void main()\n{{\n    location l = GetStartingLocation();\n    object o;\n{probes}    \
         WriteTimestampedLogEntry(\"MG_DONE\");\n}}\n"
    );
    probe_module(&root, &dir, "mg_saves", &script, &[], &extra);
    let run = run_server(&root, &dir, "mg_saves", "MG_DONE", Duration::from_secs(120)).unwrap();
    assert!(run.finished, "{}", &run.log[run.log.len().saturating_sub(2000)..]);
    let engine: std::collections::HashMap<String, Vec<i32>> = run
        .values("MG_SAVE")
        .into_iter()
        .filter_map(|v| {
            let (name, rest) = v.split_once('=')?;
            Some((name.to_string(), rest.split(',').filter_map(|n| n.parse().ok()).collect()))
        })
        .collect();
    let mut wrong = Vec::new();
    for (name, saves) in ours {
        let theirs = &engine[name];
        assert_eq!(theirs[0], 1, "{name} was made");
        if theirs[1..] != saves {
            wrong.push(format!("{name}: ours {saves:?}, the engine's {:?}", &theirs[1..]));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
