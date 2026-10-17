use super::*;

pub(super) fn picture(h: &mut Harness<'_, Moonglow>, name: &str) {
    let dir = std::env::var_os("MGLIVE_ACCEPT6_PICTURES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-output/live-accept6")
        });
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.png"));
    if name.ends_with("before") && path.exists() {
        return;
    }
    h.render().unwrap().save(path).unwrap();
}

#[test]
fn live_accept6_vfx_ordinary_effects_without_colour_ui() {
    let Some((mut h, area)) = area_harness("live-accept6") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Preview target","app":6,"x":20,"y":20}
        ]}]}),
    );
    h.state_mut().palette.area = Some(area);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    h.state_mut().live_vfx.row = 0;
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveVfx));
    h.run_steps(4);
    assert!(h.query_by_label("Only effects with shader parameters").is_none());
    assert!(h.query_by_label("Coloured effect").is_none());
    picture(&mut h, "vfx-after");
    h.get_by_label("Apply VFX").click();
    h.run_steps(2);
    let commands = store.commands(mg_live::now_ms());
    let sent = commands["commands"].as_array().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["op"], "apply_vfx");
    assert_eq!(sent[0]["target"], "a");
    assert!(sent[0].get("colour").is_none_or(serde_json::Value::is_null));
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_accept6_portraits_from_demo_report() {
    let Some((mut h, area)) = area_harness("live-accept6-portraits") else { return };
    let game = h.state().game.as_ref().unwrap();
    let appearance = game.table("appearance").unwrap();
    let beholder = (0..appearance.len())
        .find(|&r| appearance.get(r, "LABEL").is_some_and(|s| s.eq_ignore_ascii_case("Beholder")))
        .unwrap();
    // Projected from the read-only demo report (33 humans with unset po_).
    assert!(appearance.get(6, "PORTRAIT").is_none());
    assert_eq!(appearance.get(beholder, "PORTRAIT"), Some("po_behold"));
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Demo human","app":6,"portrait":"po_","x":20,"y":20},
            {"id":"b","name":"Demo beholder","app":beholder,"portrait":"po_behold_","x":22,"y":20}
        ]}]}),
    );
    h.state_mut().palette.area = Some(area);
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    view.live.select(Some("a".into()), false);
    view.live.select(Some("b".into()), true);
    h.state_mut().live_focus.data_dir = Some(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/ui-area-live-accept6-portraits/focus-data"),
    );
    let module = h.state().module_path();
    h.state_mut().live_focus.context(module);
    h.state_mut().live_focus.add(&store, &["a".into()]).unwrap();
    h.state_mut().live_focus.add(&store, &["b".into()]).unwrap();
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":area.to_string(),"id":"a","text":"Portrait check.","channel":1},
        {"seq":2,"kind":"chat","area":area.to_string(),"id":"b","text":"Beholder portrait check.","channel":1}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    let before = std::env::var("MGLIVE_ACCEPT6").as_deref() == Ok("before");
    for (tab, name) in
        [(Tab::LiveChat, "chat"), (Tab::LiveFocus, "focus"), (Tab::LiveSelection, "selection")]
    {
        h.state_mut().actions.push(mg_ui::Action::OpenTab(tab));
        h.run_steps(4);
        picture(&mut h, &format!("portraits-{name}-{}", if before { "before" } else { "after" }));
        if !before {
            h.get_by_label("Portrait of Demo human");
            h.get_by_label("Portrait of Demo beholder");
        }
    }
}

