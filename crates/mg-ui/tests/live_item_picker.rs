//! Picker reads authored palettes and sends the exact chosen resource.
#[path = "support/live_windows.rs"]
mod support;
use egui_kittest::kittest::Queryable;
use mg_core::{Gender, Language, LocString, ResType};
use mg_gff::{Gff, Struct, Value};
use mg_resman::ResKey;
use mg_ui::Tab;
use serde_json::json;

fn palette_app(case: &str) -> (mg_ui::Moonglow, std::sync::Arc<mg_live::store::LiveStore>) {
    let (mut app, store, _) = support::app(case, None);
    let mut skeleton = Gff::new(*b"ITP ");
    let mut category = Struct::new(1);
    category.set("NAME", Value::String(b"Campaign medicines".to_vec()));
    category.set("ID", Value::Byte(0));
    skeleton.root.set("MAIN", Value::List(vec![category]));
    let ws = app.ws.as_mut().unwrap();
    ws.module.set_gff(ResKey::parse("itempal", ResType::ITP).unwrap(), &skeleton).unwrap();
    for (resref, name) in
        [("hexxar_tonic", "Hexxar Healing Tonic"), ("antidote", "Forest Antidote")]
    {
        let mut item = Gff::new(*b"UTI ");
        item.root.set(
            "LocalizedName",
            Value::LocString(LocString::from_text(
                Language::ENGLISH,
                Gender::Male,
                name.as_bytes().to_vec(),
            )),
        );
        item.root.set("PaletteID", Value::Byte(0));
        item.root.set("BaseItem", Value::Int(49));
        ws.module.set_gff(ResKey::parse(resref, ResType::UTI).unwrap(), &item).unwrap();
    }
    (app, store)
}

#[test]
fn inventory_picker_search_and_double_click_give_the_named_item() {
    let (app, store) = palette_app("inventory-picker-search");
    let mut h = egui_kittest::Harness::builder()
        .with_step_dt(1.0 / 60.0)
        .with_size(egui::vec2(1400., 950.))
        .build_ui_state(|ui, app: &mut mg_ui::Moonglow| app.ui(ui), app);
    support::prime(&mut h, Tab::LiveInventory);
    support::inventory_reply(&mut h, json!({"items":[]}));
    h.get_by_label("Give item...").click();
    h.run_steps(2);
    h.get_by_label("Search items").click();
    h.run_steps(1);
    h.get_by_label("Search items").type_text("medicines tonic");
    h.run_steps(2);
    assert!(h.query_by_label("Forest Antidote (antidote)").is_none());
    // Separate the search-focus click from the item double click (egui also detects triples).
    h.run_steps(40);
    let pos = h.get_by_label("Hexxar Healing Tonic (hexxar_tonic)").rect().center();
    h.event(egui::Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
    }
    h.run_steps(2);
    assert_eq!(
        h.state().live_inventory.resref,
        "hexxar_tonic",
        "item click reached picker; notice={:?}",
        h.state().live_inventory.notice
    );
    let commands = support::commands(&store);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["op"], "give_item");
    assert_eq!(commands[0]["resref"], "hexxar_tonic");
    assert_eq!(commands[0]["target"], "7");
}

#[test]
fn inventory_picker_select_then_give_and_selection_change_are_safe() {
    let (app, store) = palette_app("inventory-picker-give");
    let mut h = egui_kittest::Harness::builder()
        .with_step_dt(1.0 / 60.0)
        .with_size(egui::vec2(1400., 950.))
        .build_ui_state(|ui, app: &mut mg_ui::Moonglow| app.ui(ui), app);
    support::prime(&mut h, Tab::LiveInventory);
    support::inventory_reply(&mut h, json!({"items":[]}));
    h.get_by_label("Give item...").click();
    h.run_steps(2);
    h.get_by_label("Module").click();
    h.run_steps(3);
    h.get_by_label("Campaign medicines").click();
    h.run_steps(3);
    h.get_by_label("Forest Antidote (antidote)").click();
    h.run_steps(2);
    assert!(support::commands(&store).is_empty());
    h.get_by_label("Give").click();
    h.run_steps(2);
    assert_eq!(support::commands(&store)[0]["resref"], "antidote");
    // A different holder invalidates the pending picker/action context.
    h.state_mut()
        .area_views
        .get_mut(&support::area())
        .unwrap()
        .live
        .select(Some("8".into()), false);
    h.run_steps(2);
    assert!(h.query_by_label("Give item to Hexxar's guard").is_none());
}

#[test]
fn inventory_picker_real_palette_tree_and_search_picture() {
    mg_testkit::gpu::hold();
    let (mut app, _store) = palette_app("inventory-picker-tree");
    // Deliberately preserve palette order, which differs from alphabetical order.
    let ws = app.ws.as_mut().unwrap();
    let key = ResKey::parse("itempal", ResType::ITP).unwrap();
    let mut skeleton = ws.module.gff(&key).unwrap().unwrap();
    let child = skeleton.root.list("MAIN").unwrap()[0].clone();
    let mut parent = Struct::new(1);
    parent.set("NAME", Value::String(b"Campaign supplies".to_vec()));
    parent.set("LIST", Value::List(vec![child]));
    skeleton.root.set("MAIN", Value::List(vec![parent]));
    ws.module.set_gff(key, &skeleton).unwrap();
    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1400., 950.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::default())
        .build_ui_state(|ui, app: &mut mg_ui::Moonglow| app.ui(ui), app);
    support::prime(&mut h, Tab::LiveInventory);
    support::inventory_reply(&mut h, json!({"items":[]}));
    h.get_by_label("Give item...").click();
    h.run_steps(3);
    let before = std::env::var("MGLIVE_ACCEPT5_STAGE").as_deref() == Ok("before");
    if !before {
        h.get_by_label("Module").click();
        h.run_steps(3);
        h.get_by_label("Campaign supplies").click();
        h.run_steps(3);
        h.get_by_label("Campaign medicines").click();
        h.run_steps(3);
        h.get_by_label("Forest Antidote (antidote)");
        h.get_by_label("Module").click();
        h.run_steps(3);
        assert!(h.query_by_label("Forest Antidote (antidote)").is_none());
    }
    h.get_by_label("Search items").click();
    h.get_by_label("Search items").type_text("medicines tonic");
    h.run_steps(3);
    h.get_by_label("Hexxar Healing Tonic (hexxar_tonic)");
    assert!(h.query_by_label("Forest Antidote (antidote)").is_none());
    if let Ok(stage) = std::env::var("MGLIVE_ACCEPT5_STAGE") {
        assert!(matches!(stage.as_str(), "before" | "after"));
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/live-accept5-astra2");
        std::fs::create_dir_all(&out).unwrap();
        h.render().unwrap().save(out.join(format!("picker-{stage}.png"))).unwrap();
    }
}
