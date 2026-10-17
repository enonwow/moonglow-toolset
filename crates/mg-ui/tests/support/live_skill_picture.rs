//! Current interactive skill panel; rendering uses only the test-owned GPU.
use crate::live_windows as support;
use egui_kittest::Harness;
use mg_ui::{Moonglow, Tab};
use serde_json::json;

#[test]
#[ignore]
fn live_skill_roll_snapshot() {
    mg_testkit::gpu::hold();
    let root = std::path::PathBuf::from(std::env::var("NWN_ROOT").unwrap());
    let (app, store, dir) = support::app("skill-roll-picture", Some(&root));
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
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
    support::prime(&mut h, Tab::LiveSkill);
    h.state_mut()
        .area_views
        .get_mut(&support::area())
        .unwrap()
        .live
        .select(Some("9".into()), false);
    let skill = &mut h.state_mut().live_skill;
    skill.bonus = 2;
    skill.show_dc = false;
    skill.success_text = "You notice fresh tracks beside the gate.".into();
    skill.result = Some(json!({"name":"Concentration","skill":1,"dc":15,"bonus":2,"players":[
        {"id":"9","status":"rolled","roll":14,"modifier":6,"total":22,"passed":true,"nui_token":2},
        {"id":"a","status":"waiting","nui_token":3},
        {"id":"b","status":"no_roll"}]}));
    h.run_steps(4);
    h.render().unwrap().save(dir.join("skill-roll.png")).unwrap();
}