#[test]
fn live_accept6_chat_area_tabs_and_detached_window() {
    let Some((mut h, area)) = area_harness("live-accept6-chat") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Human","app":6,"portrait":"po_","x":20,"y":20}]}]}),
    );
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":area.to_string(),"id":"a","text":"Field conversation.","channel":1},
        {"seq":2,"kind":"chat","area":"harbour","id":"b","text":"Harbour conversation.","channel":1}
    ]}).to_string().as_bytes(), mg_live::now_ms()).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveChat));
    h.run_steps(4);
    let before = std::env::var("MGLIVE_ACCEPT6").as_deref() == Ok("before");
    picture(&mut h, if before { "chat-areas-before" } else { "chat-areas-after" });
    if before {
        return;
    }
    h.get_by_label("harbour").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("Field conversation.").is_none());
    h.get_by_label_contains("Harbour conversation.");
    h.get_by_label("harbour").click_secondary();
    h.run_steps(2);
    h.get_by_label("Open in new window").click();
    h.run_steps(3);
    assert!(h.state().dock.find_tab(&Tab::LiveChatArea("harbour".parse().unwrap())).is_some());
    picture(&mut h, "chat-area-window-after");

    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
}

#[test]
fn live_accept6_focus_group_areas_and_member() {
    let Some((mut h, area)) = area_harness("live-accept6-focus") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Field guard","app":6,"portrait":"po_","x":20,"y":20}]}]}),
    );
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[{"area":"harbour","full":true,"cre":[{"id":"b","name":"Dock guard","app":6,"portrait":"po_","x":20,"y":20}]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.state_mut().live_focus.data_dir = Some(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/ui-area-live-accept6-focus/focus-data"),
    );
    let module = h.state().module_path();
    h.state_mut().live_focus.context(module);
    h.state_mut().live_focus.add(&store, &["a".into(), "b".into()]).unwrap();
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":3,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":area.to_string(),"id":"a","text":"Field watch.","channel":1},
        {"seq":2,"kind":"chat","area":"harbour","id":"b","text":"Harbour watch.","channel":1}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveFocus));
    h.run_steps(4);
    let before = std::env::var("MGLIVE_ACCEPT6").as_deref() == Ok("before");
    picture(&mut h, if before { "focus-areas-before" } else { "focus-areas-after" });
    if before {
        return;
    }
    h.get_by_label("harbour").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("Field watch.").is_none());
    h.get_by_label_contains("Harbour watch.");
    h.get_by_label("Members").click();
    h.run_steps(2);
    picture(
        &mut h,
        if std::env::var_os("MGLIVE_FOCUS_MEMBERS_BEFORE").is_some() {
            "focus-members-before"
        } else {
            "focus-members-after"
        },
    );
    h.get_by_label("Field guard").click();
    h.run_steps(2);
    h.get_by_label("Back to group");
    h.get_by_label_contains("Field watch.");
    assert!(h.query_by_label_contains("Harbour watch.").is_none());
    h.state_mut().live_focus.text = "Member reply".into();
    h.run_steps(2);
    h.get_by_label("Send reply").click();
    h.run_steps(2);
    let commands = store.commands(mg_live::now_ms());
    assert_eq!(commands["commands"].as_array().unwrap().len(), 1);
    assert_eq!(commands["commands"][0]["target"], "a");
    assert_eq!(commands["commands"][0]["op"], "npc_say");
    picture(&mut h, "focus-member-after");
    h.get_by_label("Back to group").click();
    h.run_steps(2);
    h.get_by_label_contains("Harbour watch.");
}

#[test]
fn live_accept6_tools_keep_the_chosen_area_with_two_maps_visible() {
    let Some((mut h, area)) = area_harness("live-accept6-scopes") else { return };
    let other: mg_core::ResRef = "harbour".parse().unwrap();
    for ty in [ResType::ARE, ResType::GIT, ResType::GIC] {
        let ws = h.state_mut().ws.as_mut().unwrap();
        if let Some(bytes) = ws.module.get(&ResKey::new(area, ty)).map(|b| b.to_vec()) {
            ws.module.set(ResKey::new(other, ty), bytes);
        }
    }
    review_live_start(&mut h, area, serde_json::json!({"areas":[{"cre":[]}]}));
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::Area(other)));
    h.run_steps(3);
    // Two panes really render in the same frame (the former unconditional assignment lost scope).
    let path = h.state().dock.find_tab(&Tab::Area(other)).unwrap();
    h.state_mut().dock.remove_tab(path);
    let path = h.state().dock.find_tab(&Tab::Area(area)).unwrap();
    h.state_mut().dock.main_surface_mut().split_right(path.node, 0.5, vec![Tab::Area(other)]);
    h.state_mut().palette.area = Some(area);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveSelection));
    h.run_steps(4);
    assert_eq!(h.state().palette.area, Some(area));
    h.get_by_label("Field (harbour)").click();
    h.run_steps(3);
    assert_eq!(h.state().palette.area, Some(other));
    picture(&mut h, "area-tools-after");
}

