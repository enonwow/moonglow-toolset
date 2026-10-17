use super::*;

#[test]
fn live_review_ctrl_drag_pans_and_ctrl_click_toggles_without_a_box() {
    let before = std::env::var("MGLIVE_PAN_PICTURE").as_deref() == Ok("before");
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Pan NPC","x":18,"y":20},
            {"id":"b","name":"Pan PC","pc":true,"x":24,"y":20}
        ],"plc":[{"id":"c","name":"Pan placeable","x":20,"y":24}]}]}),
    );
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("b".into()), false);
    h.run_steps(2);
    let ctrl = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
    let origin = h.state().area_views[&area].orbit.as_ref().unwrap().target;
    let from = screen(&h, area, glam::vec3(16., 16., 0.));
    let to = screen(&h, area, glam::vec3(26., 26., 0.));
    h.hover_at(from);
    press(&h, from, true, ctrl);
    h.run_steps(1);
    h.event_modifiers(egui::Event::PointerMoved(to), ctrl);
    h.run_steps(2);
    let moved = h.state().area_views[&area].orbit.as_ref().unwrap().target;
    assert_eq!(moved.distance(origin) > 1., !before);
    let stage = if before { "before" } else { "after" };
    h.render()
        .unwrap()
        .save(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../target/test-output/live-next/visual/ctrl-pan-{stage}.png")),
        )
        .unwrap();
    press(&h, to, false, ctrl);
    h.run_steps(2);
    if before {
        return;
    }
    assert_eq!(h.state().area_views[&area].live.selection_ids().collect::<Vec<_>>(), vec!["b"]);
    // A drag starting on a selected placeable pans, with no move command.
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("c".into()), false);
    h.run_steps(2);
    let from = screen(&h, area, glam::vec3(20., 24., 0.3));
    let to = from + egui::vec2(35., 20.);
    h.hover_at(from);
    press(&h, from, true, ctrl);
    h.run_steps(1);
    h.event_modifiers(egui::Event::PointerMoved(to), ctrl);
    h.run_steps(2);
    press(&h, to, false, ctrl);
    h.run_steps(2);
    assert!(h.state().area_views[&area].orbit.as_ref().unwrap().target.distance(moved) > 0.1);
    assert_eq!(h.state().area_views[&area].live.selection_ids().collect::<Vec<_>>(), vec!["c"]);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(None, false);
    // Clicks toggle both ways without moving the camera.
    let origin = h.state().area_views[&area].orbit.as_ref().unwrap().target;
    for (id, x, selected) in [("a", 18., true), ("b", 24., true), ("a", 18., false)] {
        let at = screen(&h, area, glam::vec3(x, 20., 0.3));
        h.hover_at(at);
        press(&h, at, true, ctrl);
        press(&h, at, false, ctrl);
        h.run_steps(2);
        assert_eq!(h.state().area_views[&area].live.is_selected(id), selected);
        assert_eq!(h.state().area_views[&area].orbit.as_ref().unwrap().target, origin);
    }
    // Plain boxes replace; Shift boxes add. Neither pans the camera.
    for (x, modifiers, count) in [(18., egui::Modifiers::NONE, 1), (24., egui::Modifiers::SHIFT, 2)]
    {
        let at = screen(&h, area, glam::vec3(x, 20., 0.3));
        let rect = egui::Rect::from_center_size(at, egui::vec2(36., 36.));
        h.hover_at(rect.min);
        press(&h, rect.min, true, modifiers);
        h.run_steps(1);
        h.event_modifiers(egui::Event::PointerMoved(rect.max), modifiers);
        h.run_steps(2);
        press(&h, rect.max, false, modifiers);
        h.run_steps(2);
        assert!(h.state().area_views[&area].live.is_selected("a"));
        assert_eq!(h.state().area_views[&area].live.selection_ids().count(), count);
        assert_eq!(h.state().area_views[&area].orbit.as_ref().unwrap().target, origin);
    }
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_review_filter_notice_restores_chips_search_and_kinds() {
    use mg_area::ObjectKind;
    let before = std::env::var("MGLIVE_FILTER_PICTURE").as_deref() == Ok("before");
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Filter NPC","x":18,"y":20},
            {"id":"b","name":"Filter PC","pc":true,"x":24,"y":20}
        ]}]}),
    );
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LivePanel));
    h.run_steps(3);
    h.get_by_label("Players").click();
    h.run_steps(3);
    let picture = |h: &mut Harness<'_, Moonglow>, name: &str| {
        h.render()
            .unwrap()
            .save(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../../target/test-output/live-next/visual/filter-{name}.png")),
            )
            .unwrap();
    };
    if before {
        assert!(h.query_by_label("1 of 2 hidden by filters").is_none());
        picture(&mut h, "before");
        return;
    }
    assert!(h.query_by_label("1 of 2 hidden by filters").is_some());
    picture(&mut h, "notice");
    let reads = h.state().area_views[&area].live.report_reads;
    h.get_by_label("Show all").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("hidden by filters").is_none());
    assert!(h.state().area_views[&area].live.filter.enabled.is_empty());
    assert_eq!(h.state().area_views[&area].live.report_reads, reads);
    for (id, x) in [("a", 18.), ("b", 24.)] {
        let at = screen(&h, area, glam::vec3(x, 20., 0.3));
        h.hover_at(at);
        press(&h, at, true, egui::Modifiers::NONE);
        press(&h, at, false, egui::Modifiers::NONE);
        h.run_steps(2);
        assert_eq!(h.state().area_views[&area].live.selected.as_deref(), Some(id));
    }
    picture(&mut h, "restored");
    // Toolbar kind toggles and the panel search share the same recovery action.
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    view.live.filter.search = "missing".into();
    for kind in [ObjectKind::Creature, ObjectKind::Placeable, ObjectKind::Door, ObjectKind::Item] {
        view.show[kind.index()] = false;
    }
    h.run_steps(2);
    h.get_by_label("2 of 2 hidden by filters");
    h.get_by_label("Show all").click();
    h.run_steps(3);
    let view = &h.state().area_views[&area];
    assert!(view.live.filter.search.is_empty());
    for kind in [ObjectKind::Creature, ObjectKind::Placeable, ObjectKind::Door, ObjectKind::Item] {
        assert!(view.show[kind.index()]);
    }
    assert!(h.query_by_label_contains("hidden by filters").is_none());
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_review_accept3_radius_aim_picture() {
    let stage = std::env::var("MGLIVE_AIM_PICTURE").unwrap_or_else(|_| "after".into());
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Skill radius centre","x":20,"y":20}]}]}),
    );
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveSkill));
    h.run_steps(4);
    h.get_by_label("Send skill check").click();
    h.run_steps(2);
    let at = screen(&h, area, glam::vec3(28., 28., 0.));
    h.hover_at(at);
    h.run_steps(3);
    let queued = store.status(mg_live::now_ms())["queued"].as_array().unwrap().len();
    assert_eq!(queued, if stage == "before" { 1 } else { 0 });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../target/test-output/live-next/visual/accept3-skill-aim-{stage}.png"));
    h.render().unwrap().save(path).unwrap();
}

