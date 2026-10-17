//! The session pane uses fake live reports; edits remain isolated from repute.fac.
use crate::live_windows as support;
use egui_kittest::kittest::{NodeT, Queryable};
use mg_ui::Tab;
use serde_json::json;
#[test]
fn session_factions_offline_reserve_and_native_queue() {
    let (app, store, _) = support::app("session-factions", None);
    let mut h = support::harness(app, Tab::Factions);
    h.state_mut().faction_view.session_spares = 2;
    h.get_by_label("Add session factions").click();
    h.run_steps(3);
    let factions = |h: &egui_kittest::Harness<'_, mg_ui::Moonglow>| {
        mg_module::factions::Factions::of_module(&h.state().ws.as_ref().unwrap().module)
    };
    assert_eq!(factions(&h).factions.len(), 7);
    assert_eq!(factions(&h).factions[5].name, "Session 1");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    assert_eq!(factions(&h).factions.len(), 5);
    h.get_by_label("Add session factions").click();
    h.run_steps(3);
    let before = factions(&h).to_gff();
    h.get_by_label("Live session").click();
    h.run_steps(3);
    h.state_mut().faction_view.live.faction = 5;
    h.run_steps(2);
    h.get_by_label_contains("Session 1: not on the server");
    assert!(h.get_by_label("Assign faction to target").accesskit_node().is_disabled());
    assert!(support::commands(&store).is_empty());
    store.sync(json!({"v":1,"boot":"windows","seq":2,"factions":[{"id":0,"name":"PC"},{"id":1,"name":"Hostile"},{"id":5,"name":"Session 1"}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    h.run_steps(2);
    h.state_mut().faction_view.live.label = "Dock ambushers".into();
    h.get_by_label("Save label").click();
    h.run_steps(3);
    assert_eq!(h.state().faction_view.live.label_for(5, "Session 1"), "Dock ambushers (Session 1)");
    assert_eq!(factions(&h).to_gff(), before, "session labels never dirty repute.fac");
    h.state_mut().faction_view.live.reputation = 10;
    h.get_by_label("Set live reputation").click();
    h.run_steps(2);
    let queued = support::commands(&store);
    let command = &queued[0];
    assert_eq!(command["op"], "set_faction_reputation");
    assert_eq!(command["faction"], 5);
    assert_eq!(command["target"], "7");
    assert_eq!(command["reputation"], 10);
    support::report(
        &store,
        3,
        vec![json!({"id":command["id"],"ok":true,"value":{"before":50,"reputation":10}})],
    );
    h.run_steps(3);
    assert!(h.state().faction_view.live.notice.as_ref().unwrap().contains("10"));
    h.get_by_label("Read live reputation").click();
    h.run_steps(2);
    let q = support::commands(&store);
    assert_eq!(q[0]["op"], "get_faction_reputation");
    support::report(
        &store,
        4,
        vec![
            json!({"id":q[0]["id"],"ok":false,"code":"disabled","err":"Native mismatch","value":{"reputation":90}}),
        ],
    );
    h.run_steps(3);
    assert!(h.state().faction_view.live.notice.as_ref().unwrap().contains("Native mismatch"));
    assert!(h.state().faction_view.live.notice.as_ref().unwrap().contains("90"));
    h.get_by_label("Assign faction to target").click();
    h.run_steps(2);
    assert_eq!(support::commands(&store)[0]["op"], "set_faction");
    store
        .sync(
            json!({"v":1,"boot":"restarted","seq":1,"areas":[]}).to_string().as_bytes(),
            mg_live::now_ms(),
        )
        .unwrap();
    h.run_steps(3);
    assert_eq!(h.state().faction_view.live.label_for(5, "Session 1"), "Session 1");
    assert_eq!(factions(&h).to_gff(), before);
}
