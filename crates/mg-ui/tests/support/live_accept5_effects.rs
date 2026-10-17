use super::live_accept5::{before, picture};
use super::*;

#[test]
fn live_accept5_effects() {
    let Some((mut h, area)) = area_harness("live-accept5") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Freedom of movement","r":"nw_bartender","x":20,"y":20,"app":6,"vfx":[3]},
            {"id":"b","name":"Fireball target","r":"nw_bartender","x":28,"y":24,"app":6}
        ]}]}),
    );
    h.state_mut().palette.area = Some(area);
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.live.hide_module_markers = true;
        view.live.markers.hidden.insert("NPCs".into());
        let orbit = view.orbit.as_mut().unwrap();
        orbit.target = glam::vec3(20., 20., 0.7);
        orbit.distance = 12.;
        orbit.pitch = 55_f32.to_radians();
        if before() {
            view.live.effects = mg_ui::live_effects::EffectsMode::Markers;
        }
    }
    if !before() {
        h.state_mut().area_views.get_mut(&area).unwrap().live.effects =
            mg_ui::live_effects::EffectsMode::Off;
        h.run_steps(3);
        assert_eq!(h.state().area_views[&area].live_particles_shown, 0);
        picture(&mut h, "lasting-effects-off");
        h.state_mut().area_views.get_mut(&area).unwrap().live.effects =
            mg_ui::live_effects::EffectsMode::Full;
    }
    h.run_steps(12);
    picture(&mut h, "lasting-effects");
    if !before() {
        assert_eq!(
            h.state().area_views[&area].live.effects,
            mg_ui::live_effects::EffectsMode::Full
        );
        assert!(h.state().area_views[&area].effect_models_shown > 0);
        assert!(h.state().area_views[&area].live_particles_shown > 0);
    }
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.live.markers.hidden.remove("NPCs");
        let orbit = view.orbit.as_mut().unwrap();
        orbit.target = glam::vec3(24., 20., 0.);
        orbit.distance = 26.;
    }
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"spell","area":area.to_string(),"id":"a","position":{"x":28,"y":24,"z":0},"spell":58,"instant":false},
        {"seq":2,"kind":"vfx","subtype":"projectile","area":area.to_string(),"id":"a","position":{"x":28,"y":24,"z":0},"spell":58,"delta_ms":700}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(10);
    picture(&mut h, "spell-projectile");
    if !before() {
        assert!(h.state().area_views[&area].transient_models_shown >= 2);
    }
    h.run_steps(38);
    picture(&mut h, "spell-impact");
    if !before() {
        assert!(h.state().area_views[&area].transient_models_shown > 0);
    }
    h.run_steps(20);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::LiveVfx));
    h.run_steps(4);
    h.state_mut().live_vfx.row = 22;
    h.state_mut().live_vfx.duration = "instant".into();
    h.get_by_label("Aim on the ground").click();
    h.run_steps(2);
    h.get_by_label("Apply VFX").click();
    h.run_steps(2);
    let point = screen(&h, area, glam::vec3(24., 16., 0.));
    h.hover_at(point);
    press(&h, point, true, egui::Modifiers::NONE);
    press(&h, point, false, egui::Modifiers::NONE);
    h.run_steps(2);
    let q = store.commands(mg_live::now_ms());
    let q = &q["commands"][0];
    assert_eq!(q["op"], "apply_vfx");
    assert!(q["location"].is_object());
    h.state_mut().area_views.get_mut(&area).unwrap().live.effects =
        mg_ui::live_effects::EffectsMode::Markers;
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":3,"areas":[],"results":[{"id":q["id"],"ok":true}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(3);
    picture(&mut h, "ground-vfx-markers");
    if !before() {
        h.state_mut().area_views.get_mut(&area).unwrap().live.effects =
            mg_ui::live_effects::EffectsMode::Full;
    }
    h.run_steps(8);
    picture(&mut h, "ground-vfx");
    if !before() {
        assert!(h.state().area_views[&area].transient_models_shown > 0);
    }
}
