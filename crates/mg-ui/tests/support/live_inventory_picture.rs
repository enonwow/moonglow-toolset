//! Real installed item icons, fake live holder; no engine or demo process.
use crate::live_windows as support;
use egui_kittest::{Harness, kittest::Queryable};
use mg_ui::{Moonglow, Tab};
use serde_json::json;

#[test]
#[ignore]
fn live_inventory_grid_snapshot() {
    mg_testkit::gpu::hold();
    let root = std::path::PathBuf::from(std::env::var("NWN_ROOT").unwrap());
    let (app, store, dir) = support::app("inventory-grid-picture", Some(&root));
    // Polish captures retain the worker's accepted baseline pictures.
    let dir = if std::env::var("MGLIVE_POLISH_PICTURE").as_deref() == Ok("after") {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/live-next/visual/inventory-after");
        std::fs::create_dir_all(&out).unwrap();
        out
    } else {
        dir
    };
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let reply = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reply_in_frame = reply.clone();
    let mut seq = 10;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400., 1050.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(
            move |ui, app: &mut Moonglow| {
                seq += 1;
                let replies = support::commands(&store)
                    .iter()
                    .filter(|c| {
                        reply_in_frame.load(std::sync::atomic::Ordering::Relaxed)
                            && c["op"] == "view_inventory"
                    })
                    .map(|c| json!({"id":c["id"],"ok":true,"value":{"items":[]}}))
                    .collect();
                support::report(&store, seq, replies);
                app.ui(ui);
            },
            app,
        );
    support::prime(&mut h, Tab::LiveInventory);
    h.render().unwrap().save(dir.join("inventory-loading.png")).unwrap();
    reply.store(true, std::sync::atomic::Ordering::Relaxed);
    h.run_steps(3);
    h.state_mut().live_inventory.gold_amount = 25;
    h.state_mut().live_inventory.resref = "nw_it_mpotion001".into();
    h.state_mut().live_inventory.result = Some(json!({"items":[
        {"id":"a","name":"Gold Ring","resref":"nw_it_mring001","stack":1,"slot":7,"look":{"BaseItem":52,"ModelPart1":4,"xModelPart1":4}},
        {"id":"b","name":"Longsword","resref":"nw_wswls001","stack":1,"slot":4},
        {"id":"c","name":"Healing Potion","resref":"nw_it_mpotion001","stack":3,"slot":-1,"equip_slots":0,"uses":[{"property":0,"name":"Cure Light Wounds"}]},
        {"id":"d","name":"Gold Ring","resref":"nw_it_mring001","stack":1,"slot":-1},
        {"id":"e","name":"Longsword","resref":"nw_wswls001","stack":1,"slot":-1}],"truncated":false}));
    h.run_steps(4);
    h.render().unwrap().save(dir.join("inventory-grid.png")).unwrap();
    h.get_by_label("Left ring: Gold Ring").click_secondary();
    h.run_steps(3);
    h.render().unwrap().save(dir.join("inventory-context.png")).unwrap();
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.get_by_label("Backpack: Healing Potion").click_secondary();
    h.run_steps(2);
    h.get_by_label("Use \u{23f5}").click();
    h.run_steps(2);
    h.render().unwrap().save(dir.join("inventory-use.png")).unwrap();
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    h.get_by_label("Give item...").click();
    h.run_steps(2);
    h.get_by_label("Search items").click();
    h.run_steps(1);
    h.get_by_label("Search items").type_text("potion");
    h.run_steps(4);
    h.render().unwrap().save(dir.join("inventory-picker.png")).unwrap();
    assert!(!h.state().has_unsaved_work());
}
