use super::live_accept5::{before, picture};
use super::*;

#[test]
fn live_accept5_scene() {
    let Some((mut h, area)) = area_harness("live-accept5") else { return };
    let store = review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Street lamplighter","r":"nw_bartender","x":20,"y":20,"app":6}
        ]}]}),
    );
    h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some("a".into()), false);
    h.state_mut().palette.area = Some(area);
    h.state_mut().live_scene.data_dir = Some(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/live-accept5/scenes"),
    );
    h.state_mut().actions.push(mg_ui::Action::OpenTab(Tab::Scene));
    h.run_steps(4);
    if !before() {
        h.get_by_label("Try it: make an example scene with the selected NPC").click();
        h.run_steps(3);
        h.get_by_label("1. Street lamplighter says ‘Hello’");
        h.get_by_label("2. Street lamplighter walks to 25.0, 20.0");
    }
    picture(&mut h, "scene-example");
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
    if !before() {
        h.get_by_label("Play scene").click();
        h.run_steps(3);
        let queued = |op: &str| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
            loop {
                let q = store.commands(mg_live::now_ms());
                if let Some(command) =
                    q["commands"].as_array().unwrap().iter().find(|q| q["op"] == op)
                {
                    return command.clone();
                }
                assert!(std::time::Instant::now() < deadline, "scene did not queue {op}");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        let say = queued("npc_say");
        assert_eq!(say["target"], "a");
        assert_eq!(say["text"], "Hello");
        store
            .sync(
                serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],
            "results":[{"id":say["id"],"ok":true}]})
                .to_string()
                .as_bytes(),
                mg_live::now_ms(),
            )
            .unwrap();
        let walk = queued("npc_move");
        assert_eq!(walk["target"], "a");
        assert_eq!(walk["x"], 25.);
        assert_eq!(walk["y"], 20.);
        store.sync(serde_json::json!({"v":1,"boot":"accept2","seq":3,"areas":[
            {"area":area.to_string(),"cre":[{"id":"a","name":"Street lamplighter","r":"nw_bartender","x":25,"y":20,"app":6}]}],
            "results":[{"id":walk["id"],"ok":true}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let status = h.state_mut().live_scene.player.status();
            if status["playing"].is_null() {
                assert_eq!(status["last"]["ok"], true, "{status}");
                assert_eq!(status["last"]["steps"].as_array().unwrap().len(), 2);
                break;
            }
            assert!(std::time::Instant::now() < deadline, "scene did not finish: {status}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