#[test]
fn live_accept6_legend_is_a_window_with_per_view_settings() {
    let Some((mut h, area)) = area_harness("live-accept6-legend") else { return };
    review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Guard","x":20,"y":20}]}]}),
    );
    h.run_steps(3);
    assert!(h.state().dock.find_tab(&Tab::LiveLegend).is_some());
    assert!(h.query_by_label("- Legend").is_none());
    assert!(h.query_by_label("NPC factions").is_none());
    h.get_by_label("Legend").click();
    h.run_steps(3);
    h.get_by_label("NPC factions");
    assert!(h.query_by_label("Colour key").is_none());
    h.get_by_label("NPCs").click();
    h.run_steps(2);
    assert!(!h.state().area_views[&area].live.markers.shows("NPCs"));
    assert!(mg_ui::live_view::LiveView::default().markers.shows("NPCs"));
    picture(&mut h, "legend-window-after");
    h.state_mut().actions.push(mg_ui::Action::CloseTab(Tab::LiveLegend));
    h.run_steps(2);
    h.state_mut().area_views.get_mut(&area).unwrap().live.on = false;
    h.run_steps(1);
    h.state_mut().area_views.get_mut(&area).unwrap().live.on = true;
    h.run_steps(2);
    assert!(!h.state().area_views[&area].live.markers.shows("NPCs"));
    assert!(h.query_by_label("NPC factions").is_none());
}
#[test]
fn live_accept6_factions_use_only_the_server_catalogue() {
    let Some((mut h, area)) = area_harness("live-accept6-factions") else { return };
    let mut factions =
        mg_module::factions::Factions::of_module(&h.state().ws.as_ref().unwrap().module);
    factions.add_session_factions(2).unwrap();
    factions.factions[5].name = "Unsaved faction".into();
    h.state_mut()
        .ws
        .as_mut()
        .unwrap()
        .module
        .set_gff(ResKey::parse("repute", ResType::FAC).unwrap(), &factions.to_gff())
        .unwrap();
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Guard","x":20,"y":20}]}]}),
    );
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"factions":[{"id":1,"name":"Hostile"},{"id":2,"name":"Commoner"},{"id":6,"name":"Server session"}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    review_open_menu(&mut h, area, false);
    h.get_by_label("Character ⏵").click();
    h.run_steps(2);
    h.get_by_label("Faction ⏵").click();
    h.run_steps(2);
    assert!(h.get_by_label("5: Unsaved faction (editor only)").accesskit_node().is_disabled());
    picture(&mut h, "factions-server-after");
    h.get_by_label("6: Server session").click();
    h.run_steps(2);
    let commands = store.commands(mg_live::now_ms());
    assert_eq!(commands["commands"][0]["faction"], 6);
}

