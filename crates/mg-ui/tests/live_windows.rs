//! Regressions for windows against the runtime service and a fresh fake store.
#[path = "support/live_windows.rs"]
mod support;
use egui_kittest::kittest::{NodeT, Queryable};
use mg_ui::Tab;

#[test]
fn hexxar_runtime_enabled_overrides_saved_option() {
    let (mut app, store, dir) = support::app("runtime-enabled", None);
    // An owned listener reserves the Redis endpoint without touching any server.
    let redis = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    app.settings.live.enabled = false; // Start-LiveDemo leaves this unchanged.
    app.live_service = Some(
        mg_live::LiveService::start(
            mg_live::LiveConfig {
                enabled: true,
                api_port: 0,
                redis_port: redis.local_addr().unwrap().port(),
                keep_wsl: Some(false),
                keep_logs: false,
                policy: mg_live::store::Policy { players: true, ..Default::default() },
                ..Default::default()
            },
            store.clone(),
            std::sync::Arc::new(|| {}),
        )
        .unwrap(),
    );
    // Complete desktop startup before issuing commands through the runtime service.
    app.live_service.as_mut().unwrap().publish_descriptor(&dir.join("data")).unwrap();
    app.live_library.data_dir = Some(dir.join("data"));
    let mut h = support::harness(app, Tab::LiveVfx);
    assert!(
        !h.get_by_label("Apply VFX").accesskit_node().is_disabled(),
        "fresh report, runtime enabled"
    );
    h.get_by_label("Apply VFX").click();
    h.run_steps(2);
    let queued = support::commands(&store);
    assert_eq!(queued[0]["op"], "apply_vfx");
    support::report(&store, 2, vec![serde_json::json!({"id":queued[0]["id"],"ok":true})]);
    support::prime(&mut h, Tab::LiveSkill);
    h.state_mut()
        .area_views
        .get_mut(&support::area())
        .unwrap()
        .live
        .select(Some("9".into()), false);
    h.state_mut().live_skill.single = Some(true);
    h.get_by_label("Send skill check").click();
    h.run_steps(2);
    let queued = support::commands(&store);
    assert_eq!(queued[0]["op"], "skill_check");
    support::report(
        &store,
        3,
        vec![serde_json::json!({"id":queued[0]["id"],"ok":true,"value":{"players":[]}})],
    );
    h.run_steps(2);
    assert!(
        h.state().live_skill.result.is_some(),
        "pending result must survive saved enabled=false"
    );
    support::prime(&mut h, Tab::Scene);
    assert!(!h.get_by_label("Record").accesskit_node().is_disabled());
    h.get_by_label("Record").click();
    h.run_steps(2);
    assert!(
        h.state().live_scene.recording,
        "Scenes follows runtime enabled, even with saved enabled=false"
    );
    h.get_by_label("Stop scene").click();
    h.run_steps(2);
    support::prime(&mut h, Tab::LiveLibrary);
    // An existing entry is sufficient to exercise Spawn without a game or disk capture.
    let data = dir.join("data");
    if !mg_ui::live_library::list(&data, "fixture").unwrap().contains(&"Runtime guard".into()) {
        mg_ui::live_library::save(&data, "fixture", "Runtime guard", &support::snapshot()).unwrap();
    }
    h.get_by_label("Refresh library").click();
    h.run_steps(2);
    h.get_by_label("Runtime guard").click();
    h.run_steps(2);
    h.get_by_label("Spawn NPC").click();
    h.run_steps(2);
    assert!(
        support::commands(&store).is_empty(),
        "runtime-enabled library spawn arms a map preview"
    );
    h.get_by_label("Aim the NPC ghost, then click the map to spawn. Turn keys rotate; Esc/right click cancels.");
    // Runtime availability does not turn an actually old area into a fresh one.
    store.sync(serde_json::json!({"v":1,"boot":"windows","seq":4,"areas":[{"area":"start","full":true,"cre":[]}]}).to_string().as_bytes(), mg_live::now_ms()-20_000).unwrap();
    support::prime(&mut h, Tab::LiveVfx);
    assert!(h.get_by_label("Apply VFX").accesskit_node().is_disabled());
}

