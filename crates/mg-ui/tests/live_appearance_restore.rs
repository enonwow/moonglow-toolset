//! First-report restore is live-only and requires target confirmation.
#[path = "support/live_windows.rs"]
mod support;
use egui_kittest::kittest::Queryable;
use mg_ui::Tab;
use serde_json::json;

#[test]
fn live_appearance_restore_creature_and_placeable_use_their_own_baseline() {
    for (kind, original) in [("cre", 0), ("plc", 4)] {
        let (app, store, _) = support::app(&format!("appearance-restore-{kind}"), None);
        let mut report = json!({"v":1,"boot":"windows","seq":2,"areas":[{"area":"start","full":true,"cre":[],"plc":[]}]});
        report["areas"][0][kind] = json!([{"id":"7","name":"Changed target","app":1,"app_original":original,"x":3,"y":5,"z":0}]);
        store.sync(report.to_string().as_bytes(), mg_live::now_ms()).unwrap();
        let mut h = support::harness(app, Tab::PlaceableGallery);
        if kind == "cre" {
            h.get_by_label("Creatures").click();
            h.run_steps(2);
        }
        h.get_by_label("Restore original appearance").click();
        h.run_steps(2);
        assert!(support::commands(&store).is_empty());
        h.get_by_label("Changed target (7)");
        h.get_by_label("Confirm appearance").click();
        h.run_steps(2);
        let commands = support::commands(&store);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["op"], "set_appearance");
        assert_eq!(commands[0]["row"], original);
        assert!(!h.state().has_unsaved_work());
    }
}

#[test]
fn live_appearance_restore_is_unavailable_without_a_reported_baseline() {
    use egui_kittest::kittest::NodeT;
    let (app, store, _) = support::app("appearance-restore-legacy", None);
    let mut h = support::harness(app, Tab::PlaceableGallery);
    h.get_by_label("Creatures").click();
    h.run_steps(2);
    assert!(h.get_by_label("Restore original appearance").accesskit_node().is_disabled());
    assert!(support::commands(&store).is_empty());
}