#[test]
fn live_accept6_follow_opens_the_destination_and_keeps_tool_scope() {
    let Some((mut h, area)) = area_harness("live-accept6-follow") else { return };
    let other: mg_core::ResRef = "harbour".parse().unwrap();
    for ty in [ResType::ARE, ResType::GIT, ResType::GIC] {
        let ws = h.state_mut().ws.as_mut().unwrap();
        if let Some(bytes) = ws.module.get(&ResKey::new(area, ty)).map(|b| b.to_vec()) {
            ws.module.set(ResKey::new(other, ty), bytes);
        }
    }
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Traveller","app":6,"portrait":"po_","x":20,"y":20}]}]}),
    );
    h.state_mut().live_focus.data_dir = Some(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/ui-area-live-accept6-follow/focus-data"),
    );
    let module = h.state().module_path();
    h.state_mut().live_focus.context(module);
    h.state_mut().live_focus.add(&store, &["a".into()]).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveFocus));
    h.run_steps(3);
    h.get_by_label("Show on map").click();
    h.run_steps(3);
    assert_eq!(h.state().live_panel.follow.as_ref().unwrap().id, "a");
    picture(&mut h, "follow-before");
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[{"area":area.to_string(),"full":true,"cre":[]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(2);
    assert!(h.state().live_panel.follow.is_some(), "bridge gap during area transfer");
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":3,"areas":[{"area":"harbour","full":true,"cre":[{"id":"a","name":"Traveller","app":6,"portrait":"po_","x":30,"y":20}]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(5);
    assert!(h.state().dock.find_tab(&Tab::Area(other)).is_some());
    assert_eq!(h.state().palette.area, Some(other));
    assert_eq!(h.state().live_panel.follow.as_ref().unwrap().area, other);
    assert!(h.state().area_views[&other].live.on);
    assert!(
        h.state().area_views[&other]
            .orbit
            .as_ref()
            .unwrap()
            .target
            .distance(glam::vec3(30., 20., 0.))
            < 1.
    );
    assert_eq!(h.state().live_chat.area.as_deref(), Some("harbour"));
    assert_eq!(h.state().live_focus.area.as_deref(), Some("harbour"));
    h.get_by_label("Show on map"); // Focus remains the active tool after following.
    picture(&mut h, "follow-after");
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
}

#[test]
fn live_accept6_npc_equipment_hash_replaces_the_reported_look() {
    let Some((mut h, area)) = area_harness("live-accept6-npc-look") else { return };
    let game = h.state().game.as_ref().unwrap();
    let mut gff = Gff::new(*b"UTC ");
    gff.root = mg_preview::CreatureLook::new(6).to_utc();
    let bare = mg_gff::to_json(&gff, game.codepage()).unwrap();
    let mut sword =
        Gff::read(&game.resman.get_named("nw_wswls001", ResType::UTI).unwrap()).unwrap().root;
    sword.id = 16;
    gff.root.set("Equip_ItemList", mg_gff::Value::List(vec![sword]));
    let equipped = mg_gff::to_json(&gff, game.codepage()).unwrap();
    // One reported NPC matches a GIT preview carrying a sword; another is an extra.
    let mut original = mg_area::AreaObject::read(game, mg_area::ObjectKind::Creature, 0, &gff.root);
    original.template = Some("nw_bartender".parse().unwrap());
    original.position = glam::vec3(20., 20., 0.);
    h.state_mut().area_views.get_mut(&area).unwrap().model.as_mut().unwrap().objects.push(original);
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","r":"nw_bartender","app":6,"name":"Equipment watcher","x":20,"y":20,"lookHash":1,"look":equipped},{"id":"b","r":"nw_bartender","app":6,"name":"Extra NPC","x":22,"y":20,"lookHash":1,"look":equipped}]}]}),
    );
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.show_start = false;
        let orbit = view.orbit.as_mut().unwrap();
        orbit.target = glam::vec3(21., 20., 0.7);
        orbit.pitch = 0.55;
        orbit.yaw = 0.4;
        orbit.distance = 8.;
    }
    h.run_steps(4);
    if std::env::var_os("MGLIVE_ACCEPT6_NPC_BEFORE").is_some() {
        picture(&mut h, "npc-look-before");
        return;
    }
    let sword_shown = |h: &Harness<'_, Moonglow>| {
        h.state().area_views[&area]
            .live
            .preview("a")
            .unwrap()
            .parts
            .iter()
            .any(|p| p.model.to_ascii_lowercase().starts_with("wswls"))
    };
    assert!(sword_shown(&h));
    picture(&mut h, "npc-look-equipped-after");
    for (seq, look, has_sword) in [(2, bare.clone(), false), (3, equipped, true), (4, bare, false)]
    {
        store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":seq,"areas":[{"area":area.to_string(),"full":true,"cre":[{"id":"a","r":"nw_bartender","app":6,"name":"Equipment watcher","x":20,"y":20,"lookHash":seq,"look":look},{"id":"b","r":"nw_bartender","app":6,"name":"Extra NPC","x":22,"y":20,"lookHash":seq,"look":look}]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
        h.run_steps(3);
        assert_eq!(sword_shown(&h), has_sword, "look hash {seq}");
        assert_eq!(
            h.state().area_views[&area]
                .live
                .preview("b")
                .unwrap()
                .parts
                .iter()
                .any(|p| p.model.to_ascii_lowercase().starts_with("wswls")),
            has_sword
        );
    }
    picture(&mut h, "npc-look-unequipped-after");
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_accept6_to_dm_excludes_party_and_replies_to_the_clicked_player() {
    let Some((mut h, area)) = area_harness("live-accept6-dm") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Asha","pc":true,"x":20,"y":20},{"id":"b","name":"Bram","pc":true,"x":22,"y":20},{"id":"c","name":"NPC","x":24,"y":20}]}]}),
    );
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":area.to_string(),"id":"a","name":"Asha","text":"Question for the DM.","channel":14,"to_dm":true},
        {"seq":2,"kind":"chat","area":area.to_string(),"id":"b","name":"Bram","text":"Party only.","channel":6},
        {"seq":3,"kind":"chat","area":area.to_string(),"id":"c","text":"AI only.","channel":14,"silent":true},
        {"seq":4,"kind":"chat","area":area.to_string(),"id":"b","text":"Ordinary talk.","channel":1}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveChat));
    h.run_steps(3);
    h.get_by_label("To DM").click();
    h.run_steps(2);
    h.get_by_label_contains("Question for the DM.");
    for text in ["Party only.", "AI only.", "Ordinary talk."] {
        assert!(h.query_by_label_contains(text).is_none());
    }
    h.get_by_label("Reply to Asha").click();
    h.run_steps(1);
    store.set_policy(mg_live::store::Policy { players: false, ..Default::default() });
    h.state_mut().live_chat.text = "DM answer".into();
    h.run_steps(2);
    assert!(h.get_by_label("Send DM reply").accesskit_node().is_disabled());
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    h.run_steps(2);
    picture(&mut h, "chat-to-dm-after");
    h.get_by_label("Send DM reply").click();
    h.run_steps(2);
    let q = store.commands(mg_live::now_ms());
    assert_eq!(q["commands"].as_array().unwrap().len(), 1);
    assert_eq!(q["commands"][0]["op"], "pc_message");
    assert_eq!(q["commands"][0]["mode"], "dm");
    assert_eq!(q["commands"][0]["target"], "a");
}