#[test]
fn live_review_accept3_radius_vfx_jump_are_click_only_and_cancel() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"Radius source","x":20,"y":20}]}]}),
    );
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveSkill));
    h.run_steps(4);
    h.get_by_label("Send skill check").click();
    h.run_steps(2);
    review_number(&mut h, "Radius", "4");
    let at = screen(&h, area, glam::vec3(28., 28., 0.));
    h.hover_at(at);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Crosshair);
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    h.get_by_label("Send skill check").click();
    h.run_steps(2);
    h.hover_at(at);
    press(&h, at, true, egui::Modifiers::NONE);
    press(&h, at, false, egui::Modifiers::NONE);
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"][0].clone();
    assert_eq!(q["op"], "skill_check");
    assert_eq!(q["radius"], 4.);
    assert!((q["x"].as_f64().unwrap() - 28.).abs() < 0.2);
    assert!(q.get("target").is_none());
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveVfx));
    h.run_steps(4);
    h.get_by_label("Aim on the ground").click();
    h.get_by_label("Instant").click();
    h.run_steps(2);
    h.get_by_label("Apply VFX").click();
    h.run_steps(2);
    assert_eq!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().len(), 1);
    h.hover_at(at);
    press(&h, at, true, egui::Modifiers::NONE);
    press(&h, at, false, egui::Modifiers::NONE);
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"][1].clone();
    assert_eq!(q["op"], "apply_vfx");
    assert!((q["location"]["x"].as_f64().unwrap() - 28.).abs() < 0.2);
    for down in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed: down,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(2);
    h.get_by_label("Jump All Players\u{2026}").click();
    h.run_steps(2);
    assert_eq!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().len(), 2);
    h.hover_at(at);
    press(&h, at, true, egui::Modifiers::NONE);
    press(&h, at, false, egui::Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(store.status(mg_live::now_ms())["queued"][2]["op"], "jump_all_players");
}

