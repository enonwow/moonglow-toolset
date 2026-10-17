use super::*;

fn picture(h: &mut Harness<'_, Moonglow>, name: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-audit7")
        .join(format!("{name}.png"));
    if name.ends_with("before") && path.exists() {
        return;
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    h.render().unwrap().save(&path).unwrap();
    std::fs::write(path.with_file_name("fixture.json"),serde_json::to_vec_pretty(&serde_json::json!({"module":h.state().module_path(),"data_dir":h.state().live_assistant.data_dir})).unwrap()).unwrap();
}

#[test]
fn live_audit7_baseline() {
    if std::env::var_os("MGLIVE_AUDIT7_BEFORE").is_none() {
        return;
    }
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","tag":"watch_captain","name":"Watch captain","app":6,"x":20,"y":20},
            {"id":"b","tag":"traveller","name":"Traveller","pc":true,"app":6,"x":22,"y":20}
        ]}]}),
    );
    h.state_mut().palette.area = Some(area);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    h.state_mut().live_focus.add(&store, &["a".into()]).unwrap();
    for (tab, name) in [
        (Tab::LiveChat, "chat-before"),
        (Tab::LiveFocus, "focus-before"),
        (Tab::LiveSelection, "selection-before"),
        (Tab::Scene, "scenes-before"),
    ] {
        h.state_mut().actions.push(mg_ui::Action::OpenTab(tab));
        h.run_steps(4);
        picture(&mut h, name);
    }
    h.state_mut().settings.live.clients.push(mg_live::access::ClientToken {
        name: "Storyteller".into(),
        token: "t".repeat(32),
        ..Default::default()
    });
    h.state_mut().actions.push(mg_ui::Action::LiveOptions);
    h.run_steps(3);
    h.get_by_label("Named clients").click();
    h.run_steps(3);
    h.get_by_label("Add client").scroll_to_me();
    h.run_steps(3);
    picture(&mut h, "clients-list-before");
}

