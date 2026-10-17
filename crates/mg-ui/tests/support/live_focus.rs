use super::*;

fn picture(h: &mut Harness<'_, Moonglow>, name: &str) {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-focus");
    std::fs::create_dir_all(&out).unwrap();
    h.render().unwrap().save(out.join(format!("{name}.png"))).unwrap();
}

#[test]
fn live_focus_baseline() {
    let Some((mut h, area)) = area_harness("live-focus") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","tag":"lamplighter","name":"Street lamplighter","r":"nw_bartender","x":20,"y":20,"app":7,"hp":[12,20]},
            {"id":"b","tag":"pc_one","name":"Mira","pc":true,"x":24,"y":20,"app":7,"hp":[30,30]},
            {"id":"c","tag":"pc_two","name":"Tomas","pc":true,"x":28,"y":20,"app":7,"hp":[25,30]}
        ]}]}),
    );
    h.state_mut().palette.area = Some(area);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":area.to_string(),"id":"a","text":"The lamps are lit. Welcome!","channel":1},
        {"seq":2,"kind":"chat","area":area.to_string(),"id":"b","text":"Thank you. Have you seen Tomas?","channel":1}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveChat));
    h.run_steps(4);
    if std::env::var("MGLIVE_FOCUS_PICTURE").as_deref() == Ok("before") {
        picture(&mut h, "chat-before");
        review_open_menu(&mut h, area, false);
        picture(&mut h, "menu-before");
    }
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
}