#[test]
fn live_accept6_delete_confirms_the_whole_eligible_selection() {
    let Some((mut h, area)) = area_harness("live-accept6-delete") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"First NPC","x":20,"y":20},{"id":"b","name":"Second NPC","x":22,"y":20}]}]}),
    );
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    view.live.select(Some("a".into()), false);
    view.live.select(Some("b".into()), true);
    let at = screen(&h, area, glam::vec3(20., 18., 0.));
    h.hover_at(at);
    h.key_press(egui::Key::Delete);
    h.run_steps(3);
    if std::env::var_os("MGLIVE_ACCEPT6_DELETE_BEFORE").is_some() {
        picture(&mut h, "delete-group-before");
        assert_eq!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().len(), 1);
        return;
    }
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
    h.get_by_label("Remove 2 objects");
    picture(&mut h, "delete-group-after");
    h.get_by_label("Remove 2 objects").click();
    h.run_steps(2);
    let q = store.commands(mg_live::now_ms());
    let cmds = q["commands"].as_array().unwrap();
    assert_eq!(cmds.len(), 2);
    assert!(cmds.iter().all(|c| c["op"] == "destroy"));
    assert!(cmds.iter().any(|c| c["target"] == "a") && cmds.iter().any(|c| c["target"] == "b"));
}

