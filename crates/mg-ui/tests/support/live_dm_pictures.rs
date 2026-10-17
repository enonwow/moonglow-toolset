//! Isolated Hexxar follow-up pictures: native client acceptance is separate.
use crate::live_windows as support;
use egui_kittest::Harness;
use mg_ui::{Moonglow, Tab};

#[test]
#[ignore]
fn hexxar_followup() {
    mg_testkit::gpu::hold();
    let root = std::path::PathBuf::from(std::env::var("NWN_ROOT").unwrap());
    let (mut app, store, dir) = support::app("dm-pictures", Some(&root));
    app.live_library.data_dir = Some(dir.join("data"));
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    app.set_render_state(rs.clone());
    let mut seq = 1;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400., 950.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(
            move |ui, app: &mut Moonglow| {
                seq += 1;
                support::report(&store, seq, vec![]);
                app.ui(ui);
            },
            app,
        );
    let stage = std::env::var("MGLIVE_PICTURE_STAGE").unwrap_or_else(|_| "after".into());
    assert!(matches!(stage.as_str(), "before" | "after"));
    for (tab, name) in [
        (Tab::LiveLibrary, "library"),
        (Tab::LiveSkill, "skill"),
        (Tab::LiveInventory, "inventory"),
    ] {
        support::prime(&mut h, tab);
        h.state_mut().live_library.name = "Hexxar guard".into();
        if name == "skill" {
            h.state_mut()
                .area_views
                .get_mut(&support::area())
                .unwrap()
                .live
                .select(Some("9".into()), false);
            h.state_mut().live_skill.result =
                Some(serde_json::json!({"players":[{"id":"9","passed":true,"nui_token":2}]}));
        }
        if name == "inventory" {
            h.state_mut().live_inventory.resref = "nw_it_mpotion001".into();
            h.state_mut().live_inventory.result = Some(serde_json::json!({"items":[
                {"id":"a","name":"Potion of Cure Light Wounds","resref":"nw_it_mpotion001","stack":3,"slot":-1},
                {"id":"b","name":"Longsword","resref":"nw_wswdls001","stack":1,"slot":4}],"truncated":false}));
        }
        h.run_steps(4);
        h.render().expect("render").save(dir.join(format!("{stage}-{name}.png"))).unwrap();
    }
}
