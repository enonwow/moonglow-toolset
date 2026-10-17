use super::live_accept5::{before, picture};
use super::*;

#[test]
fn live_accept5_factions() {
    let Some((mut h, area)) = area_harness("live-accept5") else { return };
    let mut factions =
        mg_module::factions::Factions::of_module(&h.state().ws.as_ref().unwrap().module);
    factions.add_session_factions(2).unwrap();
    factions.factions[5].name = "test".into();
    h.state_mut()
        .ws
        .as_mut()
        .unwrap()
        .module
        .set_gff(ResKey::parse("repute", ResType::FAC).unwrap(), &factions.to_gff())
        .unwrap();
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Street lamplighter","x":20,"y":20,"faction":5}
        ]}]}),
    );
    store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],
        "factions":factions.factions.iter().enumerate().map(|(id,f)|serde_json::json!({"id":id,"name":f.name})).collect::<Vec<_>>()
    }).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    review_open_menu(&mut h, area, false);
    h.get_by_label(if before() { "Combat ⏵" } else { "Character ⏵" }).click();
    h.run_steps(2);
    h.get_by_label("Faction ⏵").click();
    h.run_steps(2);
    picture(&mut h, "factions");
    if !before() {
        h.get_by_label("5: test");
        h.get_by_label("6: Session 2").click();
        h.run_steps(2);
        let q = store.commands(mg_live::now_ms());
        assert_eq!(q["commands"][0]["op"], "set_faction");
        assert_eq!(q["commands"][0]["faction"], 6);
    }
}
