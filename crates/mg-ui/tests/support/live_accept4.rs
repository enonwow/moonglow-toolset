use super::*;

fn picture(h: &mut Harness<'_, Moonglow>, name: &str) {
    let stage = std::env::var("MGLIVE_ACCEPT4").unwrap_or_else(|_| "after".into());
    h.render()
        .unwrap()
        .save(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../../target/test-output/live-next/visual/accept4-{name}-{stage}.png"
            )),
        )
        .unwrap();
}

#[test]
fn live_accept4_menu_pictures() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Guard","x":20,"y":20,"hp":[20,20],"faction":2},
            {"id":"b","name":"Player","pc":true,"x":24,"y":20,"hp":[30,30],"faction":0}
        ]}]}),
    );
    picture(&mut h, "layout");
    review_open_menu(&mut h, area, false);
    picture(&mut h, "menu-single");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("b".into()), true);
    review_open_menu(&mut h, area, false);
    picture(&mut h, "menu-group");
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
}

#[test]
fn live_accept4_log_opens_only_on_demand() {
    live_review_layout_places_tools_right_and_selection_focus_keeps_the_group();
}

#[test]
fn live_accept4_colours_distinguish_world_objects_from_commoners() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{
        "cre":[{"id":"a","name":"Commoner NPC","x":16,"y":16,"faction":2}],
        "plc":[{"id":"b","name":"Cabinet","x":27,"y":16,"faction":2}],
        "doors":[{"id":"c","name":"Door","x":16,"y":27,"faction":2}],
        "items":[{"id":"d","name":"Ground item","x":27,"y":27,"faction":2}]}]}),
    );
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveLegend));
    h.run_steps(3);
    h.get_by_label("NPC factions").click();
    h.run_steps(2);
    picture(&mut h, "colours");
    h.get_by_label("2: Commoner").click();
    h.run_steps(2);
    let view = &h.state().area_views[&area];
    let report = h.state().live.as_ref().unwrap().area(&area.to_string()).unwrap();
    for (kind, o) in report.all() {
        assert_eq!(view.live.markers.object(kind, o), kind != mg_area::ObjectKind::Creature);
    }
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_accept4_windows_audit() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let out =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output/live-next");
    h.state_mut().live_scene.data_dir = Some(out.join("window-audit-scenes"));
    h.state_mut().live_library.data_dir = Some(out.join("window-audit-library"));
    review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Gate guard","x":20,"y":20,"hp":[14,20],"faction":4},
            {"id":"b","name":"Player","pc":true,"x":25,"y":20,"hp":[25,30],"faction":0}
        ]}]}),
    );
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    h.state_mut().palette.area = Some(area);
    let stage = std::env::var("MGLIVE_POLISH_PICTURE").ok();
    let only = std::env::var("MGLIVE_POLISH_WINDOW").ok();
    for tab in [
        Tab::LivePanel,
        Tab::LiveSelection,
        Tab::LiveChat,
        Tab::LiveLog,
        Tab::Scene,
        Tab::LiveInventory,
        Tab::LiveSkill,
        Tab::LiveVfx,
        Tab::LiveLibrary,
    ] {
        let name = format!("{tab:?}").to_ascii_lowercase();
        if only.as_ref().is_some_and(|only| only != &name) {
            continue;
        }
        h.state_mut().actions.push(mg_ui::Action::OpenTab(tab.clone()));
        h.run_steps(4);
        let at = h.state().dock.find_tab(&tab).unwrap();
        assert_eq!(at.node, h.state().dock.find_tab(&Tab::Palette).unwrap().node);

        if tab == Tab::LiveVfx {
            h.get_by_label(
                "Choose an effect, then apply it to the selection or aim it on the map.",
            );
            assert!(!h.get_by_label("Apply VFX").accesskit_node().is_disabled());
            h.state_mut().live_vfx.search = "no-such-effect-in-the-table".into();
            h.run_steps(2);
            h.get_by_label("No matching effects. Clear the search or choose All.");
            h.state_mut().live_vfx.search.clear();
            h.state_mut().area_views.get_mut(&area).unwrap().live.select(None, false);
            h.run_steps(2);
            assert!(h.get_by_label("Apply VFX").accesskit_node().is_disabled());
            assert!(h.get_by_label("Clear VFX (ours)").accesskit_node().is_disabled());
            h.get_by_label("Aim on the ground").click();
            h.run_steps(2);
            assert!(!h.get_by_label("Apply VFX").accesskit_node().is_disabled());
            assert!(h.get_by_label("Clear VFX (ours)").accesskit_node().is_disabled());
            h.get_all_by_label("Selection")
                .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
                .unwrap()
                .click();
            h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
            h.run_steps(2);
        }
        if tab == Tab::LiveSkill {
            h.get_by_label(
                "Next: Send skill check, then click the map to choose the circle's centre.",
            );
            assert!(h.get_by_label("Selected player").accesskit_node().is_disabled());
            assert!(!h.get_by_label("Send skill check").accesskit_node().is_disabled());
        }
        if tab == Tab::LiveLibrary {
            h.get_by_label("No saved NPCs in this module's library.");
            assert!(h.get_by_label("Spawn NPC").accesskit_node().is_disabled());
            assert!(!h.get_by_label("Capture selected NPC").accesskit_node().is_disabled());
        }
        if tab == Tab::LiveLog {
            h.get_by_label("No events in this view.");
            assert!(h.get_by_label("Copy").accesskit_node().is_disabled());
        }
        if tab == Tab::LiveChat {
            h.get_by_label("No conversation yet.");
            assert!(h.get_by_label("Send").accesskit_node().is_disabled());
            h.state_mut().live_chat.text = "Guard the gate.".into();
            h.run_steps(2);
            assert!(!h.get_by_label("Send").accesskit_node().is_disabled());
            h.state_mut().live_chat.text.clear();
            h.run_steps(2);
        }
        if tab == Tab::LivePanel {
            h.get_by_label("Wounded").click();
            h.run_steps(2);
            h.get_by_label("No objects match the current filters.");
            h.get_by_label("Clear filters").click();
            h.run_steps(2);
            h.get_by_label("2 of 2 reported objects");
        }
        if let Some(stage) = &stage {
            h.render()
                .unwrap()
                .save(out.join(format!("visual/polish-{name}-{stage}.png")))
                .unwrap();
        }
    }
    if only.as_deref().is_none_or(|v| v == "liveselection") {
        h.state_mut().area_views.get_mut(&area).unwrap().live.select(None, false);
        h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveSelection));
        h.run_steps(3);
        h.get_by_label("Select objects on the Live map.");
        assert_eq!(h.state().area_views[&area].live.selection_ids().count(), 0);
        if let Some(stage) = &stage {
            h.render()
                .unwrap()
                .save(out.join(format!("visual/polish-liveselection-empty-{stage}.png")))
                .unwrap();
        }
    }
    assert!(!h.state().has_unsaved_work(), "browsing Live tools never edits the module");
}