#[test]
fn live_review_accept3_library_spawn_has_npc_ghost_and_waits_for_map_click() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(&mut h, area, serde_json::json!({"areas":[{"cre":[]}]}));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-next/accept3-library");
    let module =
        h.state().module_path().unwrap().file_stem().unwrap().to_string_lossy().into_owned();
    let mut gff = mg_gff::Gff::new(*b"UTC ");
    gff.root = mg_preview::CreatureLook::new(6).to_utc();
    gff.root.set("Tag", mg_gff::Value::String(b"accept3_owned_ghost".to_vec()));
    let json = mg_gff::to_json(&gff, h.state().game.as_ref().unwrap().codepage()).unwrap();
    if let Ok(old) = mg_ui::live_library::load(&root, &module, "Ghost guard") {
        assert_eq!(old["Tag"]["value"], "accept3_owned_ghost");
        if old != json {
            mg_ui::live_library::delete(&root, &module, "Ghost guard").unwrap();
        }
    }
    if mg_ui::live_library::list(&root, &module).unwrap().is_empty() {
        mg_ui::live_library::save(&root, &module, "Ghost guard", &json).unwrap();
    }
    h.state_mut().area_views.get_mut(&area).unwrap().orbit = Some(mg_ui::area_view::Orbit {
        target: glam::vec3(25., 25., 0.8),
        yaw: -1.57,
        pitch: 0.65,
        distance: 9.,
    });
    h.state_mut().live_library.data_dir = Some(root);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveLibrary));
    h.run_steps(4);
    h.get_by_label("Refresh library").click();
    h.run_steps(2);
    h.get_by_label("Ghost guard").click();
    h.run_steps(2);
    h.get_by_label("Spawn NPC").click();
    h.run_steps(2);
    let at = screen(&h, area, glam::vec3(25., 25., 0.));
    h.hover_at(at);
    h.run_steps(3);
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    assert!(
        h.state().area_views[&area]
            .ghost_shown
            .as_ref()
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .parts
            .len()
            > 10
    );
    h.render()
        .unwrap()
        .save(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-output/live-next/visual/accept3-library-ghost-after.png"),
        )
        .unwrap();
    press(&h, at, true, egui::Modifiers::NONE);
    press(&h, at, false, egui::Modifiers::NONE);
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"][0].clone();
    assert_eq!(q["op"], "spawn_json");
    assert_eq!(q["json"], json);
    assert!((q["x"].as_f64().unwrap() - 25.).abs() < 0.2);
}

