//! Inventory interaction proofs use a fresh fake store, never a game process.
#[path = "support/live_windows.rs"]
mod support;
use egui_kittest::kittest::{NodeT, Queryable};
use mg_ui::Tab;
use serde_json::json;

#[test]
fn inventory_grid_loads_on_open_and_selection_ignores_previous_reply() {
    let (app, store, _) = support::app("inventory-auto-load", None);
    let mut h = support::harness(app, Tab::LiveInventory);
    h.get_by_label("Loading inventory...");
    let first = support::commands(&store);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["op"], "view_inventory");
    assert_eq!(first[0]["target"], "7");
    h.run_steps(5);
    assert_eq!(support::commands(&store).len(), 1, "do not poll every frame");
    h.state_mut()
        .area_views
        .get_mut(&support::area())
        .unwrap()
        .live
        .select(Some("8".into()), false);
    h.run_steps(2);
    let commands = support::commands(&store);
    let second = commands.iter().find(|c| c["target"] == "8").unwrap();
    support::report(
        &store,
        2,
        vec![
            json!({"id":first[0]["id"],"ok":true,"value":{"items":[{"id":"a","name":"Previous holder's item","slot":-1}]}}),
        ],
    );
    h.run_steps(2);
    assert!(h.state().live_inventory.result.is_none());
    h.get_by_label("Loading inventory...");
    support::report(&store, 3, vec![json!({"id":second["id"],"ok":true,"value":{"items":[]}})]);
    h.run_steps(2);
    h.get_by_label("Inventory is empty.");
    h.get_by_label("Refresh inventory").click();
    h.run_steps(2);
    let refresh = support::commands(&store);
    assert_eq!(refresh.len(), 1);
    assert_eq!(refresh[0]["target"], "8");
}

#[test]
fn inventory_grid_drag_equips_the_exact_item() {
    let (app, store, _) = support::app("inventory-grid-drag", None);
    let mut h = support::harness(app, Tab::LiveInventory);
    support::inventory_reply(
        &mut h,
        json!({"items":[
        {"id":"a","name":"Gold Ring","resref":"nw_it_mring001","equip_slots":384,"stack":1,"slot":-1}]}),
    );
    h.run_steps(3);
    h.get_by_label("Backpack: Gold Ring").scroll_to_me();
    h.run_steps(3);
    let from = h.get_by_label("Backpack: Gold Ring").rect().center();
    let to = h.get_by_label("Left ring: empty").rect().center();
    h.event(egui::Event::PointerMoved(from));
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(1);
    for t in [0.1, 0.4, 0.7, 1.0] {
        h.event(egui::Event::PointerMoved(from + (to - from) * t));
        h.run_steps(1);
    }
    h.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
    let commands = support::commands(&store);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["op"], "equip_item");
    assert_eq!(commands[0]["target"], "7");
    assert_eq!(commands[0]["item"], "a");
    assert_eq!(commands[0]["slot"], 7);
    assert!(!h.state().has_unsaved_work());
}

#[test]
fn inventory_grid_unequip_and_confirmed_drop_are_native_commands() {
    for equipped in [true, false] {
        let (app, store, _) = support::app(
            if equipped { "inventory-grid-unequip" } else { "inventory-grid-drop" },
            None,
        );
        let mut h = support::harness(app, Tab::LiveInventory);
        support::inventory_reply(
            &mut h,
            json!({"items":[{"id":"a","name":"Gold Ring","resref":"nw_it_mring001","equip_slots":384,"stack":1,"slot":if equipped {7} else {-1},"droppable":true,"plot":false}]}),
        );
        h.run_steps(3);
        h.get_by_label(if equipped { "Left ring: Gold Ring" } else { "Backpack: Gold Ring" })
            .scroll_to_me();
        h.run_steps(3);
        h.get_by_label(if equipped { "Left ring: Gold Ring" } else { "Backpack: Gold Ring" })
            .click_secondary();
        h.run_steps(2);
        if equipped {
            h.get_by_label("Unequip").click();
        } else {
            h.get_by_label("Drop... \u{23f5}").click();
            h.run_steps(2);
            assert!(support::commands(&store).is_empty());
            h.get_by_label("Confirm drop").click();
        }
        h.run_steps(2);
        let commands = support::commands(&store);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["op"], if equipped { "unequip_item" } else { "drop_item" });
        assert_eq!(commands[0]["item"], "a");
        if equipped {
            assert_eq!(commands[0]["slot"], 7);
        }
        assert!(!h.state().has_unsaved_work());
    }
}

