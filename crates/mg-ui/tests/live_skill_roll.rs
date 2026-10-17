//! Interactive skill request UI, fake store only; no server or app launch.
#[path = "support/live_windows.rs"]
mod support;
use egui_kittest::kittest::{NodeT, Queryable};
use mg_ui::Tab;
use serde_json::json;

#[test]
fn skill_roll_window_keeps_waiting_then_shows_roll_and_no_roll() {
    let (app, store, _) = support::app("skill-roll", None);
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    let mut h = support::harness(app, Tab::LiveSkill);
    h.state_mut()
        .area_views
        .get_mut(&support::area())
        .unwrap()
        .live
        .select(Some("9".into()), false);
    h.state_mut().live_skill.single = Some(true);
    h.state_mut().live_skill.bonus = 2;
    h.state_mut().live_skill.show_dc = false;
    h.state_mut().live_skill.timeout = 90;
    h.run_steps(2);
    h.get_by_label("DM bonus");
    h.get_by_label("Roll timeout");
    h.get_by_label("Send skill check").click();
    h.run_steps(2);
    let queued = support::commands(&store);
    let id = &queued[0]["id"];
    assert_eq!(queued[0]["bonus"], 2);
    assert_eq!(queued[0]["show_dc"], false);
    assert_eq!(queued[0]["timeout"], 90);
    let value = json!({"name":"Concentration","skill":1,"dc":15,"bonus":2,"players":[{"id":"9","status":"waiting","nui_token":2}]});
    support::report(&store, 2, vec![json!({"id":id,"ok":true,"pending":true,"value":value})]);
    h.run_steps(3);
    h.get_by_label("Waiting for Roll");
    assert!(h.get_by_label("Send skill check").accesskit_node().is_disabled());
    let mut finished = value;
    finished["players"] = json!([{"id":"9","status":"rolled","roll":14,"modifier":6,"total":22,"passed":true,"nui_token":2},{"id":"a","status":"no_roll"}]);
    support::report(&store, 3, vec![json!({"id":id,"ok":true,"pending":false,"value":finished})]);
    h.run_steps(3);
    h.get_by_label("Concentration: 14 + 6 + 2 (DM) = 22 vs DC 15: Success");
    h.get_by_label("No roll (timed out)");
    assert!(!h.get_by_label("Send skill check").accesskit_node().is_disabled());
    let log = mg_ui::live_skill::log_result(&finished, &|id| id.into()).unwrap();
    assert!(log.contains("14 + 6 + 2 (DM) = 22") && log.contains("No roll"));
    assert!(!h.state().has_unsaved_work());
}
