use super::*;

pub(super) fn before() -> bool {
    std::env::var("MGLIVE_ACCEPT5").as_deref() == Ok("before")
}

pub(super) fn picture(h: &mut Harness<'_, Moonglow>, name: &str) {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-accept5");
    std::fs::create_dir_all(&out).unwrap();
    h.render()
        .unwrap()
        .save(out.join(format!("{name}-{}.png", if before() { "before" } else { "after" })))
        .unwrap();
}

#[test]
fn live_accept5_portraits() {
    let Some((mut h, area)) = area_harness("live-accept5") else { return };
    let game = h.state().game.as_ref().unwrap();
    let appearance = game.table("appearance").unwrap();
    let beholder = (0..appearance.len())
        .find(|&r| appearance.get(r, "LABEL").is_some_and(|s| s.eq_ignore_ascii_case("Beholder")))
        .unwrap();
    for r in [7, beholder] {
        eprintln!(
            "portrait probe: app {r} label {:?} portrait {:?}",
            appearance.get(r, "LABEL"),
            appearance.get(r, "PORTRAIT")
        );
    }
    for size in ['s', 'm', 't', 'l', 'h'] {
        let name = format!("po_behold_{size}");
        eprintln!(
            "portrait probe: {name}: {}",
            game.resman.texture(name.parse().unwrap()).is_some()
        );
    }
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Appearance default","x":20,"y":20,"app":7,"portrait":"po_","hp":[10,10]},
            {"id":"b","name":"Beholder","x":24,"y":20,"app":beholder,"portrait":"po_behold_","hp":[10,10]}
        ]}]}),
    );
    h.state_mut().palette.area = Some(area);
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    view.live.select(Some("a".into()), false);
    view.live.select(Some("b".into()), true);
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":[
        {"seq":1,"kind":"chat","area":area.to_string(),"id":"a","text":"Hello from the street.","channel":1},
        {"seq":2,"kind":"chat","area":area.to_string(),"id":"b","text":"I see you.","channel":1}
    ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    for (tab, name) in
        [(Tab::LiveSelection, "selection-portraits"), (Tab::LiveChat, "chat-portraits")]
    {
        h.state_mut().actions.push(mg_ui::Action::OpenTab(tab));
        h.run_steps(4);
        picture(&mut h, name);
        if !before() {
            h.get_by_label("Portrait of Appearance default");
            h.get_by_label("Portrait of Beholder");
        }
    }
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
    assert!(!h.state().has_unsaved_work());
}