#[test]
fn inventory_actions_respect_masks_and_use_native_abilities() {
    let (app, store, _) = support::app("inventory-actions", None);
    let mut h = support::harness(app, Tab::LiveInventory);
    support::inventory_reply(
        &mut h,
        json!({"items":[
        {"id":"a","name":"Potion","slot":-1,"equip_slots":0,"uses":[{"property":2,"name":"Healing"}]},
        {"id":"b","name":"Ring","slot":-1,"equip_slots":384,"uses":[]}]}),
    );
    h.get_by_label("Backpack: Potion").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Backpack: Potion").click_secondary();
    h.run_steps(2);
    assert!(h.query_by_label("Equip \u{23f5}").is_none());
    h.get_by_label("Use \u{23f5}").click();
    h.run_steps(2);
    h.get_by_label("Healing on self").click();
    h.run_steps(2);
    let ops = support::commands(&store);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["op"], "use_item");
    assert_eq!(ops[0]["item"], "a");
    assert_eq!(ops[0]["property"], 2);
}

#[test]
fn inventory_ring_menu_limits_slots_and_can_transfer() {
    let (app, store, _) = support::app("inventory-transfer", None);
    let mut h = support::harness(app, Tab::LiveInventory);
    support::inventory_reply(
        &mut h,
        json!({"items":[{"id":"b","name":"Ring","slot":-1,"equip_slots":384,"uses":[]}]}),
    );
    h.get_by_label("Backpack: Ring").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Backpack: Ring").click_secondary();
    h.run_steps(2);
    assert!(h.query_by_label("Use \u{23f5}").is_none());
    h.get_by_label("Equip \u{23f5}").click();
    h.run_steps(2);
    assert!(h.query_by_label("Head").is_none());
    h.get_by_label("Left ring");
    h.get_by_label("Right ring");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.get_by_label("Backpack: Ring").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Backpack: Ring").click_secondary();
    h.run_steps(2);
    h.get_by_label("Give to... \u{23f5}").click();
    h.run_steps(2);
    h.get_by_label("Second guard (8)").click();
    h.run_steps(2);
    let ops = support::commands(&store);
    assert_eq!(ops[0]["op"], "transfer_item");
    assert_eq!(ops[0]["item"], "b");
    assert_eq!(ops[0]["recipient"], "8");
}

#[test]
fn inventory_nondroppable_potion_has_a_confirmed_dm_drop() {
    mg_testkit::gpu::hold();
    let (app, store, _) = support::app("inventory-drop-flags", None);
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1400., 950.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::default())
        .build_ui_state(|ui, app: &mut mg_ui::Moonglow| app.ui(ui), app);
    support::prime(&mut h, Tab::LiveInventory);
    support::inventory_reply(
        &mut h,
        json!({"items":[{"id":"a","name":"Potion of Cure Light Wounds","resref":"nw_it_mpotion001","slot":-1,"equip_slots":0,"droppable":false,"plot":false,"uses":[{"property":0,"name":"Cure Light Wounds"}]}]}),
    );
    h.get_by_label("Backpack: Potion of Cure Light Wounds").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Backpack: Potion of Cure Light Wounds").click_secondary();
    h.run_steps(3);
    let before = std::env::var("MGLIVE_ACCEPT5_STAGE").as_deref() == Ok("before");
    if let Ok(stage) = std::env::var("MGLIVE_ACCEPT5_STAGE") {
        assert!(matches!(stage.as_str(), "before" | "after"));
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/live-accept5-astra2");
        std::fs::create_dir_all(&out).unwrap();
        h.render().unwrap().save(out.join(format!("drop-{stage}.png"))).unwrap();
    }
    assert_eq!(h.get_by_label("Drop... \u{23f5}").accesskit_node().is_disabled(), before);
    if !before {
        h.get_by_label("Drop... \u{23f5}").click();
        h.run_steps(2);
        assert!(support::commands(&store).is_empty());
        assert!(h.query_by_label("Drop item on the ground here").is_none());
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/live-accept6-astra2/drop-after.png");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        h.render().unwrap().save(out).unwrap();
        h.get_by_label("Confirm drop").click();
        h.run_steps(2);
        let commands = support::commands(&store);
        assert_eq!(commands[0]["op"], "drop_item");
        assert_eq!(commands[0]["item"], "a");
    }
}