#[test]
fn hexxar_skill_selected_player_or_radius_with_optional_texts() {
    let (app, store, _) = support::app("skill-target", None);
    let mut h = support::harness(app, Tab::LiveSkill);
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    h.state_mut()
        .area_views
        .get_mut(&support::area())
        .unwrap()
        .live
        .select(Some("9".into()), false);
    h.run_steps(2);
    h.get_by_label("Selected player");
    h.get_by_label("Send skill check").click();
    h.run_steps(2);
    let queued = support::commands(&store);
    assert_eq!(queued[0]["target"], "9");
    assert_eq!(queued[0]["success_text"], "");
    assert_eq!(queued[0]["fail_text"], "");
    assert!(queued[0].get("x").is_none() && queued[0].get("radius").is_none());
    support::report(
        &store,
        2,
        vec![
            serde_json::json!({"id":queued[0]["id"],"ok":true,"value":{"players":[{"id":"9","passed":true,"nui_token":1}]}}),
        ],
    );
    h.run_steps(2);
    h.get_by_label("Success");
    h.get_by_label("Players in radius").click();
    h.get_by_label("Send skill check").click();
    h.run_steps(2);
    let queued = support::commands(&store);
    assert!(queued.is_empty(), "radius mode needs a map click, covered in the real-area aim test");
}

#[test]
fn hexxar_inventory_reads_then_gives_takes_and_protects_players() {
    let (app, store, _) = support::app("inventory", None);
    let mut h = support::harness(app, Tab::LiveInventory);
    h.get_by_label("Loading inventory...");
    let query = support::commands(&store)[0].clone();
    assert_eq!(query["op"], "view_inventory");
    let items = serde_json::json!({"items":[{"id":"a","name":"Potion","resref":"nw_it_mpotion001","stack":3,"slot":-1},{"id":"b","name":"Sword","resref":"nw_wswdls001","stack":1,"slot":4}],"truncated":false});
    support::report(&store, 2, vec![serde_json::json!({"id":query["id"],"ok":true,"value":items})]);
    h.run_steps(2);
    h.get_by_label("Backpack");
    h.get_by_label("Right hand: Sword");
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    h.state_mut().live_inventory.resref = "nw_it_mpotion001".into();
    h.run_steps(2);
    h.get_by_label("Give item...").click();
    h.run_steps(2);
    h.get_by_label("Give").click();
    h.run_steps(2);
    let give = support::commands(&store)[0].clone();
    assert_eq!(give["op"], "give_item");
    assert_eq!(give["target"], "7");
    support::report(&store, 3, vec![serde_json::json!({"id":give["id"],"ok":true,"obj":"c"})]);
    h.run_steps(2);
    let refresh = support::commands(&store)[0].clone();
    assert_eq!(refresh["op"], "view_inventory");
    support::report(
        &store,
        4,
        vec![serde_json::json!({"id":refresh["id"],"ok":true,"value":items})],
    );
    h.run_steps(2);
    h.get_by_label("Backpack: Potion").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Backpack: Potion").click_secondary();
    h.run_steps(2);
    h.get_by_label("Take... \u{23f5}").click();
    h.run_steps(2);
    assert!(support::commands(&store).is_empty());
    h.get_by_label("Confirm take").click();
    h.run_steps(2);
    let take = support::commands(&store)[0].clone();
    assert_eq!(take["op"], "take_item");
    assert_eq!(take["item"], "a");
    // A changed selection cannot display or act on the previous inventory.
    h.state_mut()
        .area_views
        .get_mut(&support::area())
        .unwrap()
        .live
        .select(Some("9".into()), false);
    store.set_policy(Default::default());
    h.run_steps(2);
    assert!(h.state().live_inventory.result.is_none());
    assert!(h.get_by_label("Refresh inventory").accesskit_node().is_disabled());
    assert!(h.query_by_label("Backpack: Potion").is_none());
    assert!(!h.state().has_unsaved_work());
}
