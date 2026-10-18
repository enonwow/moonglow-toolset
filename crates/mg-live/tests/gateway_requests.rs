use mg_live::{
    gateway::{Gateway, SessionConfig},
    gateway_frames::Request,
    store::LiveStore,
};
use serde_json::{Value, json};
use std::sync::Arc;
fn session(g: &Gateway, id: &str, groups: &[&str]) {
    g.connect(
        SessionConfig {
            id: id.into(),
            id_prefix: format!("{id}."),
            groups: groups.iter().map(|v| (*v).into()).collect(),
            watch_limit: 16,
        },
        None,
        &[],
        1000,
    )
    .unwrap();
}
fn request(
    g: &Gateway,
    who: &str,
    op: &str,
    a: Value,
) -> Result<Value, mg_live::gateway_frames::FrameError> {
    g.handle(who, Request { id: 1, op: op.into(), a }, 1000).result
}
#[test]
fn notes_and_event_pages_require_read_and_preserve_filtered_cursor() {
    let s = Arc::new(LiveStore::default());
    s.sync(
        json!({"v":1,"boot":"boot","seq":1,"areas":[],"events":[
            {"seq":1,"kind":"chat","channel":1,"text":"one"},
            {"seq":2,"kind":"chat","channel":4,"text":"private"},
            {"seq":3,"kind":"chat","channel":1,"text":"two"}
        ]})
        .to_string()
        .as_bytes(),
        1000,
    )
    .unwrap();
    let g = Gateway::new(s.clone()).unwrap();
    session(&g, "dm:reader", &["read", "read_chat"]);
    session(&g, "dm:none", &[]);
    assert_eq!(
        request(&g, "dm:reader", "call", json!({"op":"area_notes","input":{"area":"square"}}))
            .unwrap(),
        json!({"notes":{}})
    );
    assert_eq!(
        request(&g, "dm:none", "call", json!({"op":"area_notes","input":{"area":"square"}}))
            .unwrap_err()
            .code,
        "forbidden"
    );
    assert_eq!(
        request(&g, "dm:none", "events.since", json!({"n":0})).unwrap_err().code,
        "forbidden"
    );
    let first = request(&g, "dm:reader", "events.since", json!({"n":0,"limit":1,"kinds":["chat"]}))
        .unwrap();
    assert_eq!(first["rows"].as_array().unwrap().len(), 1);
    let second = request(
        &g,
        "dm:reader",
        "events.since",
        json!({"n":first["through"],"limit":1,"kinds":["chat"]}),
    )
    .unwrap();
    assert_eq!(second["rows"].as_array().unwrap().len(), 1);
    assert_ne!(first["rows"][0]["n"], second["rows"][0]["n"]);
    assert_eq!(second["through"], s.event_cursor());
}

#[test]
fn presence_tracks_focus_disconnect_and_reconnect_with_revisions() {
    use mg_live::gateway::Push;
    let g = Gateway::new(Arc::new(LiveStore::default())).unwrap();
    session(&g, "dm:one", &["read", "focus"]);
    session(&g, "dm:two", &["read"]);
    let presence = |who: &str| {
        g.pushes(who, 1000)
            .unwrap()
            .into_iter()
            .find_map(|p| match p {
                Push::Presence { rev, actors, focus } => Some((rev, actors, focus)),
                _ => None,
            })
            .expect("presence snapshot")
    };
    let (rev, actors, _) = presence("dm:two");
    assert_eq!(actors.len(), 2);
    g.focus_set("dm:one", vec![("square".into(), "a1".into())]).unwrap();
    let (focused, actors, focus) = presence("dm:two");
    assert!(focused > rev);
    assert_eq!(focus["a1"], vec!["dm:one"]);
    assert!(actors.iter().any(|a| a["id"] == "dm:one" && a["area"] == "square"));
    g.disconnect("dm:one", 1001).unwrap();
    let (gone, actors, focus) = presence("dm:two");
    assert!(gone > focused);
    assert_eq!(actors.len(), 1);
    assert!(focus.is_empty());
    session(&g, "dm:one", &["read", "focus"]);
    let (back, actors, focus) = presence("dm:two");
    assert!(back > gone);
    assert_eq!(actors.len(), 2);
    assert!(focus.is_empty());
}
