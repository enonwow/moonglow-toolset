use mg_live::{
    gateway::{Gateway, Push, Resume, SessionConfig},
    store::LiveStore,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
fn config(id: &str, groups: &[&str]) -> SessionConfig {
    SessionConfig {
        id: id.into(),
        id_prefix: format!("{id}."),
        groups: groups.iter().map(|s| s.to_string()).collect(),
        watch_limit: 16,
    }
}
fn report(s: &LiveStore, seq: u64, events: Value, results: Value) {
    s.sync(json!({"v":1,"boot":"boot","seq":seq,"areas":[{"area":"square","full":true,"cre":[{"id":"a1","name":"Guard","r":"guard"}]}],"events":events,"results":results}).to_string().as_bytes(),seq*1000).unwrap();
}
fn setup() -> (Arc<LiveStore>, Gateway) {
    let s = Arc::new(LiveStore::default());
    report(&s, 1, json!([]), json!([]));
    let g = Gateway::new(s.clone()).unwrap();
    g.connect(config("one", &["read", "npc"]), None, &["square".into()], 1000).unwrap();
    (s, g)
}
fn command(n: u64) -> Value {
    json!({"id":format!("one.1234abcd.{n}"),"boot":"boot","ttl_ms":30000,"op":"stop_patrol","area":"square","target":"a1"})
}
#[test]
fn changed_areas_coalesce_and_resume_preserves_cursors() {
    let (s, g) = setup();
    let first = g.pushes("one", 1000).unwrap();
    assert!(matches!(first[0], Push::Meta(_)));
    let area =
        first.iter().find_map(|p| if let Push::Area(a) = p { Some(a) } else { None }).unwrap();
    let resume = Resume {
        gw_epoch: g.epoch().into(),
        boot: "boot".into(),
        event_n: s.event_cursor(),
        areas: BTreeMap::from([("square".into(), area.rev)]),
    };
    report(&s, 2, json!([]), json!([]));
    assert!(!g.pushes("one", 1100).unwrap().iter().any(|p| matches!(p, Push::Area(_))));
    assert!(
        g.connect(config("one", &["read", "npc"]), Some(&resume), &["square".into()], 2100)
            .unwrap()
    );
    assert_eq!(
        g.pushes("one", 2100).unwrap().iter().filter(|p| matches!(p, Push::Area(_))).count(),
        1
    );
    g.watch_set("one", &[], 2200).unwrap();
    assert!(g.pushes("one", 2400).unwrap().iter().any(|p| matches!(p, Push::AreaGone { .. })));
}
#[test]
fn event_permissions_advance_through_hidden_rows() {
    let (s, g) = setup();
    g.connect(config("chat", &["read", "read_chat"]), None, &[], 1000).unwrap();
    g.connect(config("tell", &["read", "read_chat", "read_tells"]), None, &[], 1000).unwrap();
    report(
        &s,
        2,
        json!([{"seq":1,"kind":"chat","area":"square","channel":1,"text":"public"},{"seq":2,"kind":"chat","area":"square","channel":4,"text":"private"},{"seq":3,"kind":"chat","area":"square","channel":20,"text":"private"}]),
        json!([]),
    );
    for (id, count) in [("one", 0), ("chat", 1), ("tell", 3)] {
        let pushes = g.pushes(id, 2000).unwrap();
        let e = pushes
            .iter()
            .find_map(|p| if let Push::Events(e) = p { Some(e) } else { None })
            .unwrap();
        assert_eq!(e.rows.iter().filter(|e| e.kind == "chat").count(), count);
        assert_eq!(e.through, s.event_cursor());
    }
}
#[test]
fn command_validation_idempotency_and_versioned_results() {
    let (s, g) = setup();
    let c = command(1);
    assert_eq!(g.commands("one", &[c.clone(), c.clone()], 1000).unwrap().accepted.len(), 2);
    let mut changed = c.clone();
    changed["target"] = json!("a2");
    assert_eq!(g.commands("one", &[changed], 1000).unwrap().rejected[0].code, "conflict");
    for (key, value, code) in [
        ("boot", json!("old"), "expired"),
        ("id", json!("other.1234abcd.1"), "invalid"),
        ("ttl_ms", json!(0), "invalid"),
        ("op", json!("resync"), "forbidden"),
        ("op", json!("watch_areas"), "forbidden"),
    ] {
        let mut bad = command(2);
        bad[key] = value;
        assert_eq!(g.commands("one", &[bad], 1000).unwrap().rejected[0].code, code);
    }
    let initial = g.pushes("one", 1000).unwrap();
    assert!(initial.iter().any(|p| matches!(p,Push::Result(r) if r.rev==1)));
    report(&s, 2, json!([]), json!([{"id":c["id"],"ok":true}]));
    let pushes = g.pushes("one", 2000).unwrap();
    let r =
        pushes.iter().find_map(|p| if let Push::Result(r) = p { Some(r) } else { None }).unwrap();
    assert_eq!(r.rev, 2);
    g.ack("one", &[(r.id.clone(), 1)], s.event_cursor()).unwrap();
    let resume = Resume {
        gw_epoch: g.epoch().into(),
        boot: s.boot(),
        event_n: s.event_cursor(),
        areas: BTreeMap::new(),
    };
    assert!(g.connect(config("one", &["read", "npc"]), Some(&resume), &[], 2000).unwrap());
    assert!(g.pushes("one", 2000).unwrap().iter().any(|p| matches!(p, Push::Result(_))));
    g.ack("one", &[(r.id.clone(), 2)], s.event_cursor()).unwrap();
    assert!(g.connect(config("one", &["read", "npc"]), Some(&resume), &[], 2000).unwrap());
    assert!(!g.pushes("one", 2000).unwrap().iter().any(|p| matches!(p, Push::Result(_))));
}
#[test]
fn outbox_retains_more_than_store_ring_and_limits_commands() {
    let (s, g) = setup();
    for n in 1..=256 {
        let now = 1000 + ((n - 1) / 60) * 60_000;
        let reply = g.commands("one", &[command(n)], now).unwrap();
        assert!(reply.rejected.is_empty(), "{reply:?}");
        report(&s, n + 1, json!([]), json!([{"id":format!("one.1234abcd.{n}"),"ok":true}]));
    }
    assert_eq!(
        g.commands("one", &[command(257)], 300_000).unwrap().rejected[0].code,
        "rate_limited"
    );
    assert_eq!(
        g.pushes("one", 300_000).unwrap().iter().filter(|p| matches!(p, Push::Result(_))).count(),
        256
    );
    assert_eq!(s.status(300_000)["results"].as_array().unwrap().len(), 64);
    assert!(s.result("one.1234abcd.1").unwrap().ok);
    g.pushes("one", 601_000).unwrap();
    assert!(g.commands("one", &[command(257)], 601_001).unwrap().rejected.is_empty());
}
#[test]
fn shorter_ttl_and_watch_union_are_enforced() {
    let (s, g) = setup();
    let mut c = command(1);
    c["ttl_ms"] = json!(10);
    g.commands("one", &[c], 1000).unwrap();
    g.tick(1010).unwrap();
    g.pushes("one", 1010).unwrap();
    assert_eq!(s.result("one.1234abcd.1").unwrap().code, Some(mg_live::store::ErrorCode::Expired));
    for i in 0..5 {
        let id = format!("s{i}");
        g.connect(
            config(&id, &["read"]),
            None,
            &(0..16).map(|j| format!("a{i}_{j}")).collect::<Vec<_>>(),
            1000,
        )
        .unwrap();
    }
    let response =
        g.watch_set("s4", &(0..16).map(|j| format!("a4_{j}")).collect::<Vec<_>>(), 1000).unwrap();
    assert_eq!(response.waiting.len(), 16);
}

#[test]
fn sessions_share_one_serialized_copy_of_an_area() {
    use mg_live::gateway_frames::{ServerFrame, encode_server};
    let (_s, g) = setup();
    g.connect(config("two", &["read"]), None, &["square".into()], 1000).unwrap();
    let area = |id: &str| {
        g.pushes(id, 1000).unwrap().into_iter().find_map(|p| match p {
            Push::Area(a) => Some(a),
            _ => None,
        })
    };
    let (a, b) = (area("one").unwrap(), area("two").unwrap());
    assert!(std::ptr::eq(&*a.live, &*b.live), "one deep copy per revision");
    for frame in [ServerFrame::Push(Push::Area(a)), ServerFrame::Push(Push::Area(b))] {
        let text = encode_server(&frame).unwrap();
        assert_eq!(text, serde_json::to_string(&frame).unwrap());
        assert!(!text.contains("received_ms"));
        // received_ms is local to each process and never on the wire.
        let back = serde_json::from_str::<ServerFrame>(&text).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), text);
    }
}
#[test]
fn meta_is_pushed_for_state_changes_not_for_the_ageing_of_the_last_report() {
    let (s, g) = setup();
    let metas =
        |now| g.pushes("one", now).unwrap().iter().filter(|p| matches!(p, Push::Meta(_))).count();
    assert_eq!(metas(1000), 1);
    // The pump polls every 50 ms; the report age moves by that much each time.
    for now in (1050..2000).step_by(50) {
        assert_eq!(metas(now), 0, "age alone at {now}");
    }
    assert_eq!(metas(2100), 1, "a second without a report is news");
    report(&s, 3, json!([]), json!([]));
    assert_eq!(metas(3000), 1, "a report changes the game status");
    s.set_policy(mg_live::store::Policy { chunks: true, ..Default::default() });
    assert_eq!(metas(3050), 1, "so does the policy");
    assert_eq!(metas(3100), 0);
}
