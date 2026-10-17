//! VFX controls on installed rows; fake store, never the running demo.
use crate::live_windows as support;
use egui_kittest::{Harness, kittest::Queryable};
use mg_ui::{Moonglow, Tab};
#[test]
#[ignore]
fn live_vfx_transform_snapshot() {
    mg_testkit::gpu::hold();
    let root = std::path::PathBuf::from(std::env::var("NWN_ROOT").unwrap());
    let (mut app, store, dir) = support::app("vfx-transform-picture", Some(&root));
    app.live_vfx.search = "GLOW_RED".into();
    app.live_vfx.row = 411;
    app.live_vfx.scale = 1.5;
    app.live_vfx.offset = [1.25, -2.5, 3.75];
    app.live_vfx.rotation = [10., 20., 30.];
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
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
    support::prime(&mut h, Tab::LiveVfx);
    h.get_by_label("Scale / Offset / Rotation").click();
    h.run_steps(3);
    h.render().unwrap().save(dir.join("vfx-transform.png")).unwrap();
    assert!(!h.state().has_unsaved_work());
}
