//! Offline reserve and live session pane, using authored module and fake reports.
use crate::live_windows as support;
use egui_kittest::{Harness, kittest::Queryable};
use mg_ui::{Moonglow, Tab};
#[test]
#[ignore]
fn live_session_factions_snapshot() {
    mg_testkit::gpu::hold();
    let root = std::path::PathBuf::from(std::env::var("NWN_ROOT").unwrap());
    let (app, store, dir) = support::app("session-factions-picture", Some(&root));
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    store.sync(serde_json::json!({"v":1,"boot":"windows","seq":2,"factions":[{"id":0,"name":"PC"},{"id":1,"name":"Hostile"},{"id":2,"name":"Commoner"},{"id":3,"name":"Merchant"},{"id":4,"name":"Defender"}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    let mut seq = 20;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400., 1050.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(
            move |ui, app: &mut Moonglow| {
                seq += 1;
                support::report(&store, seq, vec![]);
                app.ui(ui);
            },
            app,
        );
    support::prime(&mut h, Tab::Factions);
    h.state_mut().faction_view.session_spares = 2;
    h.get_by_label("Add session factions").click();
    h.run_steps(3);
    h.render().unwrap().save(dir.join("offline-reserve.png")).unwrap();
    h.get_by_label("Live session").click();
    h.run_steps(3);
    h.state_mut().faction_view.live.faction = 5;
    h.run_steps(2);
    h.state_mut().faction_view.live.label = "Dock ambushers".into();
    h.get_by_label("Save label").click();
    h.run_steps(3);
    h.render().unwrap().save(dir.join("live-session.png")).unwrap();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-accept6-astra2/factions-after.png");
    h.render().unwrap().save(out).unwrap();
}