fn agent_call(app: &Moonglow, op: &str, input: serde_json::Value) -> serde_json::Value {
    use std::{
        io::{Read, Write},
        sync::Arc,
    };
    let service = app.live_service.as_ref().unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(service.certificate().to_vec())).unwrap();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let client =
        rustls::ClientConnection::new(Arc::new(config), "moonglow-live".try_into().unwrap())
            .unwrap();
    let socket = std::net::TcpStream::connect(("127.0.0.1", service.config().api_port)).unwrap();
    socket.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
    let mut tls = rustls::StreamOwned::new(client, socket);
    let body = input.to_string();
    write!(tls,"POST /v1/call/{op} HTTP/1.1\r\nHost: moonglow-live\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}","s".repeat(32),body.len()).unwrap();
    tls.flush().unwrap();
    let mut response = String::new();
    tls.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn live_audit7_drafts_require_review_in_chat_and_focus() {
    use mg_live::access::{ClientGroup, ClientToken, DeliveryMode};
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(
        &mut h,
        area,
        json!({"areas":[{"cre":[
            {"id":"a","tag":"watch_captain","name":"Watch captain","app":6,"x":20,"y":20},
            {"id":"b","tag":"traveller","name":"Traveller","pc":true,"app":6,"x":22,"y":20}
        ]}]}),
    );
    h.state_mut().settings.live.allow_agents = true;
    h.state_mut().settings.live.clients = vec![ClientToken {
        name: "Storyteller".into(),
        token: "s".repeat(32),
        groups: vec![ClientGroup::Read, ClientGroup::ReadChat, ClientGroup::Npc],
        delivery: DeliveryMode::Drafts,
        ..Default::default()
    }];
    h.state_mut().apply_live(&egui::Context::default());
    h.state_mut().palette.area = Some(area);
    h.state_mut().live_focus.add(&store, &["a".into()]).unwrap();
    store.sync(json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":"field","id":"a","name":"Watch captain","text":"Who goes there?","channel":0},
        {"seq":2,"kind":"chat","area":"field","id":"b","name":"Traveller","text":"May I pass?","channel":0,"pc":true}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    let result = agent_call(
        h.state(),
        "live_npc_say",
        json!({"id":"proposal","area":"field","target":"a","text":"Welcome. The eastern gate is open."}),
    );
    assert!(result["ok"].as_bool().unwrap(), "{result}");
    assert_eq!(store.drafts().len(), 1);
    assert_eq!(store.commands(mg_live::now_ms())["commands"], json!([]));
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveChat));
    h.run_steps(4);
    h.get_by_label("LLM drafts (1)");
    picture(&mut h, "chat-drafts-after");
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveFocus));
    h.run_steps(4);
    h.get_by_label("LLM drafts (1)");
    picture(&mut h, "focus-drafts-after");
    h.get_by_label("Edit draft").click();
    h.run_steps(2);
    h.get_by_label("Edit proposed message").click();
    h.get_by_label("Edit proposed message").type_text(" Bring your papers.");
    h.run_steps(2);
    h.get_by_label("Send draft").click();
    h.run_steps(3);
    let commands = store.commands(mg_live::now_ms());
    assert_eq!(commands["commands"][0]["by"], "Storyteller");
    let native = commands["commands"][0]["id"].as_str().unwrap();
    assert!(commands["commands"][0]["text"].as_str().unwrap().contains("Bring your papers."));
    store
        .sync(
            json!({"v":1,"boot":"accept2","seq":3,"areas":[],"results":[{"id":native,"ok":true}]})
                .to_string()
                .as_bytes(),
            mg_live::now_ms(),
        )
        .unwrap();
    assert_eq!(store.result("proposal").unwrap().by.as_deref(), Some("Storyteller"));
    assert_eq!(store.result("proposal").unwrap().value.unwrap()["draft"]["outcome"], "edited");
    let wait = agent_call(h.state(), "live_wait", json!({"id":"proposal","timeout_ms":0}));
    assert_eq!(wait["result"]["value"]["draft"]["outcome"], "edited");
    for id in ["reject", "batch1", "batch2"] {
        agent_call(
            h.state(),
            "live_npc_say",
            json!({"id":id,"area":"field","target":"a","text":id}),
        );
    }
    h.run_steps(3);
    h.get_all_by_label("Reject draft").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(store.result("reject").unwrap().value.unwrap()["draft"]["outcome"], "rejected");
    h.get_by_label("Send all drafts").click();
    h.run_steps(3);
    assert!(store.drafts().is_empty());
    assert_eq!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len(), 2);
    agent_call(
        h.state(),
        "live_npc_say",
        json!({"id":"keyboard","area":"field","target":"a","text":"Approved with Enter"}),
    );
    h.run_steps(3);
    h.ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    h.get_by_label("LLM drafts (1)").hover();
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(store.drafts().is_empty(), "Enter approves the top draft in the pointed-at panel");
    h.state_mut().actions.push(mg_ui::Action::LiveOptions);
    h.run_steps(3);
    h.get_by_label("Named clients").click();
    h.run_steps(2);
    h.get_by_label("Agent speech").scroll_to_me();
    h.run_steps(3);
    assert_eq!(h.state().options.as_ref().unwrap().live.clients[0].delivery, DeliveryMode::Drafts);
    picture(&mut h, "clients-drafts-after");
    h.get_by_label("Auto").click();
    h.run_steps(2);
    assert_eq!(h.state().options.as_ref().unwrap().live.clients[0].delivery, DeliveryMode::Auto);
}