#[test]
fn live_accept6_clipboard_captures_group_and_pastes_with_offsets() {
    let Some((mut h, area)) = area_harness("live-accept6-clipboard") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Copied guard","r":"nw_bartender","app":6,"x":20,"y":20,"f":90}],"plc":[{"id":"b","name":"Copied chest","r":"nw_plc_chest01","app":4,"x":23,"y":20,"f":0}]}]}),
    );
    store.set_policy(mg_live::store::Policy { scripts: true, ..Default::default() });
    {
        let v = h.state_mut().area_views.get_mut(&area).unwrap();
        v.live.select(Some("a".into()), false);
        v.live.select(Some("b".into()), true);
        v.show_start = false;
        v.orbit.as_mut().unwrap().pitch = 0.65;
        v.orbit.as_mut().unwrap().target = glam::vec3(24., 22., 0.5);
        v.orbit.as_mut().unwrap().distance = 13.;
    }
    let game = h.state().game.as_ref().unwrap();
    let mut npc = Gff::new(*b"UTC ");
    npc.root = mg_preview::CreatureLook::new(6).to_utc();
    let mut plc = Gff::new(*b"UTP ");
    plc.root.set("Appearance", mg_gff::Value::Dword(4));
    let a = mg_gff::to_json(&npc, game.codepage()).unwrap();
    let b = mg_gff::to_json(&plc, game.codepage()).unwrap();
    let at = screen(&h, area, glam::vec3(25., 23., 0.));
    h.hover_at(at);
    picture(&mut h, "clipboard-before");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    h.run_steps(2);
    let q = store.commands(mg_live::now_ms());
    let commands = q["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 2);
    assert!(commands.iter().all(|c| c["op"] == "capture_object"));
    let results:Vec<_>=commands.iter().map(|c|serde_json::json!({"id":c["id"],"ok":true,"object_json":if c["target"]=="a" {a.clone()}else{b.clone()}})).collect();
    store
        .sync(
            serde_json::json!({"v":1,"boot":"accept2","results":results}).to_string().as_bytes(),
            mg_live::now_ms(),
        )
        .unwrap();
    h.run_steps(3);
    h.hover_at(at);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::V);
    h.run_steps(3);
    assert_eq!(h.state().area_views[&area].pasted_shown, 2);
    picture(&mut h, "clipboard-ghosts-after");
    h.key_press(egui::Key::E);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(3);
    let q = store.commands(mg_live::now_ms());
    let commands = q["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 2);
    assert!(commands.iter().all(|c| c["op"] == "spawn_json"));
    let dx = commands[0]["x"].as_f64().unwrap() - commands[1]["x"].as_f64().unwrap();
    let dy = commands[0]["y"].as_f64().unwrap() - commands[1]["y"].as_f64().unwrap();
    assert!((dx.hypot(dy) - 3.).abs() < 0.01);
    assert!(!h.state().has_unsaved_work());
    // A failed capture after Cut's single confirmation must never remove originals.
    let ack: Vec<_> =
        commands.iter().map(|c| serde_json::json!({"id":c["id"],"ok":true})).collect();
    store
        .sync(
            serde_json::json!({"v":1,"boot":"accept2","results":ack}).to_string().as_bytes(),
            mg_live::now_ms(),
        )
        .unwrap();
    h.hover_at(at);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::X);
    h.run_steps(3);
    h.get_by_label("Cut 2 objects");
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
    picture(&mut h, "clipboard-cut-after");
    h.get_by_label("Cut 2 objects").click();
    h.run_steps(2);
    let q = store.commands(mg_live::now_ms());
    let commands = q["commands"].as_array().unwrap();
    let results: Vec<_> = commands
        .iter()
        .map(|c| serde_json::json!({"id":c["id"],"ok":false,"err":"test refusal"}))
        .collect();
    store
        .sync(
            serde_json::json!({"v":1,"boot":"accept2","results":results}).to_string().as_bytes(),
            mg_live::now_ms(),
        )
        .unwrap();
    h.run_steps(3);
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
    // Retry cut: every snapshot must be valid before either destroy is queued.
    h.hover_at(at);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::X);
    h.run_steps(3);
    h.get_by_label("Cut 2 objects").click();
    h.run_steps(2);
    let q = store.commands(mg_live::now_ms());
    let commands = q["commands"].as_array().unwrap();
    let results:Vec<_>=commands.iter().map(|c|serde_json::json!({"id":c["id"],"ok":true,"object_json":if c["target"]=="a" {a.clone()}else{b.clone()}})).collect();
    store
        .sync(
            serde_json::json!({"v":1,"boot":"accept2","results":results}).to_string().as_bytes(),
            mg_live::now_ms(),
        )
        .unwrap();
    h.run_steps(3);
    let q = store.commands(mg_live::now_ms());
    assert_eq!(q["commands"].as_array().unwrap().len(), 2);
    assert!(q["commands"].as_array().unwrap().iter().all(|c| c["op"] == "destroy"));
    h.hover_at(at);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::V);
    h.run_steps(2);
    assert_eq!(h.state().area_views[&area].pasted_shown, 2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().area_views[&area].pasted_shown, 0);
}

