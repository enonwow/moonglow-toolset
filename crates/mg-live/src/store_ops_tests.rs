use super::*;

fn store(players: bool) -> LiveStore {
    let store = LiveStore::default();
    store.set_policy(Policy { players, ..Policy::default() });
    store
        .sync(
            json!({"v":1,"boot":"ops","seq":1,"areas":[
        {"area":"events","full":true,"cre":[
            {"id":"1","name":"NPC","gold":10,"xp":20},
            {"id":"2","pc":true},{"id":"3","dead":true}],
            "plc":[{"id":"4"}],"doors":[{"id":"5","lock":false}]},
        {"area":"other","cre":[{"id":"6"}]}]})
            .to_string()
            .as_bytes(),
            1,
        )
        .unwrap();
    store
}

fn command(op: Value) -> Command {
    let mut value = json!({"id":"test","area":"events"});
    value.as_object_mut().unwrap().extend(op.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

/// Explicit defaults in these rows make the expected game payload unambiguous.
fn cases() -> Vec<Value> {
    vec![
        json!({"op":"give_item","target":"1","resref":"nw_it_mring001"}),
        json!({"op":"equip_item","target":"1","item":"abc","slot":7}),
        json!({"op":"unequip_item","target":"1","item":"abc","slot":7}),
        json!({"op":"drop_item","target":"1","item":"abc"}),
        json!({"op":"use_item","target":"1","item":"abc","property":0}),
        json!({"op":"transfer_item","target":"1","item":"abc","recipient":"2"}),
        json!({"op":"play_creature_animation","target":"1","animation":105,"loop":false,"speed":1.0,"seconds":30.0}),
        json!({"op":"play_creature_animation","target":"1","animation":"dance","loop":true,"speed":0.5,"seconds":3.0}),
        json!({"op":"take_gold","target":"1","amount":10}),
        json!({"op":"set_xp","target":"1","value":0}),
        json!({"op":"take_xp","target":"1","amount":100000}),
        json!({"op":"remove_level","target":"1"}),
        json!({"op":"replace_object_animation","target":"1","old":"walk","new":"run","reset":false}),
        json!({"op":"replace_object_animation","target":"1","old":"walk","reset":true}),
        json!({"op":"set_tile","x":1,"y":1,"tile_id":51,"orientation":1,"height":1}),
        json!({"op":"patrol","target":"1","points":[{"x":1.0,"y":2.0,"z":0.0}],"loop":true,"run":false}),
        json!({"op":"stop_patrol","target":"1"}),
        json!({"op":"skill_check","x":1.0,"y":2.0,"radius":30.0,"skill":0,"dc":15,"bonus":0,"show_dc":true,"timeout":120,"success_text":"Passed","fail_text":""}),
        json!({"op":"skill_check","target":"2","skill":0,"dc":15,"bonus":0,"show_dc":true,"timeout":120,"success_text":"","fail_text":""}),
        json!({"op":"capture_object","target":"1"}),
        json!({"op":"spawn_json","json":{"__data_type":"UTC "},"x":1.0,"y":2.0,"z":0.0,"f":90.0}),
        json!({"op":"set_appearance","target":"1","row":0}),
        json!({"op":"set_lootable","target":"1","lootable":false,"no_drop":true}),
        json!({"op":"set_static","target":"4","static":false}),
        json!({"op":"set_appearance","target":"4","row":5}),
        json!({"op":"apply_vfx","target":"1","row":411,"duration":"temporary","seconds":3.0}),
        json!({"op":"apply_vfx","location":{"x":1.0,"y":2.0,"z":0.0},"row":9,"duration":"instant"}),
        json!({"op":"clear_vfx","target":"1","row":411}),
        json!({"op":"npc_attack","target":"1","victim":"2","faction":5,"hold":false,"timeout":30}),
        json!({"op":"give_level","target":"2"}),
        json!({"op":"give_alignment","target":"2","law_chaos":10,"good_evil":-20}),
        json!({"op":"set_stat","target":"2","ability":"strength","value":19}),
        json!({"op":"get_faction_reputation","target":"2","faction":"merchant"}),
        json!({"op":"set_faction_reputation","target":"2","faction":5,"reputation":75}),
        json!({"op":"disable_trap","target":"4"}),
        json!({"op":"spawn_trap_on_object","target":"5","trap":0}),
        json!({"op":"set_time","hour":13,"minute":17}),
        json!({"op":"set_date","year":1400,"month":1,"day":2}),
        json!({"op":"jump_all_players","x":12.0,"y":8.0,"z":0.0}),
        json!({"op":"view_inventory","target":"1"}),
        json!({"op":"take_item","target":"1","item":"abc"}),
        json!({"op":"take_item","target":"4","resref":"nw_it_mpotion001"}),
        json!({"op":"get_variable","target":"module","type":"int","name":"quest"}),
        json!({"op":"set_variable","target":"area","type":"float","name":"quest","value":1.25}),
        json!({"op":"dump_locals","target":"1"}),
        json!({"op":"kill","target":"1"}),
        json!({"op":"heal","target":"1","amount":100000}),
        json!({"op":"set_invulnerable","target":"1","enabled":true}),
        json!({"op":"set_immortal","target":"1","enabled":false}),
        json!({"op":"set_ai","target":"1","level":"very_high"}),
        json!({"op":"force_rest","target":"1"}),
        json!({"op":"limbo","target":"1"}),
        json!({"op":"unlimbo","target":"7ff","x":12.0,"y":8.0,"z":0.0}),
        json!({"op":"spawn","kind":"creature","r":"nw_bartender","x":1.0,"y":2.0,"z":0.0,"f":90.0,"count":3,"faction":"merchant"}),
        json!({"op":"npc_say","target":"1","text":"Zażółć gęślą jaźń","volume":"whisper"}),
        json!({"op":"stop_combat","x":1.0,"y":2.0,"radius":50.0}),
        json!({"op":"stop_combat","target":"1"}),
        json!({"op":"set_faction","target":"1","faction":2}),
        json!({"op":"freeze","target":"1","seconds":300.0}),
        json!({"op":"unfreeze","target":"1"}),
        json!({"op":"pc_message","target":"2","text":"Hello","mode":"nui"}),
        json!({"op":"pc_message","target":"2","text":"A private reply","mode":"dm"}),
        json!({"op":"pc_journal","target":"2","quest":"intro","state":2}),
        json!({"op":"pc_take_item","target":"2","item":"abc"}),
        json!({"op":"pc_boot","target":"2","reason":"Session ended"}),
        json!({"op":"fade","target":"2","direction":"from_black","seconds":2.0}),
        json!({"op":"play_sound","target":"2","resref":"as_pl_birds1"}),
        json!({"op":"pc_heal","target":"2","amount":100000}),
        json!({"op":"pc_raise","target":"3"}),
        json!({"op":"pc_teleport","target":"2","x":15.0,"y":15.0,"z":0.0}),
        json!({"op":"pc_give_gold","target":"2","amount":1000000}),
        json!({"op":"pc_give_xp","target":"2","amount":100000}),
        json!({"op":"pc_give_item","target":"2","resref":"nw_it_mpotion001"}),
        json!({"op":"set_facing","target":"4","f":123.0}),
        json!({"op":"set_visual_transform","target":"5","scale":1.5,"rotate":{"x":10.0,"y":20.0,"z":30.0},"translate":{"x":1.0,"y":2.0,"z":3.0},"reset":false}),
        json!({"op":"set_visual_transform","target":"5","reset":true}),
        json!({"op":"set_position","target":"5","x":15.0,"y":15.0,"z":2.0}),
        json!({"op":"play_animation","target":"4","name":"activate"}),
        json!({"op":"set_useable","target":"5","useable":false}),
        json!({"op":"set_name","target":"1","name":"Zażółć gęślą jaźń"}),
    ]
}

#[test]
fn every_operation_has_the_documented_game_payload() {
    for row in cases() {
        let store = store(true);
        let command = command(row.clone());
        command.validate().unwrap();
        store.enqueue(command, 2).unwrap_or_else(|e| panic!("{row}: {e}"));
        let mut expected = row;
        expected["id"] = json!("test");
        expected["area"] = json!("events");
        expected["players"] = json!(true);
        assert_eq!(store.commands(3)["commands"][0], expected);
        // The report-response route must carry exactly the same permission.
        let reply = store.sync(br#"{"v":1,"boot":"ops","seq":2}"#, 3).unwrap();
        assert_eq!(reply["commands"][0], expected);
    }
}

#[test]
fn per_operation_refusals_and_types() {
    let invalid = [
        json!({"op":"spawn","kind":"creature","r":"x","x":0,"y":0,"z":0,"f":0,"count":11}),
        json!({"op":"spawn","kind":"item","r":"x","x":0,"y":0,"z":0,"f":0,"faction":2}),
        json!({"op":"npc_say","target":"1","text":" \t\r\n"}),
        json!({"op":"stop_combat","x":0,"y":0,"radius":0}),
        json!({"op":"stop_combat","x":0,"y":0,"radius":51}),
        json!({"op":"set_faction","target":"1","faction":65536}),
        json!({"op":"freeze","target":"1","seconds":301}),
        json!({"op":"freeze","target":"1","seconds":0}),
        json!({"op":"unfreeze","target":"nothex"}),
        json!({"op":"pc_heal","target":"1","amount":0}),
        json!({"op":"pc_raise","target":"1"}),
        json!({"op":"pc_teleport","target":"bad-id","x":1,"y":1,"z":0}),
        json!({"op":"pc_give_gold","target":"1","amount":1000001}),
        json!({"op":"pc_give_xp","target":"1","amount":100001}),
        json!({"op":"pc_give_item","target":"1","resref":"../item"}),
        json!({"op":"set_facing","target":"4","f":360}),
        json!({"op":"set_visual_transform","target":"4"}),
        json!({"op":"set_visual_transform","target":"4","reset":true,"scale":2}),
        json!({"op":"set_visual_transform","target":"4","scale":0}),
        json!({"op":"set_visual_transform","target":"4","rotate":{"x":0,"y":361,"z":0}}),
        json!({"op":"set_visual_transform","target":"4","translate":{"x":0,"y":0,"z":1001}}),
        json!({"op":"set_position","target":"1","x":0,"y":0,"z":0}),
        json!({"op":"play_animation","target":"5","name":"activate"}),
        json!({"op":"set_useable","target":"1","useable":true}),
        json!({"op":"set_name","target":"1","name":"a".repeat(201)}),
    ];
    for row in invalid {
        assert_eq!(
            store(true).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Invalid,
            "{row}"
        );
        let mut input = row.clone();
        input["area"] = json!("events");
        assert_eq!(
            crate::operations::command(input.clone(), &store(true), crate::now_ms())
                .unwrap_err()
                .code,
            ErrorCode::Invalid,
            "legacy: {row}"
        );
        let op = input.as_object_mut().unwrap().remove("op").unwrap();
        assert_eq!(
            crate::operations::call(
                &format!("live_{}", op.as_str().unwrap()),
                input,
                &store(true),
                &std::sync::atomic::AtomicBool::new(false)
            )
            .unwrap_err()
            .code,
            ErrorCode::Invalid,
            "catalogue: {row}"
        );
    }
    for row in [
        json!({"op":"freeze","target":"1","seconds":true}),
        json!({"op":"set_faction","target":"1","faction":"unknown"}),
        json!({"op":"npc_say","target":"1","text":"hi","volume":"bad"}),
        json!({"op":"set_useable","target":"4","useable":1}),
        json!({"op":"set_visual_transform","target":"4","rotate":{"x":0}}),
        json!({"op":"pc_give_xp","target":"1","amount":1.5}),
        json!({"op":"spawn","kind":"creature","r":"x","x":0,"y":0,"z":0,"f":0,"count":true}),
    ] {
        let mut row = row;
        row["area"] = json!("events");
        assert!(serde_json::from_value::<Command>(row.clone()).is_err(), "{row}");
    }
    let mut cmd = command(json!({"op":"set_position","target":"4","x":0,"y":0,"z":0}));
    if let Op::SetPosition { x, .. } = &mut cmd.op {
        *x = f32::INFINITY;
    }
    assert!(cmd.validate().is_err());
    let mut cmd = command(json!({"op":"freeze","target":"1","seconds":1}));
    if let Op::Freeze { seconds, .. } = &mut cmd.op {
        *seconds = f32::NAN;
    }
    assert!(cmd.validate().is_err());
}

#[test]
fn player_policy_is_local_revocable_and_independent_of_target_lookup() {
    let off = store(false);
    for row in cases().into_iter().filter(|r| r["op"].as_str().unwrap().starts_with("pc_")) {
        let mut cmd = command(row);
        // A JSON players:true supplied by a caller is never retained by Command.
        let mut value = serde_json::to_value(&cmd).unwrap();
        value["players"] = json!(true);
        cmd = serde_json::from_value(value).unwrap();
        assert_eq!(off.enqueue(cmd, 2).unwrap_err().code, ErrorCode::Disabled);
    }
    let on = store(true);
    on.enqueue(command(json!({"op":"pc_give_gold","target":"2","amount":1})), 2).unwrap();
    on.set_policy(Policy::default());
    assert!(on.commands(3)["commands"].as_array().unwrap().is_empty());
    assert_eq!(on.result("test").unwrap().code, Some(ErrorCode::Disabled));
    let off = store(false);
    off.enqueue(command(json!({"op":"freeze","target":"1","seconds":1})), 2).unwrap();
    assert!(off.commands(3)["commands"][0].get("players").is_none());
    assert!(!serde_json::from_value::<Policy>(json!({"scripts":true})).unwrap().players);
}

#[test]
fn target_refusals_match_the_game() {
    for (row, code) in [
        (json!({"op":"freeze","target":"ff","seconds":1}), ErrorCode::NotFound),
        (json!({"op":"freeze","target":"6","seconds":1}), ErrorCode::OtherArea),
        (json!({"op":"freeze","target":"3","seconds":1}), ErrorCode::Dead),
        (json!({"op":"freeze","target":"2","seconds":1}), ErrorCode::Protected),
        (json!({"op":"set_facing","target":"5","f":90}), ErrorCode::Disabled),
    ] {
        assert_eq!(store(false).enqueue(command(row), 2).unwrap_err().code, code);
    }
    store(true).enqueue(command(json!({"op":"freeze","target":"2","seconds":1})), 2).unwrap();
    assert_eq!(
        store(true)
            .enqueue(command(json!({"op":"set_faction","target":"2","faction":2})), 2)
            .unwrap_err()
            .code,
        ErrorCode::Protected
    );
}

#[test]
fn report_additions_default_and_clear_full_transform() {
    let old: LiveObject = serde_json::from_value(json!({"id":"1"})).unwrap();
    assert_eq!(old.faction, None);
    assert!(old.vfx.is_empty());
    assert_eq!(old.vt, None);
    let row = json!({"id":"4","faction":3,"vfx":[411],"frozen":true,"gold":25,"xp":100,"useable":true,"anim":73,
        "vt":{"scale":1.5,"rotate":{"x":10,"y":20,"z":30},"translate":{"x":1,"y":2,"z":3}}});
    let live = store(false);
    live.sync(
        json!({"v":1,"boot":"ops","seq":2,"areas":[{"area":"events","full":true,"plc":[row]}]})
            .to_string()
            .as_bytes(),
        2,
    )
    .unwrap();
    let o = live.area("events").unwrap().placeables[0].clone();
    assert_eq!(o.faction, Some(3));
    assert_eq!(o.vfx, [411]);
    assert!(o.frozen);
    assert_eq!(o.gold, Some(25));
    assert_eq!(o.xp, Some(100));
    assert_eq!(o.useable, Some(true));
    assert_eq!(o.anim, Some(73));
    assert_eq!(o.vt.unwrap().scale, 1.5);
    live.sync(br#"{"v":1,"boot":"ops","seq":3,"areas":[{"area":"events","full":false}]}"#, 3)
        .unwrap();
    assert!(live.area("events").unwrap().placeables[0].vt.is_some());
    live.sync(br#"{"v":1,"boot":"ops","seq":4,"areas":[{"area":"events","full":true,"plc":[{"id":"4"}]}]}"#,4).unwrap();
    assert!(live.area("events").unwrap().placeables[0].vt.is_none());
    for faction in [json!(3), json!("hostile_temporarily")] {
        let result: CommandResult = serde_json::from_value(json!({"id":"r","ok":true,"faction":faction,"objects":["1","2"],"count":2,"affected":["1"]})).unwrap();
        assert_eq!(result.faction, Some(faction));
        assert_eq!(result.objects.len(), 2);
        assert_eq!(result.affected, ["1"]);
    }
}

#[test]
fn catalogue_dispatches_every_operation_with_exact_game_json() {
    use crate::{now_ms, operations};
    use std::sync::atomic::AtomicBool;
    let stop = AtomicBool::new(false);
    for row in cases() {
        let command = command(row.clone());
        let expected = serde_json::to_value(&command).unwrap();
        let mut arguments = expected.clone();
        let kind = arguments.as_object_mut().unwrap().remove("op").unwrap();
        let live = store(true);
        let result = operations::call(
            &format!("live_{}", kind.as_str().unwrap()),
            arguments.clone(),
            &live,
            &stop,
        )
        .unwrap();
        assert_eq!(result, json!({"queued":"test"}));
        let mut expected = expected;
        expected["players"] = json!(true);
        expected["by"] = json!("default");
        assert_eq!(live.commands(now_ms())["commands"][0], expected);
        // Same schema and policy behavior through the legacy endpoint.
        let legacy = store(true);
        operations::command(serde_json::to_value(&command).unwrap(), &legacy, now_ms()).unwrap();
        assert_eq!(legacy.commands(now_ms())["commands"][0], expected);
        arguments["players"] = json!(true);
        assert_eq!(
            operations::call(
                &format!("live_{}", kind.as_str().unwrap()),
                arguments,
                &store(true),
                &stop
            )
            .unwrap_err()
            .code,
            ErrorCode::Invalid
        );
    }
    let off = store(false);
    let arguments = json!({"area":"events","target":"ffffffff","amount":5});
    assert_eq!(
        operations::call("live_pc_give_gold", arguments.clone(), &off, &stop).unwrap_err().code,
        ErrorCode::Disabled
    );
    let mut arguments = arguments;
    arguments["op"] = json!("pc_give_gold");
    assert_eq!(
        operations::command(arguments, &off, now_ms()).unwrap_err().code,
        ErrorCode::Disabled
    );
}

#[test]
fn chunk_returns_preserve_json_null_and_enforce_the_wire_limit() {
    use crate::operations;
    let live = store(false);
    live.set_policy(Policy { chunks: true, ..Policy::default() });
    let stop = std::sync::atomic::AtomicBool::new(false);
    let input = json!({"id":"ret","area":"events","code":"MgReturn(\"null\");","return":true});
    operations::call("live_run_chunk", input.clone(), &live, &stop).unwrap();
    assert_eq!(live.commands(crate::now_ms())["commands"][0]["return"], true);
    for value in [Value::Null, json!({"nested":[1,true,"text"]}), json!("x".repeat(16382))] {
        let result: CommandResult =
            serde_json::from_value(json!({"id":"ret","ok":true,"value":value})).unwrap();
        assert_eq!(serde_json::to_value(&result).unwrap()["value"], value);
        live.sync(
            json!({"v":1,"boot":"ops","seq":2,"results":[result]}).to_string().as_bytes(),
            crate::now_ms(),
        )
        .unwrap();
    }
    assert_eq!(
        operations::call("live_wait", json!({"id":"ret"}), &live, &stop).unwrap()["value"],
        Value::Null
    );
    let large =
        json!({"v":1,"boot":"ops","seq":3,"results":[{"id":"x","value":"x".repeat(16383)}]});
    assert!(live.sync(large.to_string().as_bytes(), crate::now_ms()).is_err());
    let mut invalid = input;
    invalid["return"] = json!(1);
    assert_eq!(
        operations::call("live_run_chunk", invalid, &live, &stop).unwrap_err().code,
        ErrorCode::Invalid
    );
    let old: CommandResult = serde_json::from_value(json!({"id":"old","ok":true})).unwrap();
    assert!(serde_json::to_value(old).unwrap().get("value").is_none());
}

#[test]
fn deltas_merge_remove_reset_fields_and_recover_gaps() {
    let live = store(false);
    let send = |seq, base: Option<u64>, area: Value| {
        let mut msg = json!({"v":1,"boot":"ops","seq":seq,"areas":[area]});
        if let Some(base) = base {
            msg["base_seq"] = json!(base);
        }
        live.sync(msg.to_string().as_bytes(), seq).unwrap()
    };
    send(
        2,
        Some(1),
        json!({"area":"events","cre":[{"id":"1","name":"changed","vfx":[411]}],"removed":["2"]}),
    );
    let a = live.area("events").unwrap();
    assert_eq!(a.creatures.len(), 2);
    assert_eq!(a.placeables.len(), 1);
    assert_eq!(a.creatures[0].name, "changed");
    send(3, Some(2), json!({"area":"events","cre":[{"id":"1","name":"cleared"}]}));
    assert!(live.area("events").unwrap().creatures[0].vfx.is_empty());
    // The skipped report may contain any change; discard this delta but keep events/results.
    let response = send(5, Some(4), json!({"area":"events","removed":["1"]}));
    assert_eq!(live.area("events").unwrap().creatures.len(), 2);
    assert_eq!(response["commands"][0]["op"], "resync");
    let first = response["commands"][0]["id"].clone();
    send(6, Some(5), json!({"area":"events","removed":["1"]}));
    assert_eq!(live.commands(6)["commands"][0]["id"], first);
    // An acknowledged resync whose full response was lost is reissued with a fresh ID.
    let msg = json!({"v":1,"boot":"ops","seq":7,"base_seq":6,"ack":[first]});
    let response = live.sync(msg.to_string().as_bytes(), 7).unwrap();
    assert_ne!(response["commands"][0]["id"], first);
    send(8, None, json!({"area":"events","full":true,"cre":[{"id":"9"}]}));
    assert!(live.commands(8)["commands"].as_array().unwrap().is_empty());
    assert_eq!(live.area("events").unwrap().creatures[0].id, "9");
    send(9, Some(8), json!({"area":"events","plc":[{"id":"a","vt":{"scale":2}}]}));
    send(10, Some(9), json!({"area":"events","plc":[{"id":"a"}]}));
    assert!(live.area("events").unwrap().placeables[0].vt.is_none());
    // Duplicate/stale deltas neither mutate objects nor request another resync.
    send(9, Some(8), json!({"area":"events","removed":["9"]}));
    send(10, Some(9), json!({"area":"events","removed":["9"]}));
    assert_eq!(live.area("events").unwrap().creatures.len(), 1);
    assert_eq!(live.status(10)["resyncPending"], false);
    let reboot = json!({"v":1,"boot":"new","seq":20,"base_seq":19,"areas":[{"area":"events","cre":[{"id":"b"}]}]});
    assert_eq!(
        live.sync(reboot.to_string().as_bytes(), 20).unwrap()["commands"][0]["op"],
        "resync"
    );
    assert!(live.area("events").is_none());
}

#[test]
fn deltas_need_an_area_baseline_and_bound_retained_objects() {
    let live = store(false);
    let new_area = json!({"v":1,"boot":"ops","seq":2,"base_seq":1,"areas":[{"area":"new","cre":[{"id":"a"}]}]});
    assert_eq!(
        live.sync(new_area.to_string().as_bytes(), 2).unwrap()["commands"][0]["op"],
        "resync"
    );
    let rows: Vec<Value> = (1..=4096).map(|id| json!({"id":format!("{id:x}")})).collect();
    let full =
        json!({"v":1,"boot":"ops","seq":3,"areas":[{"area":"events","full":true,"cre":rows}]});
    live.sync(full.to_string().as_bytes(), 3).unwrap();
    live.enqueue(command(json!({"op":"set_name","target":"1","name":"queued"})), 3).unwrap();
    let before_events = live.events(&crate::events::Query::default());
    let before_other = live.area("other").unwrap();
    // Preparing an earlier valid area must not apply it, accept acks or append
    // events when a later area's retained size exceeds the limit.
    let too_many = json!({"v":1,"boot":"ops","seq":4,"base_seq":3,
        "ack":["test"],"events":[{"seq":1,"kind":"chat","text":"must not arrive"}],
        "areas":[{"area":"other","cre":[{"id":"6","name":"must not change"}]},
                 {"area":"events","cre":[{"id":"aaaa"}]}]});
    assert!(live.sync(too_many.to_string().as_bytes(), 4).is_err());
    assert_eq!(live.area("events").unwrap().creatures.len(), 4096);
    assert_eq!(live.status(4)["seq"], 3);
    assert_eq!(live.area("other").unwrap(), before_other);
    assert_eq!(live.events(&crate::events::Query::default()), before_events);
    assert_eq!(live.commands(4)["commands"][0]["id"], "test");
}

#[test]
fn prepared_delta_keeps_order_kind_moves_metadata_and_derived_events() {
    let live = store(false);
    let before = live.area("events").unwrap();
    let message = json!({"v":1,"boot":"ops","seq":2,"base_seq":1,"areas":[{
        "area":"events","size":[80,90],
        "cre":[{"id":"1","combat":true},{"id":"7"},{"id":"8"}],
        "items":[{"id":"2"}],"removed":["3"]}]});
    live.sync(message.to_string().as_bytes(), 2).unwrap();
    let after = live.area("events").unwrap();
    assert_eq!(after.creatures.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(), ["1", "7", "8"]);
    assert_eq!(after.items[0].id, "2");
    assert_eq!(after.placeables, before.placeables);
    assert_eq!(after.doors, before.doors);
    assert_eq!((after.full_ms, after.has_full), (before.full_ms, before.has_full));
    assert_eq!((after.size, after.seq, after.received_ms), (Some([80, 90]), 2, 2));
    let (events, _) = live.events(&crate::events::Query::default());
    assert!(events.iter().any(|e| e.kind == "combat_start" && e.id.as_deref() == Some("1")));
    assert!(events.iter().any(|e| e.kind == "left" && e.id.as_deref() == Some("2")));
}

#[test]
fn dm_creature_refusals_and_limbo_recovery() {
    for name in ["kill", "heal", "force_rest", "limbo"] {
        for (target, code) in [
            ("2", ErrorCode::Protected),
            ("3", ErrorCode::Dead),
            ("4", ErrorCode::Invalid),
            ("6", ErrorCode::OtherArea),
            ("77", ErrorCode::NotFound),
        ] {
            assert_eq!(
                store(false)
                    .enqueue(command(json!({"op":name,"target":target})), 2)
                    .unwrap_err()
                    .code,
                code,
                "{name} {target}"
            );
        }
    }
    for row in [
        json!({"op":"heal","target":"1","amount":0}),
        json!({"op":"heal","target":"1","amount":100001}),
    ] {
        assert!(command(row).validate().is_err());
    }
    for row in [
        json!({"op":"set_ai","target":"1","level":"wrong"}),
        json!({"op":"set_immortal","target":"1","enabled":1}),
    ] {
        assert!(serde_json::from_value::<Op>(row).is_err());
    }
    let s = store(true);
    s.enqueue(command(json!({"op":"kill","target":"2"})), 2).unwrap();
    assert_eq!(s.commands(3)["commands"][0]["players"], true);
    assert!(
        store(false)
            .enqueue(command(json!({"op":"unlimbo","target":"7ff","x":1,"y":2,"z":0})), 2)
            .is_ok()
    );
}

#[test]
fn locals_validate_types_bounds_and_reserved_names() {
    for (kind, value) in [
        ("int", json!(true)),
        ("int", json!(2147483648u64)),
        ("float", json!("x")),
        ("string", json!("x".repeat(4097))),
        ("object", json!("bad-id")),
        ("location", json!({"x":1})),
    ] {
        assert!(command(json!({"op":"set_variable","target":"module","type":kind,"name":"quest","value":value})).validate().is_err());
    }
    for name in ["MGLIVE_ALLOW_PLAYER_OPS", "nwnx_secret", "bad/name", ""] {
        assert!(
            command(json!({"op":"get_variable","target":"module","type":"int","name":name}))
                .validate()
                .is_err()
        );
    }
    for target in ["module", "area", "1", "2"] {
        let s = store(false);
        s.enqueue(
            command(json!({"op":"get_variable","target":target,"type":"int","name":"quest"})),
            2,
        )
        .unwrap();
    }
    assert_eq!(
        store(false)
            .enqueue(
                command(
                    json!({"op":"set_variable","target":"2","type":"int","name":"quest","value":1})
                ),
                2
            )
            .unwrap_err()
            .code,
        ErrorCode::Protected
    );
    store(true)
        .enqueue(
            command(
                json!({"op":"set_variable","target":"2","type":"int","name":"quest","value":1}),
            ),
            2,
        )
        .unwrap();
    let operations = crate::operations::describe(Policy::default());
    for name in ["live_get_variable", "live_dump_locals"] {
        assert_eq!(
            operations["operations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["name"] == name)
                .unwrap()["readOnly"],
            true
        );
        assert_eq!(crate::access::group(name), Some(crate::access::ClientGroup::Read));
    }
    assert_eq!(
        crate::access::group("live_set_variable"),
        Some(crate::access::ClientGroup::Variables)
    );
}

#[test]
fn inventory_selection_and_player_protection() {
    for row in [
        json!({"op":"take_item","target":"1"}),
        json!({"op":"take_item","target":"1","item":"abc","resref":"foo"}),
        json!({"op":"take_item","target":"1","item":"bad-id"}),
        json!({"op":"view_inventory","target":"5"}),
    ] {
        assert!(store(true).enqueue(command(row), 2).is_err());
    }
    for op in ["view_inventory", "take_item"] {
        let mut row = json!({"op":op,"target":"2"});
        if op == "take_item" {
            row["resref"] = json!("foo");
        }
        assert_eq!(
            store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Protected
        );
        store(true).enqueue(command(row), 2).unwrap();
    }
}

#[test]
fn world_ranges_and_players_gate() {
    for row in [
        json!({"op":"set_time","hour":24,"minute":0}),
        json!({"op":"set_time","hour":1,"minute":60}),
        json!({"op":"set_date","year":32001,"month":1,"day":1}),
        json!({"op":"set_date","year":1,"month":0,"day":1}),
        json!({"op":"set_date","year":1,"month":1,"day":29}),
    ] {
        assert!(store(true).enqueue(command(row), 2).is_err());
    }
    assert_eq!(
        store(false)
            .enqueue(command(json!({"op":"jump_all_players","x":1,"y":2,"z":0})), 2)
            .unwrap_err()
            .code,
        ErrorCode::Disabled
    );
}

#[test]
fn traps_validate_target_and_row() {
    for op in ["disable_trap", "spawn_trap_on_object"] {
        let mut row = json!({"op":op,"target":"1"});
        if op == "spawn_trap_on_object" {
            row["trap"] = json!(0);
        }
        assert_eq!(store(true).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Invalid);
    }
    for trap in [json!(-1), json!(65536), json!(true), json!(1.5)] {
        assert!(
            serde_json::from_value::<Command>(
                json!({"area":"events","op":"spawn_trap_on_object","target":"4","trap":trap})
            )
            .is_err()
        );
    }
}

#[test]
fn dm_player_operations_validate_and_require_switch() {
    let rows = [
        json!({"op":"give_level","target":"2"}),
        json!({"op":"give_alignment","target":"2","law_chaos":0,"good_evil":1}),
        json!({"op":"set_stat","target":"2","ability":"dexterity","value":20}),
        json!({"op":"get_faction_reputation","target":"2","faction":3}),
        json!({"op":"set_faction_reputation","target":"2","faction":3,"reputation":10}),
    ];
    for row in rows {
        assert_eq!(
            store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Disabled
        );
        let mut wrong = row;
        wrong["target"] = json!("4");
        assert_eq!(store(true).enqueue(command(wrong), 2).unwrap_err().code, ErrorCode::Invalid);
    }
    for row in [
        json!({"op":"give_alignment","target":"2","law_chaos":101,"good_evil":0}),
        json!({"op":"give_alignment","target":"2","law_chaos":0,"good_evil":-101}),
        json!({"op":"set_faction_reputation","target":"2","faction":3,"reputation":101}),
    ] {
        assert!(store(true).enqueue(command(row), 2).is_err());
    }
    for row in [
        json!({"op":"set_stat","target":"2","ability":"ac","value":20}),
        json!({"op":"set_stat","target":"2","ability":"strength","value":256}),
        json!({"op":"set_stat","target":"2","ability":"strength","value":true}),
    ] {
        let mut row = row;
        row["area"] = json!("events");
        assert!(serde_json::from_value::<Command>(row).is_err());
    }
}

#[test]
fn attack_faction_is_optional_and_validated() {
    for faction in [json!(0), json!(65536), json!("missing")] {
        let row = json!({"op":"npc_attack","target":"1","victim":"2","faction":faction});
        let mut raw = row.clone();
        raw["area"] = json!("events");
        if let Ok(command) = serde_json::from_value::<Command>(raw) {
            assert!(command.validate().is_err());
        }
    }
    assert!(command(json!({"op":"npc_attack","target":"1","victim":"2"})).validate().is_ok());
}

#[test]
fn forced_kill_result_preserves_override_readback() {
    let raw = json!({"id":"kill","ok":true,"value":{"dead":true,"overridden":{"plot":true,"immortal":true}}});
    let result: CommandResult = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(result.value, Some(raw["value"].clone()));
    assert_eq!(
        store(false).enqueue(command(json!({"op":"kill","target":"2"})), 2).unwrap_err().code,
        ErrorCode::Protected
    );
    assert_eq!(
        store(true).enqueue(command(json!({"op":"kill","target":"3"})), 2).unwrap_err().code,
        ErrorCode::Dead
    );
}

#[test]
fn living_and_dead_player_lifecycle_gates() {
    let live = store(true);
    live.sync(br#"{"v":1,"boot":"ops","seq":2,"areas":[{"area":"events","cre":[{"id":"2","pc":true},{"id":"3","pc":true,"dead":true}]}]}"#, 2).unwrap();
    for op in ["kill", "heal", "pc_heal"] {
        let mut living = command(json!({"op":op,"target":"2"}));
        living.id = format!("{op}-living");
        live.enqueue(living, 3).unwrap();
        let mut dead = command(json!({"op":op,"target":"3"}));
        dead.id = format!("{op}-dead");
        assert_eq!(live.enqueue(dead, 3).unwrap_err().code, ErrorCode::Dead);
    }
    assert_eq!(
        store(true).enqueue(command(json!({"op":"pc_raise","target":"2"})), 3).unwrap_err().code,
        ErrorCode::Invalid
    );
    live.enqueue(
        Command {
            id: "raise-dead".into(),
            area: "events".into(),
            op: Op::PcRaise { target: "3".into() },
        },
        3,
    )
    .unwrap();
}

#[test]
fn vfx_validation_and_player_permissions() {
    for row in [
        json!({"op":"apply_vfx","target":"1","location":{"x":0,"y":0,"z":0},"row":411,"duration":"permanent"}),
        json!({"op":"apply_vfx","row":411,"duration":"permanent"}),
        json!({"op":"apply_vfx","target":"1","row":411,"duration":"temporary"}),
        json!({"op":"apply_vfx","target":"1","row":411,"duration":"temporary","seconds":86401}),
        json!({"op":"apply_vfx","target":"1","row":411,"duration":"instant","seconds":1}),
    ] {
        assert!(command(row).validate().is_err());
    }
    for row in [
        json!({"op":"apply_vfx","target":"2","row":411,"duration":"permanent"}),
        json!({"op":"clear_vfx","target":"2"}),
    ] {
        assert_eq!(
            store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Protected
        );
        store(true).enqueue(command(row), 2).unwrap();
    }
}

#[test]
fn fighting_faction_results_survive_report_ingestion() {
    let s = store(false);
    s.sync(br#"{"v":1,"boot":"ops","seq":2,"results":[{"id":"a","ok":true,"faction":{"original":1,"temporary":2}}]}"#, 2).unwrap();
    assert!(s.sync(br#"{"v":1,"boot":"ops","seq":3,"results":[{"id":"b","ok":true,"faction":{"original":1,"temporary":65536}}]}"#, 3).is_err());
}

#[test]
fn appearance_report_and_refusals() {
    let s = store(false);
    assert_eq!(s.area("events").unwrap().creatures[0].app, None);
    s.sync(
        br#"{"v":1,"boot":"ops","seq":2,"areas":[{"area":"events","cre":[{"id":"1","app":0}]}]}"#,
        2,
    )
    .unwrap();
    assert_eq!(s.area("events").unwrap().creatures[0].app, Some(0));
    assert_eq!(
        store(false)
            .enqueue(command(json!({"op":"set_appearance","target":"5","row":0})), 2)
            .unwrap_err()
            .code,
        ErrorCode::Invalid
    );
    assert_eq!(
        store(false)
            .enqueue(command(json!({"op":"set_appearance","target":"2","row":0})), 2)
            .unwrap_err()
            .code,
        ErrorCode::Protected
    );
    store(true).enqueue(command(json!({"op":"set_appearance","target":"2","row":0})), 2).unwrap();
}

#[test]
fn library_json_is_bounded_typed_and_requires_script_consent() {
    let good = json!({"__data_type":"UTC ","Tag":{"type":"cexostring","value":"Library"}});
    validate_object_json(&good).unwrap();
    for bad in [
        json!({}),
        json!({"__data_type":"ARE "}),
        json!({"__data_type":"UTC ","IsPC":{"type":"byte","value":1}}),
        json!({"__data_type":"UTC ","Bad":{"type":"byte","value":256}}),
        json!({"__data_type":"UTC ","Tag":{"type":"cexostring","value":"x".repeat(262144)}}),
    ] {
        assert!(validate_object_json(&bad).is_err(), "{bad}");
    }
    let c = command(json!({"op":"spawn_json","json":good,"x":1,"y":1,"z":0,"f":0}));
    let access = crate::access::Access::new(
        "a".repeat(32),
        &[crate::access::ClientToken {
            name: "limited".into(),
            token: "b".repeat(32),
            groups: vec![crate::access::ClientGroup::Spawn],
            per_minute: 60,
            delivery: Default::default(),
        }],
    )
    .unwrap();
    let scope = access.scope(1);
    assert!(!scope.allows("live_spawn_json"));
    assert_eq!(
        store(false).enqueue_scoped(c.clone(), 2, scope).unwrap_err().code,
        ErrorCode::Forbidden
    );
    let s = store(false);
    s.set_policy(Policy { scripts: false, ..Policy::default() });
    assert_eq!(s.enqueue(c, 2).unwrap_err().code, ErrorCode::Disabled);
    let large = json!({"__data_type":"UTC ","Tag":{"type":"cexostring","value":"x".repeat(17000)}});
    store(false).sync(json!({"v":1,"boot":"ops","seq":2,"results":[{"id":"capture","ok":true,"object_json":large}]}).to_string().as_bytes(),2).unwrap();
}

#[test]
#[ignore = "Requires this worktree's private capture engine proof"]
fn library_engine_snapshot_ingestion() {
    let path = std::env::var("MGLIVE_CAPTURE_PROOF").expect("proof path");
    let frames: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut captures = 0;
    for frame in frames {
        for result in frame["results"].as_array().into_iter().flatten() {
            if let Some(snapshot) = result.get("object_json") {
                validate_object_json(snapshot).unwrap();
                captures += 1;
                let s = LiveStore::default();
                s.sync(frame.to_string().as_bytes(), 2).unwrap();
                let cmd =
                    command(json!({"op":"spawn_json","json":snapshot,"x":0,"y":0,"z":0,"f":0}));
                cmd.validate().unwrap();
            }
        }
    }
    assert!(captures > 0);
}

#[test]
fn capture_batches_are_bounded() {
    let s = store(false);
    for n in 0..6 {
        let mut cmd = command(json!({"op":"capture_object","target":"1"}));
        cmd.id = format!("c{n}");
        s.enqueue(cmd, 2).unwrap();
    }
    assert_eq!(s.commands(3)["commands"].as_array().unwrap().len(), 4);
}

#[test]
fn skill_checks_validate_and_require_players() {
    let row = json!({"op":"skill_check","x":0,"y":0,"radius":30,"skill":0,"dc":255});
    assert_eq!(
        store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
        ErrorCode::Disabled
    );
    store(true).enqueue(command(row.clone()), 2).unwrap();
    for (key, value) in [
        ("radius", json!(31)),
        ("radius", json!(0)),
        ("success_text", json!("x".repeat(201))),
        ("fail_text", json!("  ")),
    ] {
        let mut bad = row.clone();
        bad[key] = value;
        assert!(command(bad).validate().is_err());
    }
}

#[test]
fn patrol_routes_and_protection() {
    for points in [json!([]), json!(vec![json!({"x":0,"y":0,"z":0}); 17])] {
        assert!(command(json!({"op":"patrol","target":"1","points":points})).validate().is_err());
    }
    let row = json!({"op":"patrol","target":"2","points":[{"x":0,"y":0,"z":0}]});
    assert_eq!(store(true).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Protected);
    assert_eq!(
        store(true)
            .enqueue(command(json!({"op":"patrol","target":"3","points":[{"x":0,"y":0,"z":0}]})), 2)
            .unwrap_err()
            .code,
        ErrorCode::Dead
    );
    assert_eq!(
        store(true).enqueue(command(json!({"op":"stop_patrol","target":"4"})), 2).unwrap_err().code,
        ErrorCode::Invalid
    );
}

#[test]
fn tile_fields_have_integer_bounds() {
    let good = json!({"op":"set_tile","x":1,"y":1,"tile_id":51,"orientation":1,"height":1});
    store(false).enqueue(command(good.clone()), 2).unwrap();
    for (key, value) in [
        ("x", json!(-1)),
        ("tile_id", json!(65536)),
        ("orientation", json!(4)),
        ("height", json!(256)),
        ("x", json!(1.5)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        bad["area"] = json!("events");
        assert_eq!(
            crate::operations::command(bad, &store(false), 2).unwrap_err().code,
            ErrorCode::Invalid
        );
    }
}

#[test]
fn stop_combat_requires_one_complete_selection_and_protects_target() {
    for fields in [
        json!({}),
        json!({"x":0}),
        json!({"x":0,"y":0}),
        json!({"radius":5}),
        json!({"target":"1","x":0}),
        json!({"target":"1","radius":5}),
        json!({"target":"1","x":0,"y":0,"radius":5}),
        json!({"target":"not-hex"}),
    ] {
        let mut input = fields;
        input["area"] = json!("events");
        let service = store(true);
        assert_eq!(
            crate::operations::call(
                "live_stop_combat",
                input.clone(),
                &service,
                &std::sync::atomic::AtomicBool::new(false)
            )
            .unwrap_err()
            .code,
            ErrorCode::Invalid,
            "{input}"
        );
        input["op"] = json!("stop_combat");
        assert_eq!(
            crate::operations::command(input.clone(), &service, 2).unwrap_err().code,
            ErrorCode::Invalid,
            "{input}"
        );
    }
    for (target, expected) in [
        ("3", ErrorCode::Dead),
        ("4", ErrorCode::Invalid),
        ("6", ErrorCode::OtherArea),
        ("7", ErrorCode::NotFound),
    ] {
        for players in [false, true] {
            assert_eq!(
                store(players)
                    .enqueue(command(json!({"op":"stop_combat","target":target})), 2)
                    .unwrap_err()
                    .code,
                expected
            );
        }
    }
    let mut invalid = command(json!({"op":"stop_combat","x":0,"y":0,"radius":5}));
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        invalid.op = Op::StopCombat { target: None, x: Some(bad), y: Some(0.0), radius: Some(5.0) };
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn skill_one_player_is_exclusive_and_protected() {
    let row = json!({"op":"skill_check","target":"2","skill":0,"dc":15});
    assert_eq!(
        store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
        ErrorCode::Disabled
    );
    store(true).enqueue(command(row.clone()), 2).unwrap();
    for patch in [json!({"x":0}), json!({"y":0}), json!({"radius":10}), json!({"target":""})] {
        let mut bad = row.clone();
        bad.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        assert!(command(bad).validate().is_err());
    }
    for (target, code) in [
        ("1", ErrorCode::Protected),
        ("3", ErrorCode::Dead),
        ("6", ErrorCode::OtherArea),
        ("ffff", ErrorCode::NotFound),
    ] {
        let mut input = row.clone();
        input["target"] = json!(target);
        assert_eq!(store(true).enqueue(command(input), 2).unwrap_err().code, code);
    }
    assert!(command(json!({"op":"skill_check","skill":0,"dc":15})).validate().is_err());
}

#[test]
fn loot_followup_refusals() {
    for row in [
        json!({"op":"set_lootable","target":"2","lootable":false}),
        json!({"op":"set_lootable","target":"3","lootable":false}),
    ] {
        assert!(store(true).enqueue(command(row.clone()), 2).is_err(), "{row}");
    }
}

#[test]
fn static_followup_refusals() {
    let row = json!({"op":"set_static","target":"1","static":true});
    assert_eq!(store(true).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Invalid);
}

#[test]
fn appearance_followup_refusals() {
    let row = json!({"op":"set_appearance","target":"5","row":4});
    assert_eq!(store(true).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Invalid);
}

#[test]
fn dm_followup_validation_and_player_gates() {
    for row in [
        json!({"op":"pc_journal","target":"2","quest":"../x","state":1}),
        json!({"op":"pc_journal","target":"2","quest":"quest","state":2147483648u32}),
        json!({"op":"pc_message","target":"2","text":"","mode":"chat"}),
        json!({"op":"pc_message","target":"2","text":"x".repeat(201),"mode":"chat"}),
        json!({"op":"pc_take_item","target":"2","item":"a","resref":"potion"}),
        json!({"op":"pc_boot","target":"2","reason":"x".repeat(201)}),
        json!({"op":"fade","target":"2","direction":"to_black","seconds":61}),
        json!({"op":"play_sound","target":"2","resref":"../sound"}),
    ] {
        assert!(store(true).enqueue(command(row.clone()), 2).is_err(), "{row}");
    }
    for row in cases().into_iter().filter(|v| {
        matches!(
            v["op"].as_str(),
            Some("pc_message" | "pc_journal" | "pc_take_item" | "pc_boot" | "fade" | "play_sound")
        )
    }) {
        assert_eq!(
            store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Disabled
        );
        let mut npc = row.clone();
        npc["target"] = json!("1");
        assert_eq!(store(true).enqueue(command(npc), 2).unwrap_err().code, ErrorCode::Protected);
        store(true).enqueue(command(row), 2).unwrap();
    }
    for op in [
        json!({"op":"npc_move","target":"2","x":1,"y":2,"z":0}),
        json!({"op":"npc_attack","target":"2","victim":"1"}),
    ] {
        assert_eq!(
            store(false).enqueue(command(op.clone()), 2).unwrap_err().code,
            ErrorCode::Protected
        );
        let store = store(true);
        store.enqueue(command(op), 2).unwrap();
        assert_eq!(store.commands(3)["commands"][0]["players"], true);
    }
    assert!(
        store(true)
            .enqueue(command(json!({"op":"npc_attack","target":"2","victim":"1","faction":1})), 2)
            .is_err()
    );
    let s = store(true);
    assert_eq!(s.area("events").unwrap().placeables[0].is_static, None);
    assert_eq!(s.area("events").unwrap().creatures[0].loot, None);
}

#[test]
fn teleport_destination_bounds_alias_and_retries() {
    let s = store(true);
    s.lock()
        .areas
        .get_mut("events")
        .unwrap()
        .items
        .push(LiveObject { id: "7".into(), ..Default::default() });
    s.sync(json!({"v":1,"boot":"ops","seq":2,"areas":[{"area":"events","size":[20,20],"cre":[{"id":"1"},{"id":"2","pc":true},{"id":"3","dead":true}]},{"area":"other","size":[40,30],"cre":[{"id":"6"}]}]}).to_string().as_bytes(),2).unwrap();
    for (i, op) in [
        json!({"op":"teleport","target":"1","area":"other","x":25.0,"y":20.0,"z":-2.0,"f":180.0}),
        json!({"op":"teleport","target":"4","x":4.0,"y":5.0,"z":0.0}),
        json!({"op":"teleport","target":"7","x":4.0,"y":5.0,"z":0.0}),
        json!({"op":"teleport","target":"5","x":4.0,"y":5.0,"z":0.0}),
        json!({"op":"teleport","target":"2","x":4.0,"y":5.0,"z":0.0}),
        json!({"op":"pc_teleport","target":"2","area":"other","x":25.0,"y":20.0,"z":0.0,"f":90.0}),
        json!({"op":"teleport","target":"1","area":"unreported","x":4.0,"y":5.0,"z":0.0}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut op = op;
        op["id"] = json!(format!("tp-{i}"));
        let command: Command = serde_json::from_value(op.clone()).unwrap();
        s.enqueue(command.clone(), 3).unwrap();
        assert_eq!(s.enqueue(command, 4).unwrap(), format!("tp-{i}"));
        let wire = game_commands(&s.lock());
        op["players"] = json!(true);
        assert_eq!(wire["commands"][i], op);
    }
    // Omitted destination survives an idempotent retry after movement.
    let omitted: Command =
        serde_json::from_value(json!({"id":"same","op":"teleport","target":"1","x":4,"y":5,"z":0}))
            .unwrap();
    s.enqueue(omitted.clone(), 4).unwrap();
    s.sync(json!({"v":1,"boot":"ops","seq":3,"areas":[{"area":"events","cre":[]},{"area":"other","cre":[{"id":"1"}]}]}).to_string().as_bytes(),5).unwrap();
    assert_eq!(s.enqueue(omitted, 6).unwrap(), "same");
    assert_eq!(s.area_json("events", 6).unwrap()["size"], json!([20, 20]));
    for fields in
        [json!({"x":40}), json!({"y":30}), json!({"x":-1}), json!({"f":360}), json!({"f":-1})]
    {
        let mut op =
            json!({"id":"bad","op":"teleport","target":"1","area":"other","x":4,"y":5,"z":0});
        op.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        let c: Command = serde_json::from_value(op).unwrap();
        assert_eq!(s.enqueue(c, 7).unwrap_err().code, ErrorCode::Invalid);
    }
    let c = Command {
        id: String::new(),
        area: String::new(),
        op: Op::Teleport { target: "1".into(), x: f32::NAN, y: 1.0, z: 0.0, f: None },
    };
    assert!(c.validate().is_err());
}

#[test]
fn teleport_scope_protection_and_optional_area_are_specific() {
    let s = store(false);
    for (target, code) in
        [("2", ErrorCode::Disabled), ("3", ErrorCode::Dead), ("fff", ErrorCode::NotFound)]
    {
        assert_eq!(
            s.enqueue(command(json!({"op":"teleport","target":target,"x":2,"y":3,"z":0})), 2)
                .unwrap_err()
                .code,
            code
        );
    }
    assert_eq!(
        s.enqueue(command(json!({"op":"pc_teleport","target":"1","x":2,"y":3,"z":0})), 2)
            .unwrap_err()
            .code,
        ErrorCode::Disabled
    );
    let invalid: Command = serde_json::from_value(json!({"op":"heal","target":"1"})).unwrap();
    assert!(invalid.validate().is_err());
    assert_eq!(crate::access::group("live_teleport"), Some(crate::access::ClientGroup::Npc));
    let stop = std::sync::atomic::AtomicBool::new(false);
    crate::operations::call("live_teleport", json!({"target":"1","x":2,"y":3,"z":0}), &s, &stop)
        .unwrap();
    assert_eq!(
        crate::operations::call(
            "live_teleport",
            json!({"target":"1","x":2,"y":3,"z":0,"f":360}),
            &s,
            &stop
        )
        .unwrap_err()
        .code,
        ErrorCode::Invalid
    );
}

#[test]
fn teleport_named_npc_client_cannot_force_a_player() {
    use crate::access::{Access, ClientGroup, ClientToken};
    let s = store(true);
    let access = Access::new(
        "a".repeat(32),
        &[ClientToken {
            name: "npc".into(),
            token: "b".repeat(32),
            groups: vec![ClientGroup::Npc],
            ..Default::default()
        }],
    )
    .unwrap();
    let scope = access.scope(1);
    let stop = std::sync::atomic::AtomicBool::new(false);
    assert_eq!(
        crate::operations::call_scoped(
            "live_teleport",
            json!({"target":"2","x":1,"y":2,"z":0}),
            &s,
            &stop,
            scope
        )
        .unwrap_err()
        .code,
        ErrorCode::Disabled
    );
    crate::operations::call_scoped(
        "live_teleport",
        json!({"target":"1","x":1,"y":2,"z":0}),
        &s,
        &stop,
        scope,
    )
    .unwrap();
    let wire = game_commands(&s.lock());
    assert!(wire["commands"][0].get("players").is_none());
}

#[test]
fn stop_combat_pc_requires_players_and_only_emits_exact_target() {
    let op = json!({"op":"stop_combat","target":"2"});
    assert_eq!(store(false).enqueue(command(op.clone()), 2).unwrap_err().code, ErrorCode::Disabled);
    let enabled = store(true);
    enabled.enqueue(command(op.clone()), 2).unwrap();
    let wire = game_commands(&enabled.lock());
    assert_eq!(
        wire["commands"][0],
        json!({"id":"test","area":"events","op":"stop_combat","target":"2","players":true})
    );
    use crate::access::{Access, ClientGroup, ClientToken};
    let access = Access::new(
        "a".repeat(32),
        &[ClientToken {
            name: "npc".into(),
            token: "b".repeat(32),
            groups: vec![ClientGroup::Npc],
            ..Default::default()
        }],
    )
    .unwrap();
    assert_eq!(
        store(true).enqueue_scoped(command(op), 2, access.scope(1)).unwrap_err().code,
        ErrorCode::Disabled
    );
    for patch in [json!({"dm":true}), json!({"possessed":true}), json!({"dead":true})] {
        let mut row = json!({"id":"2","pc":true});
        row.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
        let s = store(true);
        s.sync(
            json!({"v":1,"boot":"ops","seq":2,"areas":[{"area":"events","cre":[row]}]})
                .to_string()
                .as_bytes(),
            2,
        )
        .unwrap();
        let expected = if patch["dead"] == true { ErrorCode::Dead } else { ErrorCode::Protected };
        assert_eq!(
            s.enqueue(command(json!({"op":"stop_combat","target":"2"})), 3).unwrap_err().code,
            expected
        );
    }
}

#[test]
fn animation_replacement_validation_and_player_gates() {
    {
        let op = "replace_object_animation";
        for fields in [
            json!({"old":"walk"}),
            json!({"old":"walk","new":"run","reset":true}),
            json!({"old":"../x","new":"run"}),
            json!({"old":"walk","new":"x".repeat(17)}),
            json!({"old":"","reset":true}),
        ] {
            let mut value = json!({"op":op,"target":"1"});
            value.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
            assert!(command(value).validate().is_err());
        }
        assert_eq!(
            store(false)
                .enqueue(command(json!({"op":op,"target":"2","old":"walk","new":"run"})), 2)
                .unwrap_err()
                .code,
            ErrorCode::Protected
        );
        store(true)
            .enqueue(command(json!({"op":op,"target":"2","old":"walk","new":"run"})), 2)
            .unwrap();
        assert_eq!(crate::access::group(op), Some(crate::access::ClientGroup::Npc));
    }
}

#[test]
fn skill_roll_progress_is_owned_bounded_and_finishes_once() {
    let store = store(true);
    let op = json!({"op":"skill_check","target":"2","skill":1,"dc":15,"bonus":2,"show_dc":false,"timeout":120});
    store.enqueue(command(op.clone()), 2).unwrap();
    let send = |seq, pending, status| json!({"v":1,"boot":"ops","seq":seq,"results":[{"id":"test","ok":true,"pending":pending,"value":{"players":[{"id":"2","status":status}]}}],"ack":["test"]});
    store.sync(send(2, true, "waiting").to_string().as_bytes(), 3).unwrap();
    assert!(store.result("test").unwrap().pending);
    assert_eq!(
        store.commands(40_000)["commands"],
        json!([]),
        "accepted prompt must not be sent again or expire at command TTL"
    );
    store.sync(send(3, true, "waiting").to_string().as_bytes(), 40_001).unwrap();
    store.sync(send(4, false, "rolled").to_string().as_bytes(), 40_002).unwrap();
    assert!(!store.result("test").unwrap().pending);
    assert_eq!(store.result("test").unwrap().value.unwrap()["players"][0]["status"], "rolled");
    store.sync(send(5, true, "waiting").to_string().as_bytes(), 40_003).unwrap();
    assert!(!store.result("test").unwrap().pending, "late replay cannot reopen a completed roll");
    assert_eq!(store.status(40_004)["results"].as_array().unwrap().len(), 1);
    let mut cmd = command(op.clone());
    cmd.id = "offline".into();
    store.enqueue(cmd, 40_005).unwrap();
    store.sync(br#"{"v":1,"boot":"ops","seq":6,"results":[{"id":"offline","ok":true,"pending":true}]}"#,40_006).unwrap();
    store.expire(165_007);
    assert_eq!(store.result("offline").unwrap().code, Some(ErrorCode::Unknown));
    let mut cmd = command(op);
    cmd.id = "restart".into();
    store.enqueue(cmd, 165_008).unwrap();
    store.sync(br#"{"v":1,"boot":"ops","seq":7,"results":[{"id":"restart","ok":true,"pending":true}]}"#,165_009).unwrap();
    store.sync(br#"{"v":1,"boot":"new","seq":1}"#, 165_010).unwrap();
    assert_eq!(store.result("restart").unwrap().code, Some(ErrorCode::Unknown));
    for (key, value) in [
        ("bonus", json!(-101)),
        ("bonus", json!(101)),
        ("timeout", json!(0)),
        ("timeout", json!(601)),
    ] {
        let mut bad = json!({"op":"skill_check","target":"2","skill":1,"dc":15});
        bad[key] = value;
        assert!(command(bad).validate().is_err());
    }
}

#[test]
fn skill_roll_events_follow_the_request_owner() {
    use crate::access::{Access, ClientGroup, ClientToken};
    let s = store(true);
    let access = Access::new(
        "a".repeat(32),
        &[
            ClientToken {
                name: "requester".into(),
                token: "b".repeat(32),
                groups: vec![ClientGroup::Read, ClientGroup::Players],
                ..Default::default()
            },
            ClientToken {
                name: "reader".into(),
                token: "c".repeat(32),
                groups: vec![ClientGroup::Read, ClientGroup::ReadChat],
                ..Default::default()
            },
        ],
    )
    .unwrap();
    s.enqueue_scoped(
        command(json!({"op":"skill_check","target":"2","skill":1,"dc":15})),
        2,
        access.scope(1),
    )
    .unwrap();
    s.sync(br#"{"v":1,"boot":"ops","seq":2,"events":[{"kind":"skill_roll","seq":1,"id":"2","request":"test","text":"private result"}]}"#,3).unwrap();
    let q = crate::events::Query { kinds: vec!["skill_roll".into()], ..Default::default() };
    assert_eq!(s.events_scoped(&q, access.scope(1)).0.len(), 1);
    assert!(s.events_scoped(&q, access.scope(2)).0.is_empty());
    assert_eq!(s.events(&q).0.len(), 1);
}

#[test]
fn economy_limits_and_players() {
    for row in [
        json!({"op":"take_gold","amount":0}),
        json!({"op":"take_gold","amount":1000001}),
        json!({"op":"take_xp","amount":100001}),
        json!({"op":"take_xp","amount":0}),
        json!({"op":"set_xp","value":2147483648u64}),
    ] {
        let mut row = row;
        row["target"] = json!("1");
        assert_eq!(store(false).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Invalid);
    }
    for row in [
        json!({"op":"take_gold","amount":10}),
        json!({"op":"take_xp","amount":10}),
        json!({"op":"set_xp","value":0}),
        json!({"op":"remove_level"}),
    ] {
        let mut row = row;
        row["target"] = json!("1");
        store(false).enqueue(command(row.clone()), 2).unwrap();
        row["target"] = json!("2");
        assert_eq!(
            store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Disabled
        );
        store(true).enqueue(command(row.clone()), 2).unwrap();
        row["target"] = json!("3");
        assert_eq!(store(true).enqueue(command(row.clone()), 2).unwrap_err().code, ErrorCode::Dead);
        row["target"] = json!("4");
        assert_eq!(store(true).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Invalid);
    }
}

#[test]
fn creature_animation_limits_and_policy() {
    let good = json!({"op":"play_creature_animation","target":"1","animation":105});
    for (field, value) in [
        ("animation", json!(200)),
        ("animation", json!(99)),
        ("animation", json!("../dance")),
        ("speed", json!(0)),
        ("speed", json!(11)),
        ("seconds", json!(0)),
        ("seconds", json!(301)),
    ] {
        let mut row = good.clone();
        row[field] = value;
        assert_eq!(store(false).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Invalid);
    }
    let mut row = good;
    store(false).enqueue(command(row.clone()), 2).unwrap();
    row["target"] = json!("2");
    assert_eq!(
        store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
        ErrorCode::Disabled
    );
    store(true).enqueue(command(row.clone()), 2).unwrap();
    row["target"] = json!("3");
    assert_eq!(store(true).enqueue(command(row.clone()), 2).unwrap_err().code, ErrorCode::Dead);
    row["target"] = json!("4");
    assert_eq!(store(true).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Invalid);
}

#[test]
fn inventory_orders_validate_ids_slots_and_player_gates() {
    for op in ["equip_item", "unequip_item", "drop_item"] {
        let mut row = json!({"op":op,"target":"1","item":"abc"});
        if op != "drop_item" {
            row["slot"] = json!(7);
        }
        store(false).enqueue(command(row.clone()), 2).unwrap();
        row["target"] = json!("2");
        assert_eq!(
            store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Disabled
        );
        store(true).enqueue(command(row.clone()), 2).unwrap();
        row["target"] = json!("3");
        assert_eq!(store(true).enqueue(command(row.clone()), 2).unwrap_err().code, ErrorCode::Dead);
        row["target"] = json!("4");
        assert_eq!(
            store(true).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Invalid
        );
        row["target"] = json!("1");
        row["item"] = json!("invalid-id");
        assert!(command(row.clone()).validate().is_err());
        if op != "drop_item" {
            row["item"] = json!("abc");
            row["slot"] = json!(14);
            assert!(command(row).validate().is_err());
        }
    }
}

#[test]
fn npc_give_item_requires_players_only_for_pcs() {
    let mut row = json!({"op":"give_item","target":"1","resref":"nw_it_mring001"});
    store(false).enqueue(command(row.clone()), 2).unwrap();
    row["target"] = json!("2");
    assert_eq!(
        store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
        ErrorCode::Disabled
    );
    store(true).enqueue(command(row.clone()), 2).unwrap();
    row["resref"] = json!("../invalid");
    assert!(command(row).validate().is_err());
}

#[test]
fn inventory_use_transfer_check_both_ends() {
    for op in ["use_item", "transfer_item"] {
        let base = json!({"op":op,"target":"1","item":"abc","recipient":"2"});
        assert_eq!(
            store(false).enqueue(command(base.clone()), 2).unwrap_err().code,
            ErrorCode::Disabled
        );
        store(true).enqueue(command(base.clone()), 2).unwrap();
        for (id, code) in [
            ("3", ErrorCode::Dead),
            ("4", ErrorCode::Invalid),
            ("6", ErrorCode::OtherArea),
            ("ff", ErrorCode::NotFound),
        ] {
            let mut row = base.clone();
            row["recipient"] = json!(id);
            assert_eq!(store(true).enqueue(command(row), 2).unwrap_err().code, code);
        }
        let mut row = base;
        row["item"] = json!("../bad");
        assert!(command(row).validate().is_err());
    }
    assert!(
        serde_json::from_value::<Command>(
            json!({"id":"x","area":"events","op":"use_item","target":"1","item":"a","property":256})
        )
        .is_err()
    );
    assert!(
        command(json!({"op":"transfer_item","target":"1","item":"a","recipient":"1"}))
            .validate()
            .is_err()
    );
}

#[test]
fn vfx_transform_limits_and_wire() {
    let good = json!({"op":"apply_vfx","target":"1","row":2,"duration":"permanent","scale":1.5,"offset":{"x":1.,"y":2.,"z":3.},"rotation":{"x":10.,"y":20.,"z":30.}});
    let live = store(false);
    live.enqueue(command(good.clone()), 2).unwrap();
    let wire = live.commands(3);
    assert_eq!(wire["commands"][0]["scale"], 1.5);
    assert_eq!(wire["commands"][0]["offset"], good["offset"]);
    assert_eq!(wire["commands"][0]["rotation"], good["rotation"]);
    for (key, value) in [
        ("scale", json!(0)),
        ("scale", json!(101)),
        ("offset", json!({"x":1001,"y":0,"z":0})),
        ("rotation", json!({"x":0,"y":0,"z":3601})),
    ] {
        let mut row = good.clone();
        row[key] = value;
        assert!(command(row).validate().is_err());
    }
}

#[test]
fn session_faction_reputation_works_for_npcs_without_players() {
    for op in ["get_faction_reputation", "set_faction_reputation"] {
        let mut row = json!({"op":op,"target":"1","faction":6});
        if op.starts_with("set_") {
            row["reputation"] = json!(75);
        }
        let live = store(false);
        live.enqueue(command(row.clone()), 2).unwrap();
        let queued = live.commands(3);
        assert_eq!(queued["commands"][0]["faction"], 6);
        assert_eq!(queued["commands"][0]["target"], "1");
        row["target"] = json!("2");
        assert_eq!(
            store(false).enqueue(command(row.clone()), 2).unwrap_err().code,
            ErrorCode::Disabled
        );
        store(true).enqueue(command(row.clone()), 2).unwrap();
        row["target"] = json!("4");
        assert_eq!(store(true).enqueue(command(row), 2).unwrap_err().code, ErrorCode::Invalid);
    }
}

#[test]
fn custom_module_faction_ids_are_not_standard_faction_enums() {
    for id in [0, 5, 8, 42, 65535] {
        let op = json!({"op":"set_faction","target":"1","faction":id});
        let cmd = command(op);
        cmd.validate().unwrap();
        assert_eq!(serde_json::to_value(cmd).unwrap()["faction"], id);
    }
    assert!(command(json!({"op":"set_faction","target":"1","faction":65536})).validate().is_err());
}

#[test]
fn watch_areas_bootstraps_without_area_report_and_validates_wire() {
    let live = LiveStore::default();
    live.sync(br#"{"v":1,"boot":"a","seq":1,"watched_areas":[],"areas":[]}"#, 1).unwrap();
    let cmd: Command =
        serde_json::from_value(json!({"op":"watch_areas","areas":["new_area"],"on":true})).unwrap();
    let id = live.enqueue(cmd, 2).unwrap();
    assert_eq!(
        live.commands(2)["commands"][0],
        json!({"id":id,"op":"watch_areas","areas":["new_area"],"on":true})
    );
    for areas in [
        json!([]),
        json!(["bad/name"]),
        json!(["duplicate", "DUPLICATE"]),
        json!(["x".repeat(17)]),
        json!(vec!["a"; 65]),
    ] {
        let cmd: Command =
            serde_json::from_value(json!({"op":"watch_areas","areas":areas,"on":false})).unwrap();
        assert!(cmd.validate().is_err());
    }
    assert!(
        serde_json::from_value::<Command>(json!({"op":"watch_areas","areas":["a"],"on":1}))
            .is_err()
    );
}

#[test]
fn automatic_watches_replay_on_boot_reconnect_and_release_only_ours() {
    let live = LiveStore::default();
    live.set_watched_areas(&["street".into()], 0).unwrap();
    assert!(live.commands(0)["commands"].as_array().unwrap().is_empty());
    let report = |boot, seq, watched: Value, ack: Value| {
        json!({"v":1,"boot":boot,"seq":seq,"watched_areas":watched,"ack":ack,"areas":[]})
            .to_string()
    };
    let first = live.sync(report("a", 1, json!(["agent_area"]), json!([])).as_bytes(), 1).unwrap();
    let watch = &first["commands"][0];
    assert_eq!(watch["areas"], json!(["street"]));
    assert_eq!(watch["on"], true);
    live.sync(report("a", 2, json!(["street", "agent_area"]), json!([watch["id"]])).as_bytes(), 2)
        .unwrap();
    assert_eq!(live.status(2)["watched_areas"], json!(["street", "agent_area"]));
    live.set_watched_areas(&["street".into()], 3).unwrap();
    assert!(live.commands(3)["commands"].as_array().unwrap().is_empty());
    live.resend_watched_areas(4);
    let replay = live.commands(4)["commands"][0].clone();
    assert_eq!(replay["areas"], json!(["street"]));
    assert_ne!(replay["id"], watch["id"]);
    let reboot = live.sync(report("b", 1, json!([]), json!([])).as_bytes(), 5).unwrap()["commands"]
        [0]
    .clone();
    assert_eq!(reboot["areas"], json!(["street"]));
    assert_ne!(reboot["id"], replay["id"]);
    // Close while activation is in flight: wait for its acknowledgement, then release.
    live.set_watched_areas(&[], 6).unwrap();
    let off = live
        .sync(report("b", 2, json!(["street", "agent_area"]), json!([reboot["id"]])).as_bytes(), 7)
        .unwrap()["commands"][0]
        .clone();
    assert_eq!(off["areas"], json!(["street"]));
    assert_eq!(off["on"], false);
    let done =
        live.sync(report("b", 3, json!(["agent_area"]), json!([off["id"]])).as_bytes(), 8).unwrap();
    assert!(done["commands"].as_array().unwrap().is_empty());
    // Legacy reports continue working without hammering an unsupported native op.
    let old = LiveStore::default();
    old.set_watched_areas(&["street".into()], 0).unwrap();
    assert!(
        old.sync(br#"{"v":1,"boot":"old","areas":[]}"#, 1).unwrap()["commands"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn watch_refusal_retries_without_flood_and_module_reload_discards_old_removals() {
    let live = LiveStore::default();
    live.sync(br#"{"v":1,"boot":"a","seq":1,"watched_areas":[],"areas":[]}"#, 1).unwrap();
    live.set_watched_areas(&["old".into()], 2).unwrap();
    let first = live.commands(2)["commands"][0].clone();
    let failed = json!({"v":1,"boot":"a","seq":2,"watched_areas":[],"areas":[],"results":[{"id":first["id"],"ok":false,"code":"not_found","err":"unknown area"}]});
    live.sync(failed.to_string().as_bytes(), 3).unwrap();
    live.set_watched_areas(&["old".into()], 4).unwrap();
    assert!(live.commands(4)["commands"].as_array().unwrap().is_empty());
    assert_eq!(live.status(4)["watch_error"], "unknown area");
    live.set_watched_areas(&["new".into()], 5).unwrap();
    let reboot =
        live.sync(br#"{"v":1,"boot":"b","seq":1,"watched_areas":[],"areas":[]}"#, 6).unwrap();
    assert_eq!(reboot["commands"][0]["areas"], json!(["new"]));
    assert_eq!(reboot["commands"][0]["on"], true);
}

#[test]
fn watch_areas_remote_scope_is_forbidden_and_local_catalogue_still_validates() {
    assert_eq!(crate::access::group("live_watch_areas"), Some(crate::access::ClientGroup::Read));
    let live = LiveStore::default();
    live.sync(br#"{"v":1,"boot":"a","seq":1,"watched_areas":[],"areas":[]}"#, 1).unwrap();
    let input = json!({"id":"watch-api","areas":["fresh"],"on":true});
    let error = crate::operations::call(
        "live_watch_areas",
        input.clone(),
        &live,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Forbidden);
    let mut local = crate::access::Scope::default();
    local.name = "local".into();
    let response = crate::operations::call_scoped(
        "live_watch_areas",
        input.clone(),
        &live,
        &std::sync::atomic::AtomicBool::new(false),
        &local,
    )
    .unwrap();
    let mut legacy = input;
    legacy["op"] = json!("watch_areas");
    assert_eq!(
        crate::operations::command(legacy.clone(), &live, 2).unwrap_err().code,
        ErrorCode::Forbidden
    );
    assert_eq!(
        crate::operations::command_scoped(legacy.clone(), &live, 2, &local).unwrap(),
        response
    );
    assert_eq!(live.commands(2)["commands"][0], legacy);
    for input in [
        json!({"areas":[],"on":true}),
        json!({"areas":["a"]}),
        json!({"areas":["a"],"on":1}),
        json!({"areas":["a"],"on":true,"unknown":1}),
    ] {
        assert_eq!(
            crate::operations::call_scoped(
                "live_watch_areas",
                input,
                &live,
                &std::sync::atomic::AtomicBool::new(false),
                &local
            )
            .unwrap_err()
            .code,
            ErrorCode::Invalid
        );
    }
}