#[test]
fn live_review_accept3_group_actions_include_allowed_players_and_each_eligible_target() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[{"id":"a","name":"NPC","x":20,"y":20},{"id":"b","name":"Player","pc":true,"x":22,"y":20},{"id":"c","name":"Dead","dead":true,"x":24,"y":20}]}]}),
    );
    for id in ["a", "b", "c"] {
        h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some(id.into()), true);
    }
    let at = screen(&h, area, glam::vec3(28., 28., 0.));
    let open = |h: &mut Harness<'_, Moonglow>| {
        h.hover_at(at);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run_steps(2);
    };
    open(&mut h);
    for label in
        ["Move", "Combat", "Character", "Look and effects", "Talk", "Player", "Scripts", "DANGER"]
    {
        h.get_by_label(&format!("{label} \u{23f5}"));
    }
    assert!(h.query_by_label("Inventory (to the Live Log)").is_none());
    assert!(h.query_by_label("Inventory\u{2026}").is_none());
    review_click(&mut h, "Kill");
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"].clone();
    assert_eq!(q.as_array().unwrap().len(), 2);
    assert!(q.as_array().unwrap().iter().all(|c| c["op"] == "kill" && c["target"] != "c"));
    open(&mut h);
    review_click(&mut h, "Heal ⏵");
    h.run_steps(2);
    review_number(&mut h, "Heal amount", "23");
    h.get_by_label("Heal Amount").click();
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"].clone();
    assert_eq!(q.as_array().unwrap().len(), 4);
    assert_eq!(q[2]["amount"], 23);
    assert_eq!(q[3]["amount"], 23);
    assert!(q.as_array().unwrap().iter().any(|c| c["target"] == "b" && c["op"] == "pc_heal"));
    open(&mut h);
    review_click(&mut h, "Limbo");
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"].clone();
    assert_eq!(q.as_array().unwrap().len(), 5);
    assert_eq!(q[4]["target"], "a");
    store.set_policy(mg_live::store::Policy { players: false, ..Default::default() });
    let before = store.status(mg_live::now_ms())["queued"].as_array().unwrap().len();
    open(&mut h);
    review_click(&mut h, "Force Rest");
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"].clone();
    assert_eq!(q.as_array().unwrap().len(), before + 1);
    assert_eq!(q[before]["target"], "a");
    assert_eq!(q[before]["op"], "force_rest");
}

#[test]
fn live_effects_accept3_player_lasting_vfx_picture() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let table = h.state().game.as_ref().unwrap().table("visualeffects").unwrap();
    let row = (0..table.len())
        .find(|&r| table.get(r, "Label") == Some("VFX_DUR_MIND_AFFECTING_NEGATIVE"))
        .unwrap();
    let mut look = mg_gff::Gff::new(*b"UTC ");
    look.root = mg_preview::CreatureLook::new(6).to_utc();
    let json = mg_gff::to_json(&look, h.state().game.as_ref().unwrap().codepage()).unwrap();
    review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Player with lasting mind effect","pc":true,"x":25,"y":25,"look":json,"vfx":[row]}
        ]}]}),
    );
    h.state_mut().area_views.get_mut(&area).unwrap().orbit = Some(mg_ui::area_view::Orbit {
        target: glam::vec3(25., 25., 1.),
        yaw: -1.57,
        pitch: 0.35,
        distance: 6.,
    });
    let stage = std::env::var("MGLIVE_VFX_PICTURE").unwrap_or_else(|_| "after".into());
    h.state_mut().area_views.get_mut(&area).unwrap().live.effects =
        mg_ui::live_effects::EffectsMode::Markers;
    h.run_steps(4);
    h.render()
        .unwrap()
        .save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/test-output/live-next/visual/accept3-player-markers-{stage}.png"
        )))
        .unwrap();
    if stage == "before" {
        h.state_mut().area_views.get_mut(&area).unwrap().live.effects =
            mg_ui::live_effects::EffectsMode::Full;
    } else {
        h.get_by_label("Show Full effects").click();
    }
    h.run_steps(90);
    let v = &h.state().area_views[&area];
    eprintln!(
        "player mind VFX row={row}, models={}, particles={}",
        v.effect_models_shown, v.live_particles_shown
    );
    assert!(v.effect_models_shown > 0);
    assert!(v.live_particles_shown > 0, "the player look actually draws the effect emitters");
    assert_eq!(v.live.effects, mg_ui::live_effects::EffectsMode::Full);
    let stage = std::env::var("MGLIVE_VFX_PICTURE").unwrap_or_else(|_| "after".into());
    h.render()
        .unwrap()
        .save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/test-output/live-next/visual/accept3-player-vfx-{stage}.png"
        )))
        .unwrap();
}

