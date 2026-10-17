//! Appearance restore layout with installed resources and a fresh fake report.
use crate::live_windows as support;
use egui_kittest::{Harness, kittest::Queryable};
use mg_ui::{Moonglow, Tab};
use serde_json::json;

#[test]
#[ignore]
fn live_appearance_restore_snapshot() {
    mg_testkit::gpu::hold();
    let root = std::path::PathBuf::from(std::env::var("NWN_ROOT").unwrap());
    let (mut app, store, dir) = support::app("appearance-restore-picture", Some(&root));
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    app.set_render_state(rs.clone());
    let mut seq = 10;
    let mut h = Harness::builder().with_size(egui::vec2(1400., 950.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(move |ui, app: &mut Moonglow| {
            seq += 1;
            store.sync(json!({"v":1,"boot":"windows","seq":seq,"areas":[{"area":"start","full":true,"plc":[{"id":"7","name":"Hexxar's chest","app":5,"app_original":4,"x":3,"y":5,"z":0}]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
            app.ui(ui);
        }, app);
    support::prime(&mut h, Tab::PlaceableGallery);
    h.state_mut().placeable_gallery.as_mut().unwrap().filter = "Chest".into();
    h.run_steps(15);
    h.render().unwrap().save(dir.join("restore-placeable.png")).unwrap();
    h.get_by_label("Restore original appearance").click();
    h.run_steps(3);
    h.get_by_label("Original row 4");
    h.render().unwrap().save(dir.join("restore-confirm.png")).unwrap();
    assert!(!h.state().has_unsaved_work());
}