#[test]
fn live_audit7_cards_and_memory_are_saved_and_readable_by_the_agent() {
    use mg_live::{
        access::{ClientGroup, ClientToken},
        store::assistant::cards::{GmCard, NpcCard},
    };
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(
        &mut h,
        area,
        json!({"areas":[{"cre":[
            {"id":"a","tag":"watch_captain","name":"Watch captain","app":6,"x":20,"y":20},
            {"id":"b","tag":"traveller","name":"Traveller","pc":true,"app":6,"x":22,"y":20}
        ]}]}),
    );
    h.state_mut().settings.live.allow_agents = true;
    h.state_mut().settings.live.clients = vec![ClientToken {
        name: "Storyteller".into(),
        token: "s".repeat(32),
        groups: vec![ClientGroup::Read, ClientGroup::ReadChat, ClientGroup::Npc],
        ..Default::default()
    }];
    h.state_mut().apply_live(&egui::Context::default());
    h.run_steps(3);
    store.save_gm_card(GmCard::default()).unwrap();
    store
        .save_npc_card("a", NpcCard { name: "Watch captain".into(), ..Default::default() })
        .unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveGmCard));
    h.run_steps(3);
    h.get_by_label("Campaign tone").click();
    h.get_by_label("Campaign tone").type_text("Quiet intrigue; let players discover the truth.");
    h.run_steps(2);
    h.get_by_label("Save card").click();
    h.run_steps(3);
    assert_eq!(
        store.gm_card().unwrap().campaign_tone,
        "Quiet intrigue; let players discover the truth."
    );
    picture(&mut h, "gm-card-after");
    h.state_mut().palette.area = Some(area);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveSelection));
    h.run_steps(3);
    picture(&mut h, "notes-selection-before");
    h.get_by_label("NPC card").click();
    h.run_steps(3);
    h.get_by_label("Personality").click();
    h.get_by_label("Personality").type_text("Patient, wary, protective of the city.");
    h.run_steps(2);
    h.get_by_label("Save card").click();
    h.run_steps(3);
    assert_eq!(store.npc_card("a").unwrap().personality, "Patient, wary, protective of the city.");
    picture(&mut h, "npc-card-after");
    assert_eq!(
        agent_call(h.state(), "live_gm_card", json!({}))["result"]["card"]["campaign_tone"],
        store.gm_card().unwrap().campaign_tone
    );
    assert_eq!(
        agent_call(h.state(), "live_npc_card", json!({"npc":"a"}))["result"]["card"]["id"],
        "tag:watch_captain"
    );
    store.sync(json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":"field","id":"b","target":"a","text":"May I pass?","channel":1},
        {"seq":2,"kind":"chat","area":"field","id":"a","target":"b","text":"Show your papers first.","channel":1}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    let memory =
        agent_call(h.state(), "live_npc_memory", json!({"npc":"a","player":"b","limit":2}));
    assert_eq!(memory["result"]["conversations"][0]["turns"].as_array().unwrap().len(), 2);
    assert_eq!(
        agent_call(
            h.state(),
            "live_npc_memory_note",
            json!({"npc":"a","player":"b","summary":"Asked to enter; must show papers."})
        )["result"]["saved"],
        true
    );
    h.state_mut().live_focus.add(&store, &["a".into()]).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveFocus));
    h.run_steps(3);
    picture(&mut h, "notes-focus-before");
    h.get_by_label("Memory").click();
    h.run_steps(3);
    h.get_by_label_contains("Traveller (").click();
    h.run_steps(3);
    h.get_by_label("Summary: Asked to enter; must show papers.");
    picture(&mut h, "focus-memory-after");
    store.flush_npc_memory().unwrap();
    let folder = h.state().live_assistant.data_dir.clone().unwrap();
    let module = h.state().module_path().unwrap();
    store.clear_assistant_context();
    store.assistant_context(folder, module, "127.0.0.1:1:mglive".into()).unwrap();
    assert_eq!(store.npc_card("a").unwrap().personality, "Patient, wary, protective of the city.");
    assert_eq!(
        store.npc_memory("a", Some("b"), 1).unwrap()[0].summary,
        "Asked to enter; must show papers."
    );
}

#[test]
fn live_audit7_drag_baseline() {
    if std::env::var_os("MGLIVE_AUDIT7_DRAG_BEFORE").is_none() {
        return;
    }
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
        {"id":"a","name":"Watch captain","tag":"watch_captain","x":20,"y":20},
        {"id":"b","name":"Traveller","pc":true,"x":22,"y":20}],
        "plc":[{"id":"c","name":"Crate","x":20,"y":23}],"items":[{"id":"d","name":"Dropped key","x":22,"y":23}]}]}),
    );
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    for (i, id) in ["a", "c", "d"].into_iter().enumerate() {
        view.live.select(Some(id.into()), i > 0);
    }
    h.run_steps(3);
    let from = screen(&h, area, glam::vec3(20., 20., 0.3));
    let to = screen(&h, area, glam::vec3(25., 25., 0.3));
    press(&h, from, true, egui::Modifiers::NONE);
    for n in 1..=4 {
        h.hover_at(from + (to - from) * (n as f32 / 4.));
        h.run_steps(1);
    }
    picture(&mut h, "drag-before");
    press(&h, to, false, egui::Modifiers::NONE);
    h.run_steps(2);
}

