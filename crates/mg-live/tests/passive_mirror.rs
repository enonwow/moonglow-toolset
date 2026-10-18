use mg_live::{
    events::{Event, Query},
    store::{
        AreaLive, Command, CommandResult, ErrorCode, LiveObject, LiveStore, Policy,
        mirror::{GameStatus, ResultState, Watched},
    },
};
use serde_json::json;
fn mirror() -> LiveStore {
    let s = LiveStore::new_mirror();
    s.set_id_prefix("test.").unwrap();
    s.mirror_meta(
        "boot".into(),
        None,
        Watched::default(),
        Policy::default(),
        GameStatus { connected: true, age_ms: 0 },
    )
    .unwrap();
    s
}
fn command() -> Command {
    serde_json::from_value(json!({"op":"set_variable","area":"square","target":"a1","name":"value","type":"int","value":1})).unwrap()
}
fn event(n: u64) -> Event {
    serde_json::from_value(json!({"n":n,"at_ms":20,"kind":"attack","area":"square","id":"a1","name":"Guard","target":null,"data":{}})).unwrap()
}
#[test]
fn mirror_is_passive_and_revisions_include_every_channel() {
    let s = mirror();
    let initial = s.revision();
    assert!(s.sync(br#"{"v":1,"boot":"x","seq":1,"areas":[]}"#, 1).is_err());
    let area = AreaLive {
        has_full: true,
        full_ms: 7,
        seq: 50,
        creatures: vec![LiveObject { id: "a1".into(), ..Default::default() }],
        ..Default::default()
    };
    s.mirror_area("SQUARE", area.clone(), 25).unwrap();
    assert_ne!(s.revision(), initial);
    let mut expected = area;
    expected.received_ms = 25;
    assert_eq!(s.area("square"), Some(expected));
    assert!(s.events(&Query::default()).0.is_empty());
    s.server_log(&json!({"stream":"s","seq":1,"line":"ignored"}), 30);
    s.mirror_events(vec![event(4), event(9)], 12).unwrap();
    s.mirror_events(vec![event(9)], 12).unwrap();
    assert_eq!(s.events(&Query::default()), (vec![event(4), event(9)], 12));
    assert!(s.mirror_events(vec![event(14), event(13)], 14).is_err());
    assert_eq!(s.event_cursor(), 12);
    s.mirror_area_gone("square").unwrap();
    assert!(s.area("square").is_none());
    assert!(LiveStore::default().mirror_events(vec![], 0).is_err());
}
#[test]
fn only_unsent_commands_expire_and_reset_never_resends() {
    let s = mirror();
    let (tx, rx) = std::sync::mpsc::channel();
    s.set_kick(tx);
    let a = s.enqueue(command(), 10).unwrap();
    let b = s.enqueue(command(), 10).unwrap();
    let pieces: Vec<_> = a.split('.').collect();
    assert_eq!(pieces[0], "test");
    assert_eq!(pieces[1].len(), 8);
    assert!(pieces[1].bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(a, b);
    assert!(rx.try_recv().is_ok());
    s.mirror_set_ttl(&a, 20).unwrap();
    s.mirror_mark_sent(std::slice::from_ref(&b)).unwrap();
    s.expire(100_000);
    assert_eq!(s.result(&a).unwrap().code, Some(ErrorCode::Expired));
    assert!(s.result(&b).is_none());
    assert_eq!(s.mirror_commands(100_000).unwrap().len(), 1);
    s.mirror_reset("new epoch").unwrap();
    assert_eq!(s.result(&b).unwrap().code, Some(ErrorCode::Unknown));
    assert!(s.mirror_commands(100_000).unwrap().is_empty());
    assert_eq!(s.policy(), Policy::default());
    s.mirror_meta("new".into(), None, Watched::default(), Policy::default(), GameStatus::default())
        .unwrap();
    s.enqueue(command(), 100_001).unwrap();
    assert!(rx.try_recv().is_ok());
}
#[test]
fn gateway_results_do_not_derive_events_and_final_is_immutable() {
    let s = mirror();
    let id = s.enqueue(command(), 10).unwrap();
    s.mirror_result(&id, 1, ResultState::Pending, None).unwrap();
    let result = CommandResult { id: id.clone(), ok: true, ..Default::default() };
    s.mirror_result(&id, 2, ResultState::Final, Some(result.clone())).unwrap();
    s.mirror_result(&id, 3, ResultState::Unknown, None).unwrap();
    assert_eq!(s.result(&id), Some(result));
    assert_eq!(s.event_cursor(), 0);
    let id = s.enqueue(command(), 10).unwrap();
    s.mirror_reject(&id, ErrorCode::Forbidden, "denied").unwrap();
    assert_eq!(s.result(&id).unwrap().code, Some(ErrorCode::Forbidden));
}