#[test]
fn live_review_accept3_animation_menu_and_portrait_baselines() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let stage = std::env::var("MGLIVE_CONTROLS_PICTURE").unwrap_or_else(|_| "after".into());
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Player","pc":true,"x":20,"y":20,"hp":[19,24],"portrait":"hu_m_01_"},
            {"id":"b","name":"Guard","x":25,"y":20,"hp":[15,20],"portrait":"hu_m_02_"}
        ]}]}),
    );
    review_open_menu(&mut h, area, true);
    if stage != "before" {
        review_click(&mut h, "Animation ⏵");
        h.run_steps(3);
        h.get_by_label("Custom animation name");
        h.get_by_label("Loop animation");
        h.get_by_label("Animation speed");
        assert!(!h.get_by_label("Play animation").accesskit_node().is_disabled());
        h.get_by_label("ANIMATION_* constants").click();
        h.run_steps(2);
        h.run_steps(4);
        h.run_steps(2); // Let the nested popup finish laying out before clicking.
        h.get_by_label("ANIMATION_FIREFORGET_BOW").click();
        h.run_steps(2);
        h.get_by_label("Custom animation").click();
        h.run_steps(2);
        assert!(
            h.get_by_label("Play animation").accesskit_node().is_disabled(),
            "empty custom names cannot send"
        );
        h.get_by_label("Custom animation name").click();
        h.event(egui::Event::Text("dance_custom".into()));
        h.run_steps(2);
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-next/visual");
    h.render().unwrap().save(root.join(format!("accept3-animation-{stage}.png"))).unwrap();
    if stage != "before" {
        assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
        h.get_by_label("Loop animation").click();
        h.run_steps(2);
        review_number(&mut h, "Animation speed", "1.5");
        review_number(&mut h, "Animation seconds", "12");
        h.get_by_label("Play animation").click();
        h.run_steps(2);
        let q = store.status(mg_live::now_ms())["queued"][0].clone();
        assert_eq!(q["op"], "play_creature_animation");
        assert_eq!(q["target"], "a");
        assert_eq!(q["animation"], "dance_custom");
        assert_eq!(q["loop"], true);
        assert_eq!(q["speed"], 1.5);
        assert_eq!(q["seconds"], 12.);
    }
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let v = h.state_mut().area_views.get_mut(&area).unwrap();
    v.live.select(Some("a".into()), false);
    v.live.select(Some("b".into()), true);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveSelection));
    h.run_steps(4);
    if stage == "before" {
        h.render().unwrap().save(root.join(format!("accept3-portraits-{stage}.png"))).unwrap();
    }
    if stage != "before" {
        review_open_menu(&mut h, area, false);
        review_click(&mut h, "Animation ⏵");
        h.run_steps(2);
        assert!(!h.get_by_label("Play animation").accesskit_node().is_disabled());
        h.get_by_label("Play animation").click();
        h.run_steps(2);
        let q = store.status(mg_live::now_ms())["queued"].as_array().unwrap().clone();
        assert_eq!(q.len(), 3);
        assert_eq!(q[1]["target"], "b");
        assert_eq!(q[2]["target"], "a");
        assert_eq!(q[1]["animation"], 0);
        assert_eq!(q[2]["op"], "play_creature_animation");
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
        let mut policy = store.policy();
        policy.players = false;
        store.set_policy(policy);
        review_open_menu(&mut h, area, false);
        review_reveal(&mut h, "Animation ⏵");
        assert!(h.get_by_label("Animation ⏵").accesskit_node().is_disabled());
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        let pos = screen(&h, area, glam::vec3(25., 20., 0.3));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run_steps(2);
        review_click(&mut h, "Animation ⏵");
        h.run_steps(2);
        h.get_by_label("Custom animation name");
        let count = store.status(mg_live::now_ms())["queued"].as_array().unwrap().len();
        h.get_by_label("ANIMATION_* constants").click();
        h.run_steps(2);
        h.run_steps(4);
        h.run_steps(2); // Let the nested popup finish laying out before clicking.
        h.get_by_label("ANIMATION_FIREFORGET_BOW").click();
        h.run_steps(2);
        h.get_by_label("Play animation").click();
        h.run_steps(2);
        let q = store.status(mg_live::now_ms())["queued"].as_array().unwrap().clone();
        assert_eq!(q.len(), count + 1);
        assert_eq!(q[count]["target"], "b");
        assert_eq!(q[count]["animation"], 105);
    }
}