#[test]
fn live_accept6_scene_macros_visible_and_readable() {
    let Some((mut h, area)) = area_harness("live-accept6-scenes") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Street lamplighter","app":6,"x":20.85216522216797,"y":20}]}]}),
    );
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/ui-area-live-accept6-scenes/macros");
    h.state_mut().live_scene.data_dir = Some(dir);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::Scene));
    h.run_steps(3);
    h.get_by_label("Try it: make an example scene with the selected NPC").click();
    h.run_steps(3);
    if std::env::var_os("MGLIVE_ACCEPT6_SCENES_BEFORE").is_some() {
        picture(&mut h, "scenes-before");
        return;
    }
    h.get_by_label("A scene is a macro: a recorded list of DM commands you can replay");
    h.get_by_label("2. Street lamplighter walks to 25.9, 20.0");
    h.get_by_label("Save").click();
    h.run_steps(3);
    h.get_by_label("Saved macros");
    h.get_by_label("Play");
    picture(&mut h, "scenes-after");
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
    h.get_by_label("Play").click();
    for _ in 0..50 {
        h.run_steps(1);
        if !store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let q = store.commands(mg_live::now_ms());
    assert_eq!(q["commands"][0]["op"], "npc_say");
    assert_eq!(q["commands"][0]["target"], "a");
    h.run_steps(4);
    h.get_by_label("Stop scene").scroll_to_me();
    h.run_steps(3);
    assert!(!h.get_by_label("Stop scene").accesskit_node().is_disabled());
    h.get_by_label("Stop scene").click();
    h.run_steps(3);
    assert!(h.state().live_scene.status["playing"].is_null());
    h.get_by_label("Example scene").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Example scene").click_secondary();
    h.run_steps(2);
    h.get_by_label("Rename...").click();
    h.run_steps(2);
    h.get_by_label("New macro name").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("Morning greeting".into()));
    h.run_steps(1);
    h.get_by_label("Rename").click();
    h.run_steps(3);
    h.get_by_label("Morning greeting");
    assert_eq!(h.state().live_scene.draft.name, "Morning greeting");
    h.get_by_label("Morning greeting").click_secondary();
    h.run_steps(2);
    h.get_by_label("Delete macro...").click();
    h.run_steps(2);
    h.get_by_label("Delete saved macro").click();
    h.run_steps(3);
    assert!(h.query_by_label("Morning greeting").is_none());
    assert!(h.state().live_scene.draft.steps.is_empty());
}

