use super::*;

fn picture(h: &mut Harness<'_, Moonglow>, name: &str) {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-portrait");
    std::fs::create_dir_all(&out).unwrap();
    let phase = if before() { "before" } else { "after" };
    h.render().unwrap().save(out.join(format!("{name}-{phase}.png"))).unwrap();
}

fn before() -> bool {
    std::env::var("MGLIVE_PORTRAIT_PHASE").as_deref() == Ok("before")
}

#[test]
fn live_portrait_demo_report() {
    let Some((mut h, area)) = area_harness("live-portrait") else { return };
    // A read-only live_area_state capture can replace this authored equivalent.
    // Keep the capture out of Git, as with installed game resources.
    let mut creatures = if let Ok(path) = std::env::var("MGLIVE_PORTRAIT_REPORT") {
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        report["creatures"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| {
                ["02 - Night torch guard", "07 - Group patrol", "12 - Street lamplighter"]
                    .contains(&c["name"].as_str().unwrap_or_default())
            })
            .cloned()
            .collect::<Vec<_>>()
    } else {
        ["02 - Night torch guard", "07 - Group patrol", "12 - Street lamplighter"]
            .into_iter().enumerate().map(|(i, name)| serde_json::json!({
                "id": format!("a{i}"), "tag":format!("portrait_actor_{i}"),
                "name":name,"app":6,"portrait":"po_","hp":[30,30],"look":{"Gender":{"type":"byte","value":0}}
            })).collect()
    };
    assert_eq!(creatures.len(), 3);
    for (i, c) in creatures.iter_mut().enumerate() {
        assert_eq!(c["app"], 6);
        assert_eq!(c["portrait"], "po_");
        // Only relocate into the small scratch test area; identity and looks stay intact.
        c["x"] = (20 + i * 4).into();
        c["y"] = 20.into();
    }
    let ids = creatures.iter().map(|c| c["id"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    let store = review_live_start(&mut h, area, serde_json::json!({"areas":[{"cre":creatures}]}));
    h.state_mut().palette.area = Some(area);
    let module = h.state().module_path();
    h.state_mut().live_focus.context(module);
    h.state_mut().live_focus.entries.clear();
    h.state_mut().live_focus.add(&store, &ids).unwrap();
    for id in &ids {
        h.state_mut().area_views.get_mut(&area).unwrap().live.select(Some(id.clone()), true);
    }
    let events = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            serde_json::json!({
                "seq":i+1,"kind":"chat","area":area.to_string(),"id":id,
                "text":"The street is quiet tonight.","channel":1
            })
        })
        .collect::<Vec<_>>();
    store
        .sync(
            serde_json::json!({"v":1,"boot":"accept2","seq":2,"areas":[],"events":events})
                .to_string()
                .as_bytes(),
            mg_live::now_ms(),
        )
        .unwrap();
    for (tab, name) in
        [(Tab::LiveChat, "chat"), (Tab::LiveFocus, "focus"), (Tab::LiveSelection, "selection")]
    {
        h.state_mut().actions.push(mg_ui::Action::OpenTab(tab));
        h.run_steps(4);
        if name == "focus" {
            h.get_by_label("Members").click();
            h.run_steps(2);
        }
        picture(&mut h, name);
        for c in &creatures {
            assert!(
                h.query_all_by_label(&format!("Portrait of {}", c["name"].as_str().unwrap()))
                    .next()
                    .is_some()
            );
        }
    }
    assert!(store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty());
    assert!(!h.state().has_unsaved_work());
}