#[test]
fn live_review_accept3_take_values_and_group_removal_gate_players() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Player","pc":true,"x":20,"y":20,"level":3},
            {"id":"b","name":"Guard","x":25,"y":20,"level":2},
            {"id":"c","name":"Novice","x":30,"y":20,"level":1}
        ]}]}),
    );
    review_open_menu(&mut h, area, true);
    review_click(&mut h, "Take Gold ⏵");
    h.run_steps(2);
    review_number(&mut h, "Gold to take", "25");
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    h.get_by_label("Take Gold Now").click();
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"][0].clone();
    assert_eq!(q["op"], "take_gold");
    assert_eq!(q["target"], "a");
    assert_eq!(q["amount"], 25);
    review_open_menu(&mut h, area, true);
    review_click(&mut h, "Take XP ⏵");
    h.run_steps(2);
    review_number(&mut h, "XP to take", "35");
    h.render()
        .unwrap()
        .save(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-output/live-next/visual/accept3-take-after.png"),
        )
        .unwrap();
    h.get_by_label("Take XP Now").click();
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"][1].clone();
    assert_eq!(q["op"], "take_xp");
    assert_eq!(q["target"], "a");
    assert_eq!(q["amount"], 35);
    review_open_menu(&mut h, area, true);
    review_click(&mut h, "Remove Level");
    h.run_steps(2);
    assert_eq!(store.status(mg_live::now_ms())["queued"][2]["op"], "remove_level");
    let v = h.state_mut().area_views.get_mut(&area).unwrap();
    v.live.select(Some("b".into()), true);
    v.live.select(Some("c".into()), true);
    review_open_menu(&mut h, area, false);
    review_click(&mut h, "Remove Level");
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"].as_array().unwrap().clone();
    assert_eq!(q.len(), 5);
    assert_eq!(q[3]["target"], "b");
    assert_eq!(q[4]["target"], "a");
    let mut policy = store.policy();
    policy.players = false;
    store.set_policy(policy);
    let count = store.status(mg_live::now_ms())["queued"].as_array().unwrap().len();
    review_open_menu(&mut h, area, false);
    review_click(&mut h, "Take Gold ⏵");
    h.run_steps(2);
    h.get_by_label("Take Gold Now").click();
    h.run_steps(2);
    let q = store.status(mg_live::now_ms())["queued"].as_array().unwrap().clone();
    assert_eq!(q.len(), count + 2);
    assert!(q[count..].iter().all(|q| q["target"] != "a"));
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    review_open_menu(&mut h, area, false);
    review_reveal(&mut h, "Take Gold ⏵");
    for label in ["Take Gold ⏵", "Take XP ⏵", "Remove Level"] {
        assert!(h.get_by_label(label).accesskit_node().is_disabled());
    }
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn live_review_accept3_selection_portraits_focus_without_changing_the_group() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    let game = h.state().game.as_ref().unwrap();
    let table = game.table("portraits").unwrap();
    let bases: Vec<_> = (1..table.len())
        .filter_map(|row| {
            let base = format!("po_{}", table.get(row, "BaseResRef")?);
            [mg_core::ResType::TGA, mg_core::ResType::DDS]
                .iter()
                .any(|ext| game.resman.get_named(&format!("{base}s"), *ext).is_ok())
                .then_some(base)
        })
        .take(2)
        .collect();
    assert_eq!(bases.len(), 2);
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Player","pc":true,"x":20,"y":20,"hp":[19,24],"portrait":bases[0]},
            {"id":"b","name":"Guard","x":25,"y":20,"hp":[15,20],"portrait":bases[1]}
        ]}]}),
    );
    let v = h.state_mut().area_views.get_mut(&area).unwrap();
    v.live.select(Some("a".into()), false);
    v.live.select(Some("b".into()), true);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveSelection));
    h.run_steps(4);
    h.get_by_label("Portrait of Player");
    h.get_by_label("Portrait of Guard").click();
    h.run_steps(2);
    assert_eq!(h.state().area_views[&area].live.selection_ids().count(), 2);
    assert!(
        h.state().area_views[&area].camera().unwrap().target.distance(glam::vec3(25., 20., 0.))
            < 0.01
    );
    h.get_by_label("19/24");
    h.get_by_label("15/20");
    assert!(store.status(mg_live::now_ms())["queued"].as_array().unwrap().is_empty());
    h.render()
        .unwrap()
        .save(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-output/live-next/visual/accept3-portraits-after.png"),
        )
        .unwrap();
    assert!(!h.state().has_unsaved_work());
}