#[test]
fn live_accept4_placeable_appearance_entry() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"plc":[
            {"id":"a","name":"Cabinet","x":20,"y":20,"app":1,"app_original":2}
        ]}]}),
    );
    review_open_menu(&mut h, area, false);
    h.get_by_label("Look and effects ⏵").click();
    h.run_steps(3);
    picture(&mut h, "appearance-entry");
    if std::env::var("MGLIVE_ACCEPT4").as_deref() == Ok("before") {
        return;
    }
    h.get_by_label("Appearance…").click();
    h.run_steps(3);
    assert_eq!(format!("{:?}", h.state().placeable_gallery.as_ref().unwrap().kind), "Placeable");
    h.get_by_label("Restore original appearance");
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_accept4_group_combat_inventory_and_danger() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{
        "cre":[{"id":"a","name":"Guard","x":20,"y":20},
            {"id":"b","name":"Player","pc":true,"x":24,"y":20},
            {"id":"c","name":"Victim","x":30,"y":20}],
        "plc":[{"id":"d","name":"Chest","x":20,"y":25}]}]}),
    );
    let revision = h.state().ws.as_ref().unwrap().revision();
    review_open_menu(&mut h, area, false);
    assert_eq!(h.get_all_by_label("Inventory…").count(), 1);
    assert!(h.query_by_label("Inventory (to the Live Log)").is_none());
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("b".into()), true);
    review_open_menu(&mut h, area, false);
    assert!(h.query_by_label("Inventory…").is_none());
    h.get_by_label("Combat ⏵").click();
    h.run_steps(2);
    picture(&mut h, "group-combat");
    h.get_all_by_label("Attack…").last().unwrap().click();
    h.run_steps(2);
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    let victim = screen(&h, area, glam::vec3(30., 20., 0.3));
    h.hover_at(victim);
    press(&h, victim, true, egui::Modifiers::NONE);
    press(&h, victim, false, egui::Modifiers::NONE);
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"].clone();
    assert_eq!(q.as_array().unwrap().len(), 2);
    assert!(q.as_array().unwrap().iter().all(|q| q["op"] == "npc_attack" && q["victim"] == "c"));
    review_open_menu(&mut h, area, false);
    h.get_by_label("DANGER ⏵").click();
    h.run_steps(2);
    h.get_by_label("Kill ⏵").click();
    h.run_steps(2);
    picture(&mut h, "kill-confirm");
    assert_eq!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().len(), 2);
    for _ in 0..3 {
        h.key_press(egui::Key::Escape);
        h.run_steps(1);
    }
    review_open_menu(&mut h, area, false);
    review_click(&mut h, "Kill");
    h.run_steps(2);
    assert_eq!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().len(), 4);
    store.set_policy(mg_live::store::Policy { players: false, ..Default::default() });
    review_open_menu(&mut h, area, false);
    h.get_by_label("Inventory…").click();
    h.run_steps(3);
    assert_eq!(h.state().area_views[&area].live.selected.as_deref(), Some("a"));
    assert_eq!(h.state().area_views[&area].live.selection_ids().count(), 2);
    h.get_by_label("Inventory: Guard");
    // A selection without a living creature keeps the same disabled Attack entry.
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("d".into()), false);
    let pos = screen(&h, area, glam::vec3(20., 25., 0.3));
    h.hover_at(pos);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(2);
    assert!(h.get_by_label("Attack…").accesskit_node().is_disabled());
    h.get_by_label("Combat ⏵").click();
    h.run_steps(2);
    assert!(h.get_all_by_label("Attack…").last().unwrap().accesskit_node().is_disabled());
    for _ in 0..2 {
        h.key_press(egui::Key::Escape);
        h.run_steps(1);
    }
    assert_eq!(h.state().ws.as_ref().unwrap().revision(), revision);
}