#[test]
fn live_accept6_broken_looks_still_render() {
    let Some((mut h, area)) = area_harness("live-accept6-look-hotfix") else { return };
    let game = h.state().game.as_ref().unwrap();
    let mut gff = Gff::new(*b"UTC ");
    gff.root = mg_preview::CreatureLook::new(6).to_utc();
    let mut npc = mg_gff::to_json(&gff, game.codepage()).unwrap();
    let mut pc = npc.clone();
    npc.as_object_mut().unwrap().remove("__data_type");
    pc["Description"] =
        serde_json::json!({"type":"cexolocstring", "id":4294967295u32,"value":{"0":"Wizard 🧙"}});
    assert!(mg_gff::from_json(&npc, game.codepage()).is_err());
    assert!(mg_gff::from_json(&pc, game.codepage()).is_err());
    review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Reported NPC","r":"nw_bartender","app":6,"x":20,"y":20,"lookHash":1,"look":npc},
            {"id":"b","name":"Reported player","pc":true,"app":6,"x":22,"y":20,"lookHash":1,"look":pc}
        ]}]}),
    );
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    view.show_start = false;
    let orbit = view.orbit.as_mut().unwrap();
    orbit.target = glam::vec3(21., 20., 0.7);
    orbit.distance = 9.;
    orbit.pitch = 0.55;
    orbit.yaw = 0.4;
    h.run_steps(4);
    let before = std::env::var_os("MGLIVE_LOOK_HOTFIX_BEFORE").is_some();
    for id in ["a", "b"] {
        assert_eq!(h.state().area_views[&area].live.preview(id).is_some(), !before, "{id}");
    }
    picture(&mut h, if before { "look-hotfix-before" } else { "look-hotfix-after" });
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_accept6_focus_speech_filters() {
    let Some((mut h, area)) = area_harness("live-accept6-focus-filters") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Street guard","app":6,"portrait":"po_","x":20,"y":20},
            {"id":"b","name":"Patrol follower","app":6,"portrait":"po_","x":21,"y":20}
        ]}]}),
    );
    h.state_mut().live_focus.data_dir = Some(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/ui-area-live-accept6-focus-filters/focus-data"),
    );
    let module = h.state().module_path();
    h.state_mut().live_focus.context(module);
    h.state_mut().live_focus.entries.clear();
    h.state_mut().live_focus.add(&store, &["a".into()]).unwrap();
    h.state_mut().live_focus.notice = None;
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":area.to_string(),"id":"a","text":"My own line.","channel":1,"position":{"x":20,"y":20}},
        {"seq":2,"kind":"chat","area":area.to_string(),"id":"b","text":"Nearby patrol speech.","channel":1,"position":{"x":21,"y":20}},
        {"seq":3,"kind":"chat","area":area.to_string(),"id":null,"target":"a","text":"You are now in a Party PVP area.","channel":5}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveFocus));
    h.run_steps(4);
    if std::env::var_os("MGLIVE_FOCUS_FILTERS_BEFORE").is_some() {
        h.get_by_label_contains("Nearby patrol speech.");
        h.get_by_label_contains("You are now in a Party PVP area.");
        picture(&mut h, "focus-filters-before");
        return;
    }
    h.get_by_label_contains("My own line.");
    assert!(h.query_by_label_contains("Nearby patrol speech.").is_none());
    assert!(h.query_by_label_contains("You are now in a Party PVP area.").is_none());
    picture(&mut h, "focus-filters-after");
    h.get_by_label("Nearby / incoming speech").click();
    h.run_steps(2);
    h.get_by_label_contains("Nearby patrol speech.");
    assert!(h.query_by_label_contains("You are now in a Party PVP area.").is_none());
    h.get_by_label("Server messages").click();
    h.run_steps(2);
    h.get_by_label_contains("You are now in a Party PVP area.");
    h.get_by_label("Nearby / incoming speech").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("Nearby patrol speech.").is_none());
    h.get_by_label_contains("My own line.");
}