#[test]
fn live_focus_watch_conversation_reply_restore_and_missing() {
    let Some((mut h, area)) = area_harness("live-focus") else { return };
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-focus/data");
    h.state_mut().live_focus = mg_ui::live_focus::LiveFocus::default();
    h.state_mut().live_focus.data_dir = Some(out.clone());
    let module = h.state().module_path();
    h.state_mut().live_focus.context(module.clone());
    h.state_mut().live_focus.entries.clear();
    h.state_mut().live_focus.save().unwrap();
    let cre = serde_json::json!([
        {"id":"a","tag":"lamplighter","name":"Street lamplighter","r":"nw_bartender","x":20,"y":20,"app":7,"hp":[12,20]},
        {"id":"b","tag":"pc_one","name":"Mira","pc":true,"x":24,"y":20,"app":7,"hp":[30,30]},
        {"id":"c","tag":"pc_two","name":"Tomas","pc":true,"x":28,"y":20,"app":7,"hp":[25,30]},
        {"id":"d","tag":"remote","name":"Distant guard","x":200,"y":200}
    ]);
    let store = review_live_start(&mut h, area, serde_json::json!({"areas":[{"cre":cre}]}));
    h.state_mut().palette.area = Some(area);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    review_open_menu(&mut h, area, false);
    picture(&mut h, "menu-after");
    h.get_all_by_label("Focus")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .unwrap()
        .click();
    h.run_steps(4);
    assert_eq!(h.state().live_focus.entries.len(), 1);
    assert_eq!(h.state().live_focus.entries[0].members[0].id, "a");
    h.state_mut().live_focus.add(&store, &["b".into()]).unwrap();
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    view.live.select(Some("b".into()), true);
    view.live.select(Some("c".into()), true);
    review_open_menu(&mut h, area, false);
    h.get_all_by_label("Focus")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .unwrap()
        .click();
    h.run_steps(3);
    assert_eq!(h.state().live_focus.entries[2].members.len(), 3);
    h.state_mut().live_focus.selected = Some(0);
    // Hide Focus while the events arrive: its alert must still update.
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveChat));
    h.run_steps(3);
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":area.to_string(),"id":"a","text":"The lamps are lit. Welcome!","channel":1,"position":{"x":20,"y":20,"z":0}},
        {"seq":2,"kind":"chat","area":area.to_string(),"id":"b","text":"Thank you. Have you seen Tomas?","channel":1,"position":{"x":24,"y":20}},
        {"seq":3,"kind":"chat","area":area.to_string(),"id":"d","text":"Too far away","channel":1,"position":{"x":200,"y":200,"z":0}},
        {"seq":4,"kind":"chat","area":"other","id":"d","text":"Private to Mira","channel":4,"target":"b"},
        {"seq":5,"kind":"chat","area":area.to_string(),"id":"d","text":"Area shout","channel":2},
        {"seq":6,"kind":"chat","area":"other","id":"d","text":"Other area shout","channel":2},
        {"seq":7,"kind":"chat","area":area.to_string(),"id":"a","text":"NW_I_WAS_ATTACKED","channel":14,"silent":true},
        {"seq":8,"kind":"attack","area":area.to_string(),"id":"d","target":"a","roll":10,"modifier":2,"result":4},
        {"seq":9,"kind":"dialog","area":area.to_string(),"id":"a","target":"b","text":"A chosen dialogue line.","speaker_is_pc":false},
        {"seq":10,"kind":"chat","area":area.to_string(),"id":null,"target":"b","channel":5,"text":"A DM message"}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(4);
    let focus = &h.state().live_focus;
    assert!(focus.entries[0].unread >= 4);
    let text = focus.entries[0].rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n");
    assert!(
        text.contains("Have you seen Tomas")
            && text.contains("Area shout")
            && text.contains("chosen dialogue"),
        "{text}"
    );
    assert!(
        !text.contains("Too far")
            && !text.contains("Other area")
            && !text.contains("Private to Mira")
            && !text.contains("NW_"),
        "{text}"
    );
    assert!(focus.entries[1].rows.iter().any(|r| r.text.contains("Private to Mira")));
    assert!(focus.entries[1].rows.iter().any(|r| r.text.contains("A DM message")));
    assert_eq!(focus.entries[2].rows.iter().filter(|r| r.event.n == 2).count(), 1);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveFocus));
    h.run_steps(4);
    assert!(h.query_by_label_contains("Have you seen Tomas").is_none());
    h.get_by_label("Nearby / incoming speech").click();
    h.run_steps(2);
    h.get_by_label_contains("Have you seen Tomas");
    assert_eq!(h.state().live_focus.entries[0].unread, 1, "unseen combat event stays unread");
    picture(&mut h, "conversation-after");
    h.get_by_label("Events").click();
    h.run_steps(3);
    h.get_by_label_contains("misses Street lamplighter");
    assert_eq!(h.state().live_focus.entries[0].unread, 0);
    picture(&mut h, "events-after");
    h.get_by_label("Conversation").click();
    h.run_steps(2);
    h.state_mut().live_focus.text = "Good evening!".into();
    h.run_steps(2);
    h.get_by_label("Send reply").click();
    h.run_steps(2);
    assert!(
        store.status(mg_live::now_ms())["queued"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["op"] == "npc_say" && c["target"] == "a" && c["text"] == "Good evening!")
    );
    h.get_by_label("Group 1").click();
    h.run_steps(2);
    h.state_mut().live_focus.text = "Please meet at the gate.".into();
    h.run_steps(2);
    h.get_by_label("Send reply").click();
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"].clone();
    let targets: Vec<_> = q
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["op"] == "pc_message")
        .map(|c| c["target"].as_str().unwrap())
        .collect();
    assert_eq!(targets, ["b", "c"]);
    let replies: Vec<_> = q
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["op"] == "pc_message")
        .map(|c| serde_json::json!({"id":c["id"],"ok":true}))
        .collect();
    store
        .sync(
            serde_json::json!({"v":1,"boot":"accept2","seq":3,"areas":[],"results":replies})
                .to_string()
                .as_bytes(),
            mg_live::now_ms(),
        )
        .unwrap();
    h.run_steps(3);
    assert!(
        h.state().live_focus.entries[1]
            .rows
            .iter()
            .any(|r| r.text == "DM (command acknowledged): Please meet at the gate.")
    );

    store.set_policy(mg_ui::live::Policy { players: false, ..Default::default() });
    h.state_mut().live_focus.text = "Must not send".into();
    h.run_steps(2);
    assert!(h.get_by_label("Send reply").accesskit_node().is_disabled());
    store.set_policy(mg_ui::live::Policy { players: true, ..Default::default() });
    // Rename and restore in a new instance, with history deliberately ephemeral.
    h.get_by_label("Manage").click();
    h.run_steps(3);
    let input =
        |n: &egui_kittest::Node<'_>| n.accesskit_node().role() == egui::accesskit::Role::TextInput;
    h.get_all_by_value("Group 1").find(input).unwrap().click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.get_all_by_value("Group 1").find(input).unwrap().type_text("Gate party");
    h.run_steps(2);
    h.get_by_label("Rename").click();
    h.run_steps(3);
    assert_eq!(h.state().live_focus.entries[2].name, "Gate party");
    let saved = h.state().live_focus.path().unwrap();
    let bytes = std::fs::read(&saved).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("Good evening"));
    let mut restored = mg_ui::live_focus::LiveFocus::default();
    restored.data_dir = Some(out);
    restored.context(module);
    assert_eq!(restored.entries[2].name, "Gate party");
    assert!(
        restored.entries.iter().all(|e| e.rows.is_empty() && e.members.iter().all(|m| !m.reported))
    );
    h.state_mut().live_focus = restored;
    h.run_steps(3);
    let mut missing = cre.clone();
    missing.as_array_mut().unwrap().retain(|o| o["id"] != "a");
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":4,"areas":[{"area":area.to_string(),"full":true,"cre":missing}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(3);
    assert!(!h.state().live_focus.entries[0].members[0].reported);
    picture(&mut h, "missing-after");
    let mut returned = cre;
    returned[0]["id"] = serde_json::json!("e");
    returned[0]["x"] = serde_json::json!(30);
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":5,"areas":[{"area":area.to_string(),"full":true,"cre":returned}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().live_focus.entries[0].members[0].id, "e");
    h.state_mut().live_focus.selected = Some(0);
    h.run_steps(2);
    h.get_by_label("Show on map").click();
    h.run_steps(3);
    assert_eq!(h.state().area_views[&area].orbit.as_ref().unwrap().target.x, 30.);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveFocus));
    h.run_steps(3);
    h.get_by_label("Manage").click();
    h.run_steps(2);
    h.get_by_label("Remove from focus").click();
    h.run_steps(3);
    assert_eq!(h.state().live_focus.entries.len(), 2);
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_focus_options_default_cancel_apply_and_menu() {
    let Some((mut h, _area)) = area_harness("live-focus") else { return };
    assert!(!h.state().settings.no_live_focus);
    h.state_mut().actions.push(mg_ui::Action::LiveOptions);
    h.run_steps(4);
    picture(&mut h, "options-after");
    h.get_by_label("Focus window").click();
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(!h.state().settings.no_live_focus);
    h.state_mut().actions.push(mg_ui::Action::LiveOptions);
    h.run_steps(3);
    h.get_by_label("Focus window").click();
    h.run_steps(2);
    h.get_by_label("Apply").click();
    h.run_steps(2);
    assert!(h.state().settings.no_live_focus);
    let encoded = serde_json::to_string(&h.state().settings).unwrap();
    let restored: mg_ui::Settings = serde_json::from_str(&encoded).unwrap();
    assert!(restored.no_live_focus);
    let defaults: mg_ui::Settings = serde_json::from_str("{}").unwrap();
    assert!(!defaults.no_live_focus);
    h.get_all_by_label("Live").next().unwrap().click();
    h.run_steps(2);
    h.get_by_label("Focus…").click();
    h.run_steps(3);
    h.get_by_label("Focus is off. Enable Focus window in Options > Live.");
}
