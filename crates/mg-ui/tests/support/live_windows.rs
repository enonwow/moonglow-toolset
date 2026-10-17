//! Authored fixtures and fake-store checks; no service or game process is started.
#![allow(dead_code)]
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use mg_core::{Language, ResRef, ResType};
use mg_gff::{Gff, Struct, Value as GValue};
use mg_live::store::LiveStore;
use mg_resman::ResKey;
use mg_ui::{Action, Moonglow, NoDialogs, Tab};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) fn output() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output/live-windows")
}
pub(crate) fn area() -> ResRef {
    "start".parse().unwrap()
}

pub(crate) fn snapshot() -> Value {
    json!({"__data_type":"UTC ","Appearance_Type":{"type":"word","value":6},"Gender":{"type":"byte","value":0},"Tag":{"type":"cexostring","value":"library_guard"}})
}

pub(crate) fn report(store: &LiveStore, seq: u64, results: Vec<Value>) {
    store.sync(json!({"v":1,"boot":"windows","seq":seq,"results":results,"areas":[{"area":"start","full":true,"cre":[
        {"id":"7","name":"Hexxar's guard","gold":125,"r":"nw_bartender","x":3,"y":5,"z":0,"app":0},
        {"id":"8","name":"Second guard","r":"nw_bartender","x":7,"y":5,"z":0,"app":0},
        {"id":"9","name":"Player Mira","pc":true,"x":4,"y":5,"z":0}]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
}
pub(crate) fn commands(store: &LiveStore) -> Vec<Value> {
    store.commands(mg_live::now_ms())["commands"].as_array().unwrap().clone()
}

pub(crate) fn inventory_reply(h: &mut Harness<'_, Moonglow>, value: Value) {
    let store = h.state().live.as_ref().unwrap().clone();
    let pending = commands(&store);
    let query =
        pending.iter().find(|c| c["op"] == "view_inventory").expect("automatic inventory request");
    report(&store, 2, vec![json!({"id":query["id"],"ok":true,"value":value})]);
    h.run_steps(2);
}

pub(crate) fn app(case: &str, game_root: Option<&Path>) -> (Moonglow, Arc<LiveStore>, PathBuf) {
    let dir = output().join(case);
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join(".hexxar-windows");
    if marker.exists() {
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "hexxar-windows-v1");
    } else {
        std::fs::write(&marker, "hexxar-windows-v1").unwrap();
    }
    let mut module = mg_module::Module::new();
    let mut info = Gff::new(*b"IFO ");
    let mut entry = Struct::new(6);
    entry.set("Area_Name", GValue::ResRef(b"start".to_vec()));
    info.root.set("Mod_Area_list", GValue::List(vec![entry]));
    module.set_info(&info).unwrap();
    let mut are = Gff::new(*b"ARE ");
    are.root.set("Tileset", GValue::ResRef(b"ttr01".to_vec()));
    are.root.set("Width", GValue::Int(2));
    are.root.set("Height", GValue::Int(2));
    let mut tile = Struct::new(1);
    tile.set("Tile_ID", GValue::Int(50));
    tile.set("Tile_Orientation", GValue::Int(0));
    tile.set("Tile_Height", GValue::Int(0));
    are.root.set("Tile_List", GValue::List(vec![tile; 4]));
    module.set(ResKey::new(area(), ResType::ARE), are.to_bytes().unwrap());
    module.set(ResKey::new(area(), ResType::GIT), Gff::new(*b"GIT ").to_bytes().unwrap());
    let path = dir.join("fixture.mod");
    // One owned fixture, replaced in place; no save-as backup per attempt.
    std::fs::write(&path, module.to_archive_bytes().unwrap()).unwrap();
    let mut app = Moonglow::new(
        game_root.map(|r| mg_resman::GameInstall::new(r, None, "en")),
        Box::new(NoDialogs::default()),
    );
    app.recovery_dir = None;
    app.compiled_dir = None;
    app.open_module(&path);
    app.open_palette = false;
    if game_root.is_none() {
        let mut rm = mg_resman::ResMan::new();
        let mut mem = mg_resman::MemContainer::new();
        for (name, text) in [
            (
                "visualeffects",
                "2DA V2.0\n\nLabel Type_FD ProgFX_Duration\n0 VFX_RED_GLOW D 407\n1 VFX_SPARK F ****\n2 VFX_BEAM B ****\n3 VFX_PROJECTILE P ****\n",
            ),
            ("skills", "2DA V2.0\n\nLabel Name\n0 AnimalEmpathy ****\n1 Concentration ****\n"),
            ("appearance", "2DA V2.0\n\nLABEL STRING_REF\n0 Human ****\n1 Elf ****\n"),
        ] {
            mem.insert(ResKey::parse(name, ResType::TWODA).unwrap(), text.as_bytes().to_vec());
        }
        rm.add(99, "Authored window fixtures", mg_resman::LayerClass::Directory, mem);
        app.game = Some(Arc::new(mg_rules::GameData::new(rm, mg_tlk::Tlk::new(Language::ENGLISH))));
        app.live_vfx.row = 0;
    }
    let store = Arc::new(LiveStore::default());
    report(&store, 1, vec![]);
    app.live = Some(store.clone());
    app.settings.live.enabled = true;
    app.actions.push(Action::OpenTab(Tab::Area(area())));
    (app, store, dir)
}

pub(crate) fn harness(app: Moonglow, tab: Tab) -> Harness<'static, Moonglow> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400., 950.))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    prime(&mut h, tab);
    h
}
pub(crate) fn prime(h: &mut Harness<'_, Moonglow>, tab: Tab) {
    h.run_steps(4);
    let v = h.state_mut().area_views.get_mut(&area()).expect("area view");
    v.live.on = true;
    v.live.selected = Some("7".into());
    v.orbit = Some(mg_ui::area_view::Orbit {
        target: glam::vec3(12., 13., 0.),
        yaw: -1.57,
        pitch: 0.8,
        distance: 20.,
    });
    h.state_mut().palette.area = Some(area());
    h.state_mut().actions.push(Action::OpenTab(tab.clone()));
    h.run_steps(3);
    let at = h.state().dock.find_tab(&tab).unwrap();
    // Live activation may queue its default tools after this test opens the requested one.
    let _ = h.state_mut().dock.set_active_tab(at);
    if let Some(window) = h.state_mut().dock.get_window_state_mut(at.surface) {
        window.set_position(egui::pos2(100., 80.));
    }
    h.run_steps(3);
}

pub(crate) fn vfx_app() {
    let (app, store, _) = app("vfx-app", None);
    let mut h = harness(app, Tab::LiveVfx);
    h.state_mut().live_vfx.scale = 1.5;
    h.state_mut().live_vfx.offset = [1., 2., 3.];
    h.state_mut().live_vfx.rotation = [10., 20., 30.];
    h.state_mut().area_views.get_mut(&area()).unwrap().live.select(Some("8".into()), true);
    h.get_by_label("Apply VFX").click();
    h.run_steps(2);
    let queued = commands(&store);
    assert_eq!(queued.len(), 2);
    assert!(queued.iter().all(|c| c["op"] == "apply_vfx"
        && c["row"] == 0
        && c["duration"] == "temporary"
        && c["seconds"] == 6.
        && c["scale"] == 1.5
        && c["offset"] == json!({"x":1.,"y":2.,"z":3.})
        && c["rotation"] == json!({"x":10.,"y":20.,"z":30.})));
    assert_eq!(
        queued
            .iter()
            .map(|c| c["target"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        ["7", "8"].into_iter().collect()
    );
    report(&store, 2, queued.iter().map(|c| json!({"id":c["id"],"ok":true})).collect());
    h.run_steps(2);
    h.get_by_label("Clear VFX (ours)").click();
    h.run_steps(2);
    let queued = commands(&store);
    assert_eq!(queued.len(), 2);
    assert!(queued.iter().all(|c| c["op"] == "clear_vfx" && c.get("row").is_none()));
    report(&store, 3, queued.iter().map(|c| json!({"id":c["id"],"ok":true})).collect());
    h.state_mut().live_vfx.last_ground = Some((area(), glam::vec3(2., 4., 0.), "windows".into()));
    h.get_by_label("Aim on the ground").click();
    h.get_by_label("Instant").click();
    h.run_steps(2);
    h.get_by_label("Apply VFX").click();
    h.run_steps(2);
    let queued = commands(&store);
    assert!(
        queued.is_empty(),
        "ground effects wait for the map; real-area click covered by live_review"
    );
    h.state_mut().live_vfx.search = "SPARK".into();
    h.run_steps(2);
    h.get_by_label("1  F  VFX_SPARK");
    assert!(h.query_by_label("0  D  VFX_RED_GLOW").is_none());
    assert!(!h.state().has_unsaved_work());
    assert!(h.state().live_service.is_none());
}

pub(crate) fn appearance_app() {
    let (app, store, _) = app("appearance-app", None);
    let mut h = harness(app, Tab::PlaceableGallery);
    h.get_by_label("Creatures").click();
    h.run_steps(2);
    h.get_by_label("Elf").click();
    h.run_steps(2);
    h.get_by_label("Hexxar's guard (7)");
    assert!(commands(&store).is_empty());
    h.get_by_label("Confirm appearance").click();
    h.run_steps(2);
    assert_eq!(commands(&store)[0]["op"], "set_appearance");
    assert_eq!(commands(&store)[0]["row"], 1);
    assert!(!h.state().has_unsaved_work());
    h.state_mut().area_views.get_mut(&area()).unwrap().live.selected = None;
    h.run_steps(2);
    h.get_by_label("Select creatures in an area, then click a picture to give them its appearance");
}

// Library window checks (kept separate so each window can be committed independently).
pub(crate) fn library_app() {
    let (mut app, store, dir) = app("library-app", None);
    app.live_library.data_dir = Some(dir.join("data"));
    // These are this fixture's declared working entries, never user library files.
    for name in ["Guard copy", "Renamed guard"] {
        let p = dir.join(format!("data/live/library/fixture/{name}.json"));
        if p.exists() {
            mg_ui::live_library::delete(&dir.join("data"), "fixture", name).unwrap();
        }
    }
    let mut h = harness(app, Tab::LiveLibrary);
    h.state_mut().live_library.name = "Guard copy".into();
    h.get_by_label("Capture selected NPC").click();
    h.run_steps(2);
    let queued = commands(&store);
    assert_eq!(queued[0]["op"], "capture_object");
    assert_eq!(queued[0]["target"], "7");
    let path = dir.join("data/live/library/fixture/Guard copy.json");
    assert!(!path.exists());
    report(&store, 2, vec![json!({"id":queued[0]["id"],"ok":true,"object_json":snapshot()})]);
    h.run_steps(3);
    assert_eq!(
        mg_ui::live_library::load(&dir.join("data"), "fixture", "Guard copy").unwrap(),
        snapshot()
    );
    h.state_mut().live_library.name = "Renamed guard".into();
    h.get_by_label("Rename entry").click();
    h.run_steps(2);
    assert!(!path.exists());
    h.get_by_label("Spawn NPC").click();
    h.run_steps(2);
    assert!(
        commands(&store).is_empty(),
        "library spawning waits for the map; NPC ghost/click covered by live_review"
    );
    h.get_by_label("Delete entry...").click();
    h.run_steps(2);
    h.get_by_label("Delete permanently").click();
    h.run_steps(2);
    assert!(mg_ui::live_library::list(&dir.join("data"), "fixture").unwrap().is_empty());
    assert!(
        mg_ui::live_library::save(&dir.join("data"), "fixture", "../escape", &snapshot()).is_err()
    );
    assert!(!h.state().has_unsaved_work());
    assert!(h.state().live_service.is_none());
}

// Skill window checks.
pub(crate) fn skill_app() {
    let (app, store, _) = app("skill-app", None);
    let mut h = harness(app, Tab::LiveSkill);
    assert!(h.get_by_label("Send skill check").accesskit_node().is_disabled());
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    h.run_steps(2);
    h.state_mut().area_views.get_mut(&area()).unwrap().live.select(Some("9".into()), false);
    h.state_mut().live_skill.single = Some(true);
    h.state_mut().live_skill.success_text = "Tracks found.".into();
    h.state_mut().live_skill.fail_text = "No tracks.".into();
    h.get_by_label("Send skill check").click();
    h.run_steps(2);
    let queued = commands(&store);
    assert_eq!(queued[0]["op"], "skill_check");
    assert_eq!(queued[0]["target"], "9");
    assert!(queued[0].get("radius").is_none());
    assert_eq!(queued[0]["players"], true);
    report(
        &store,
        2,
        vec![
            json!({"id":queued[0]["id"],"ok":true,"value":{"players":[{"id":"9","passed":true,"nui_token":5},{"id":"a","passed":false,"nui_token":0}],"truncated":false}}),
        ],
    );
    h.run_steps(3);
    h.get_by_label("Player Mira (9)");
    h.get_by_label("Success");
    h.get_by_label("Failure");
    h.get_by_label("DM bonus");
    let (events, _) = store.events(&mg_live::events::Query::default());
    let event = events.iter().find(|e| e.kind == "command").unwrap();
    let line = mg_ui::live_log::line(event, &|id| format!("Player {id}"));
    assert!(line.contains("Player 9 (9): Success") && line.contains("Player a (a): Failure"));
    assert!(!h.state().has_unsaved_work());
    assert!(h.state().live_service.is_none());
}