#[test]
fn live_audit7_drag_releases_once_and_preserves_group_offsets() {
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(
        &mut h,
        area,
        json!({"areas":[{"cre":[
        {"id":"a","name":"Watch captain","tag":"watch_captain","x":20,"y":20},
        {"id":"b","name":"Traveller","pc":true,"x":22,"y":20}],
        "plc":[{"id":"c","name":"Crate","x":20,"y":23}],"items":[{"id":"d","name":"Dropped key","x":22,"y":23}]}]}),
    );
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    for (i, id) in ["a", "c", "d"].into_iter().enumerate() {
        view.live.select(Some(id.into()), i > 0);
    }
    h.run_steps(3);
    let from = screen(&h, area, glam::vec3(20., 20., 0.3));
    let to = screen(&h, area, glam::vec3(25., 25., 0.3));
    press(&h, from, true, egui::Modifiers::NONE);
    for n in 1..=4 {
        h.hover_at(from + (to - from) * (n as f32 / 4.));
        h.run_steps(1);
    }
    assert_eq!(store.commands(mg_live::now_ms())["commands"], json!([]), "Dragging only previews");
    picture(&mut h, "drag-after");
    press(&h, to, false, egui::Modifiers::NONE);
    h.run_steps(3);
    let queued = store.commands(mg_live::now_ms());
    let commands = queued["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 3, "{queued}");
    for (id, x, y, op) in
        [("a", 25., 25., "teleport"), ("c", 25., 28., "set_position"), ("d", 27., 28., "teleport")]
    {
        let command = commands.iter().find(|c| c["target"] == id).unwrap();
        assert_eq!(command["op"], op);
        assert!((command["x"].as_f64().unwrap() - x).abs() < 0.1);
        assert!((command["y"].as_f64().unwrap() - y).abs() < 0.1);
    }
    h.run_steps(4);
    assert_eq!(
        store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len(),
        3,
        "No repeats after release"
    );
    press(&h, from, true, egui::Modifiers::NONE);
    h.hover_at(to);
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    press(&h, to, false, egui::Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(
        store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len(),
        3,
        "Escape cancels the group move"
    );
    assert!(!h.state().has_unsaved_work(), "Live dragging never edits the module");
    let before = h.state().area_views[&area].orbit.as_ref().unwrap().target;
    let ctrl = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
    h.hover_at(from);
    press(&h, from, true, ctrl);
    h.run_steps(1);
    h.event_modifiers(egui::Event::PointerMoved(to), ctrl);
    h.run_steps(3);
    press(&h, to, false, ctrl);
    h.run_steps(2);
    assert_ne!(
        h.state().area_views[&area].orbit.as_ref().unwrap().target,
        before,
        "Ctrl drag still pans"
    );
    assert_eq!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len(), 3);
}

#[test]
fn live_audit7_notes_share_selection_focus_and_search_without_module_edits() {
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(
        &mut h,
        area,
        json!({"areas":[{"cre":[
        {"id":"a","tag":"watch_captain","name":"Watch captain","app":6,"x":20,"y":20},
        {"id":"b","tag":"traveller","name":"Traveller","pc":true,"app":6,"x":22,"y":20}],
        "plc":[{"id":"c","tag":"locked_chest","name":"Chest","x":24,"y":20}]}]}),
    );
    h.state_mut().palette.area = Some(area);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveSelection));
    h.run_steps(3);
    h.get_by_label("Note text").click();
    h.get_by_label("Note text").type_text("Knows the harbour password: silver gull.");
    h.run_steps(2);
    h.get_by_label("Save note").click();
    h.run_steps(3);
    assert_eq!(store.object_note("a").unwrap(), "Knows the harbour password: silver gull.");
    picture(&mut h, "notes-selection-after");
    store.save_object_note("b", "Owes the captain a favour").unwrap();
    store.save_object_note("c", "Contains the missing ledger").unwrap();
    assert_eq!(store.area_notes(&area.to_string()).len(), 3);
    h.state_mut().live_focus.add(&store, &["a".into()]).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveFocus));
    h.run_steps(3);
    h.get_by_label("DM note").click();
    h.run_steps(3);
    h.get_by_label("Note text");
    picture(&mut h, "notes-focus-after");
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LivePanel));
    h.run_steps(3);
    h.get_by_label("Search live objects").click();
    h.get_by_label("Search live objects").type_text("silver gull");
    h.run_steps(3);
    h.get_by_label("Watch captain");
    assert!(h.query_by_label("Traveller").is_none());
    picture(&mut h, "notes-panel-after");
    // A saved note change updates filtering without a new report.
    store.save_object_note("a", "Changed clue").unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Watch captain").is_none());
    store.save_object_note("a", "silver gull").unwrap();
    h.run_steps(3);
    h.get_by_label("Watch captain");
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::Area(area)));
    h.run_steps(3);
    assert!(!h.state().has_unsaved_work());
    let folder = h.state().live_assistant.data_dir.clone().unwrap();
    let module = h.state().module_path().unwrap();
    let rev = store.assistant_revision().0;
    store.assistant_context(folder.clone(), module.clone(), "another-server".into()).unwrap();
    assert!(store.assistant_revision().0 > rev);
    assert!(store.area_notes(&area.to_string()).is_empty());
    store.assistant_context(folder, module, "127.0.0.1:1:mglive".into()).unwrap();
    assert_eq!(store.object_note("b").unwrap(), "Owes the captain a favour");
    assert_eq!(store.object_note("c").unwrap(), "Contains the missing ledger");
}

