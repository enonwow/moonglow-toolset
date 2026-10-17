use super::live_accept5::{before, picture};
use super::*;

#[test]
fn live_accept5_teleport_map() {
    let Some((mut h, area)) = area_harness("live-accept5") else { return };
    let game = h.state().game.as_ref().unwrap().clone();
    let mut rng = fastrand::Rng::with_seed(78);
    let second = mg_module::new::add_area(
        &mut h.state_mut().ws.as_mut().unwrap().module,
        &game,
        &mg_module::new::AreaSpec {
            name: "Unreported destination".into(),
            tileset: "ttr01".parse().unwrap(),
            width: 4,
            height: 4,
        },
        &mut rng,
    )
    .unwrap();
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"size":[40,40],"cre":[
            {"id":"a","name":"Street lamplighter","r":"nw_bartender","x":20,"y":20,"app":6}
        ]}]}),
    );
    review_open_menu(&mut h, area, false);
    review_click(&mut h, "Teleport to Area…");
    h.run_steps(4);
    picture(&mut h, "teleport-map");
    h.get_all_by_value(&area.to_string())
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::ComboBox)
        .expect("destination area dropdown")
        .click();
    h.run_steps(2);
    picture(&mut h, "teleport-areas");
    if !before() {
        let destination = second.to_string();
        let options: Vec<_> = h.get_all_by_label(&destination).collect();
        assert_eq!(options.len(), 2, "tree entry and destination option");
        options.last().unwrap().click();
        h.run_steps(3);
        h.get_by_label("Destination map");
        picture(&mut h, "teleport-unreported");
        assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
        h.get_by_label("Destination map").click();
        h.run_steps(3);
        let q = store.commands(mg_live::now_ms());
        assert_eq!(q["commands"][0]["op"], "teleport");
        assert_eq!(q["commands"][0]["area"], second.to_string());
    }
}
