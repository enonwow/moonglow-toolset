use super::*;

#[test]
fn live_npc_menu_card_memory_and_exclusions() {
    let Some((mut h, area)) = area_harness("live-npc-menu") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","tag":"menu_guard","name":"Gate guard","app":6,"portrait":"po_","x":20,"y":20,"hp":[30,30]}
        ]}]}),
    );
    h.state_mut().palette.area = Some(area);
    h.state_mut().live_focus.entries.clear();
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    review_open_menu(&mut h, area, false);
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-npc-menu");
    std::fs::create_dir_all(&out).unwrap();
    let before = std::env::var("MGLIVE_MENU_PHASE").as_deref() == Ok("before");
    h.render()
        .unwrap()
        .save(out.join(if before { "menu-before.png" } else { "menu-after.png" }))
        .unwrap();
    if before {
        assert!(h.query_by_label("NPC card").is_none());
        return;
    }
    h.get_by_label("NPC card").click();
    h.run_steps(3);
    let card = store.npc_card("a").unwrap();
    assert!(h.state().dock.find_tab(&Tab::LiveNpcCard(card.id)).is_some());
    h.get_by_label("Who they are");
    h.render().unwrap().save(out.join("card-after.png")).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveLog));
    h.run_steps(3);
    review_open_menu(&mut h, area, false);
    h.get_by_label("Memory").click();
    h.run_steps(3);
    let focus = &h.state().live_focus;
    assert!(focus.memory_page);
    assert_eq!(focus.entries[focus.selected.unwrap()].members[0].id, "a");
    h.get_by_label_contains("Saved NPC/player history");
    h.render().unwrap().save(out.join("memory-after.png")).unwrap();
    // Reopening Memory focuses the same saved entry instead of duplicating it.
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveLog));
    h.run_steps(3);
    review_open_menu(&mut h, area, false);
    h.get_by_label("Memory").click();
    h.run_steps(3);
    assert_eq!(h.state().live_focus.entries.len(), 1);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveLog));
    h.state_mut().settings.no_live_focus = true;
    h.run_steps(3);
    review_open_menu(&mut h, area, false);
    assert!(h.get_by_label("Memory").accesskit_node().is_disabled());
    assert!(!h.get_by_label("NPC card").accesskit_node().is_disabled());
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.state_mut().settings.no_live_focus = false;
    for (seq, flags) in [
        (2, serde_json::json!({"pc":true})),
        (3, serde_json::json!({"dm":true})),
        (4, serde_json::json!({"possessed":true})),
    ] {
        let mut creature = serde_json::json!({"id":"a","tag":"menu_guard","name":"Gate guard","x":20,"y":20,"app":6});
        creature.as_object_mut().unwrap().extend(flags.as_object().unwrap().clone());
        store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":seq,"areas":[{"area":area.to_string(),"full":true,"cre":[creature]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
        h.run_steps(3);
        review_open_menu(&mut h, area, false);
        assert!(h.query_by_label("NPC card").is_none());
        assert!(h.query_by_label("Memory").is_none());
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
    }
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
    assert!(!h.state().has_unsaved_work());
}