#[test]
fn live_audit7_macro_hotkey_is_saved_configurable_and_never_repeats_while_held() {
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(
        &mut h,
        area,
        json!({"areas":[{"cre":[{"id":"a","tag":"captain","name":"Watch captain","x":20,"y":20}]}]}),
    );
    let dir = h.state().live_assistant.data_dir.clone().unwrap();
    let module = h.state().module_path().unwrap().file_stem().unwrap().to_str().unwrap().to_owned();
    let scene = mg_live::scene::Scene {
        name: "Greeting".into(),
        steps: vec![mg_live::scene::Step {
            op: "live_npc_say".into(),
            input: json!({"target":"a","text":"Welcome to the city.","area":area.to_string()}),
            delay_ms: 0,
            wait: mg_live::scene::Wait::Nothing,
        }],
    };
    mg_live::scene::save(&dir, &module, &scene).unwrap();
    h.state_mut().live_scene = Default::default();
    h.state_mut().live_scene.data_dir = Some(dir.clone());
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::Scene));
    h.run_steps(3);
    h.get_by_label("Assign key...").click();
    h.run_steps(2);
    h.get_by_label_contains("Slot 1:").click();
    h.run_steps(3);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(dir.join("live/scene-keys").join(format!("{module}.json"))).unwrap()
        )
        .unwrap()[0],
        "Greeting"
    );
    picture(&mut h, "scenes-hotkeys-after");
    // Reload all UI state: assignment survives and the key is configurable.
    h.state_mut().live_scene = Default::default();
    h.state_mut().live_scene.data_dir = Some(dir);
    h.state_mut().keymap.set(
        mg_ui::keys::Cmd::LiveMacro1,
        vec![egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::F6)],
    );
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::Area(area)));
    h.run_steps(3);
    let at = screen(&h, area, glam::vec3(25., 25., 0.));
    h.hover_at(at);
    h.run_steps(2);
    h.event_modifiers(
        egui::Event::Key {
            key: egui::Key::F6,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        },
        egui::Modifiers::ALT,
    );
    h.run_steps(4);
    for _ in 0..30 {
        if !store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let commands = store.commands(mg_live::now_ms());
    assert_eq!(commands["commands"].as_array().unwrap().len(), 1, "{commands}");
    assert_eq!(commands["commands"][0]["text"], "Welcome to the city.");
    h.event_modifiers(
        egui::Event::Key {
            key: egui::Key::F6,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: egui::Modifiers::ALT,
        },
        egui::Modifiers::ALT,
    );
    h.run_steps(4);
    assert_eq!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len(), 1);
}