#[test]
fn live_accept4_dialogs_audit() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"plc":[
            {"id":"a","name":"Cabinet","x":20,"y":20,"app":1,"static":false,"useable":true}
        ]}]}),
    );
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-next/visual");
    let stage = std::env::var("MGLIVE_POLISH_PICTURE").ok();
    review_open_menu(&mut h, area, false);
    review_click(&mut h, "Visual Transform…");
    h.run_steps(3);
    h.get_by_label("Apply Transform");
    if let Some(stage) = &stage {
        h.render().unwrap().save(out.join(format!("polish-properties-{stage}.png"))).unwrap();
    }
    h.get_by_label("Close window").click();
    h.run_steps(2);
    h.state_mut().area_views.get_mut(&area).unwrap().tile_mode = true;
    h.run_steps(2);
    let at = screen(&h, area, glam::vec3(25., 5., 0.));
    h.hover_at(at);
    press(&h, at, true, egui::Modifiers::NONE);
    press(&h, at, false, egui::Modifiers::NONE);
    h.run_steps(2);
    h.get_by_label("Advanced").click();
    h.run_steps(2);
    h.get_by_label("Apply Live Tile");
    if let Some(stage) = &stage {
        h.render().unwrap().save(out.join(format!("polish-tiles-{stage}.png"))).unwrap();
    }
    h.get_by_label("Close window").click();
    h.run_steps(2);
    assert!(!h.state().area_views[&area].tile_mode);
    h.state_mut().actions.push(mg_ui::Action::LiveOptions);
    h.run_steps(4);
    if let Some(stage) = &stage {
        h.render().unwrap().save(out.join(format!("polish-options-{stage}.png"))).unwrap();
    }
    h.get_by_label("Named clients").click();
    h.run_steps(3);
    if let Some(stage) = &stage {
        h.render().unwrap().save(out.join(format!("polish-clients-{stage}.png"))).unwrap();
    }
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    assert!(!h.state().has_unsaved_work());
}
