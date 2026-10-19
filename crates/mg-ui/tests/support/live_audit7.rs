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
    std::fs::write(
        path.with_file_name("fixture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"module":h.state().module_path()})).unwrap(),
    )
    .unwrap();
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
fn live_audit7_macro_hotkey_is_saved_configurable_and_never_repeats_while_held() {
    use serde_json::json;
    let Some((mut h, area)) = area_harness("live-audit7") else { return };
    let store = review_live_start(
        &mut h,
        area,
        json!({"areas":[{"cre":[{"id":"a","tag":"captain","name":"Watch captain","x":20,"y":20}]}]}),
    );
    let dir = mg_testkit::scratch_dir("ui-audit7-macros");
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