#[test]
fn live_audit7_atmosphere_uses_area_tabs_and_waits_for_server_readback() {
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(&mut h, area, json!({"areas":[{"cre":[]}]}));
    store.sync(json!({"v":1,"boot":"accept2","seq":2,"areas":[{"area":"harbour","cre":[],"plc":[],"doors":[],"items":[]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveAtmosphere));
    h.run_steps(3);
    h.get_by_label("harbour").click();
    h.run_steps(2);
    for title in ["Weather", "Fog"] {
        h.get_all_by_label(title).last().unwrap().click();
        h.run_steps(2);
    }
    h.get_by_label("Apply weather").click();
    h.run_steps(2);
    let queued = store.commands(mg_live::now_ms());
    assert_eq!(queued["commands"][0]["area"], "harbour");
    assert_eq!(queued["commands"][0]["op"], "set_weather");
    let id = queued["commands"][0]["id"].as_str().unwrap();
    store.sync(json!({"v":1,"boot":"accept2","seq":3,"areas":[],"results":[{"id":id,"ok":true,"value":{"weather":0,"fog_day":0}}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(3);
    h.get_by_label("Server accepted atmosphere change");
    h.get_by_label("Apply fog").click();
    h.run_steps(2);
    let queued = store.commands(mg_live::now_ms());
    assert_eq!(queued["commands"][0]["op"], "set_fog");
    assert_eq!(queued["commands"][0]["color"], 0x808080);
    let fog_id = queued["commands"][0]["id"].as_str().unwrap();
    store.sync(json!({"v":1,"boot":"accept2","seq":4,"areas":[],"results":[{"id":fog_id,"ok":true,"value":{"fog_color_day":8421504,"fog_day":0}}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(3);
    picture(&mut h, "atmosphere-after");
    h.get_by_label("Field").click();
    h.run_steps(2);
    h.get_by_label("Apply weather").click();
    h.run_steps(2);
    let queued = store.commands(mg_live::now_ms());
    assert!(
        queued["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["area"] == area.to_string() && c["op"] == "set_weather")
    );
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_audit7_conversation_profile_and_visible_agent_attribution() {
    use mg_live::access::{ClientGroup, DeliveryMode};
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(
        &mut h,
        area,
        json!({"areas":[{"cre":[{"id":"a","tag":"captain","name":"Watch captain","app":6,"x":20,"y":20}]}]}),
    );
    h.state_mut().live_focus.add(&store, &["a".into()]).unwrap();
    store.sync(json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[{"seq":1,"kind":"chat","area":area.to_string(),"id":"a","name":"Watch captain","text":"The watch is listening.","channel":1,"by":"Storyteller","command_id":"chat-proof"}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    for (tab, file) in [
        (Tab::LiveChat, "chat-agent-after"),
        (Tab::LiveFocus, "focus-agent-after"),
        (Tab::LiveLog, "log-agent-after"),
    ] {
        h.state_mut().actions.push(mg_ui::Action::OpenTab(tab));
        h.run_steps(3);
        h.get_by_label_contains("by LLM: Storyteller");
        picture(&mut h, file);
    }
    h.state_mut().actions.push(mg_ui::Action::LiveOptions);
    h.run_steps(3);
    h.get_by_label("Named clients").click();
    h.run_steps(3);
    h.get_by_label("Create conversation LLM client").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Create conversation LLM client").click();
    h.run_steps(3);
    let clients = &h.state().options.as_ref().unwrap().live.clients;
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].name, "Conversation 1");
    assert_eq!(clients[0].delivery, DeliveryMode::Drafts);
    assert_eq!(clients[0].per_minute, 60);
    assert_eq!(
        clients[0].groups,
        vec![ClientGroup::Read, ClientGroup::ReadChat, ClientGroup::Speech]
    );
    assert!(clients[0].token.len() >= 32);
    h.get_by_label("Agent speech").scroll_to_me();
    h.run_steps(3);
    picture(&mut h, "conversation-client-after");
}

#[test]
fn live_audit7_world_palette_before() {
    if std::env::var_os("MGLIVE_AUDIT7_WORLD_BEFORE").is_none() {
        return;
    }
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    review_live_start(&mut h, area, serde_json::json!({"areas":[{"cre":[]}]}));
    h.state_mut().palette.selected = Some(ResKey::parse("nw_waypoint001", ResType::UTW).unwrap());
    h.run_steps(3);
    let at = screen(&h, area, glam::vec3(25., 25., 0.));
    h.hover_at(at);
    h.run_steps(2);
    picture(&mut h, "world-palette-before");
}

#[test]
fn live_audit7_world_palette_aims_rectangles_and_removes_supported_kinds() {
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(&mut h, area, json!({"areas":[{"cre":[]}]}));
    let at = screen(&h, area, glam::vec3(25., 25., 0.));
    for (ty, kind) in [
        (ResType::UTW, "waypoint"),
        (ResType::UTS, "sound"),
        (ResType::UTE, "encounter"),
        (ResType::UTM, "store"),
    ] {
        let key = ResKey::parse("audit_template", ty).unwrap();
        h.state_mut().palette.selected = Some(key);
        h.run_steps(2);
        h.hover_at(at);
        h.run_steps(2);
        assert!(
            matches!(h.state().area_views[&area].live.aim,Some(mg_ui::live_view::Aim::Spawn(k)) if k==key)
        );
        let before = store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len();
        if ty == ResType::UTW {
            picture(&mut h, "world-palette-after");
        }
        press(&h, at, true, egui::Modifiers::NONE);
        press(&h, at, false, egui::Modifiers::NONE);
        h.run_steps(2);
        let queued = store.commands(mg_live::now_ms());
        assert_eq!(queued["commands"].as_array().unwrap().len(), before + 1, "{queued}");
        assert_eq!(queued["commands"].as_array().unwrap().last().unwrap()["kind"], kind);
    }
    h.state_mut().palette.selected = Some(ResKey::parse("audit_trigger", ResType::UTT).unwrap());
    h.run_steps(2);
    let to = screen(&h, area, glam::vec3(28., 29., 0.));
    press(&h, at, true, egui::Modifiers::NONE);
    for i in 1..=4 {
        h.hover_at(at + (to - at) * (i as f32 / 4.));
        h.run_steps(1);
    }
    assert_eq!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len(), 4);
    picture(&mut h, "world-trigger-after");
    press(&h, to, false, egui::Modifiers::NONE);
    h.run_steps(3);
    let queued = store.commands(mg_live::now_ms());
    let trigger = queued["commands"].as_array().unwrap().last().unwrap();
    assert_eq!(trigger["op"], "spawn_trigger");
    assert_eq!(trigger["r"], "audit_trigger");
    assert!((trigger["width"].as_f64().unwrap() - 3.).abs() < 0.1);
    assert!((trigger["height"].as_f64().unwrap() - 4.).abs() < 0.1);
    // A second trigger can be cancelled during the drag without sending.
    h.state_mut().palette.selected = Some(ResKey::parse("audit_trigger", ResType::UTT).unwrap());
    h.run_steps(2);
    press(&h, at, true, egui::Modifiers::NONE);
    h.hover_at(to);
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    press(&h, to, false, egui::Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len(), 5);
    store.sync(json!({"v":1,"boot":"accept2","seq":2,"areas":[{"area":area.to_string(),"full":true,
 "waypoints":[{"id":"a","name":"Meeting point","x":15,"y":25}],"sounds":[{"id":"b","name":"Birds","x":20,"y":25}],
 "encounters":[{"id":"c","name":"Watch patrol","x":25,"y":25}],"stores":[{"id":"d","name":"Supplies","x":30,"y":25}],"triggers":[{"id":"e","name":"Gate alarm","x":35,"y":25}]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    let v = h.state_mut().area_views.get_mut(&area).unwrap();
    v.live.labels = mg_ui::live_view::LabelMode::All;
    for (i, id) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
        v.live.select(Some(id.into()), i > 0);
    }
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveSelection));
    h.run_steps(3);
    picture(&mut h, "world-reports-after");
    h.hover_at(at);
    h.key_press(egui::Key::Delete);
    h.run_steps(3);
    h.get_by_label("Remove 4 objects").click();
    h.run_steps(3);
    let queued = store.commands(mg_live::now_ms());
    let removed: Vec<_> =
        queued["commands"].as_array().unwrap().iter().filter(|c| c["op"] == "destroy").collect();
    assert_eq!(removed.len(), 4, "{queued}");
    assert!(!removed.iter().any(|c| c["target"] == "c"));
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("c".into()), false);
    h.hover_at(at);
    h.key_press(egui::Key::Delete);
    h.run_steps(3);
    h.get_by_label_contains("Encounter removal is unavailable");
    picture(&mut h, "world-encounter-limitation-after");
    assert!(!h.state().has_unsaved_work());
}
