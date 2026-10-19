use super::*;
use mg_live::{
    gateway::Events,
    store::{
        Command,
        mirror::{MirroredResult, ResultState},
    },
};
use serde_json::json;

fn welcome(epoch: &str, resumed: bool) -> Welcome {
    serde_json::from_value(json!({
        "proto":1,"mirror":2,"gw_epoch":epoch,"now_ms":0,
        "server":{"id":"test","name":"Test"},
        "user":{"id":"dm:test","kind":"dm","name":"Test","avatar":null},
        "groups":["read"],"policy":{},"id_prefix":"test.",
        "limits":{"frame":65536,"book_frame":262144,"in_flight":64,"watch":16},
        "features":[],"nwsync":null,"resumed":resumed
    }))
    .unwrap()
}
fn ready() -> (Replica, LiveStore) {
    let store = LiveStore::new_mirror();
    let mut replica = Replica::default();
    replica.welcome(&store, welcome("epoch", false)).unwrap();
    let meta = serde_json::from_value(json!({"topic":"meta","body":{
        "boot":"boot","rev":1,"factions":[],"watched":{"desired":[],"reported":[]},
        "policy":{},"game":{"connected":true,"age_ms":0},"redis":{}
    }}))
    .unwrap();
    replica.push(&store, meta, 100).unwrap();
    (replica, store)
}

#[test]
fn all_push_topics_update_store_or_auxiliary_snapshot_and_ack() {
    let (mut replica, store) = ready();
    let source = LiveStore::default();
    source.sync(br#"{"v":1,"boot":"boot","areas":[{"area":"square","full":true,"cre":[{"id":"a1","name":"Guard"}]}]}"#, 90).unwrap();
    let push = Push::Area(mg_live::gateway::Area {
        boot: "boot".into(),
        area: "square".into(),
        rev: 7,
        age_ms: 10,
        live: source.area("square").unwrap().into(),
    });
    replica.push(&store, push.clone(), 100).unwrap();
    assert_eq!(store.area("square"), source.area("square"));
    let revision = store.revision();
    replica.push(&store, push, 101).unwrap();
    assert_eq!(store.revision(), revision);
    replica.push(&store, Push::Events(Events { rows: vec![], through: 9, lost: 2 }), 100).unwrap();
    replica
        .push(
            &store,
            Push::Result(MirroredResult {
                id: "test.1234abcd.1".into(),
                rev: 2,
                state: ResultState::Final,
                result: None,
            }),
            100,
        )
        .unwrap();
    assert!(replica.ack(false).is_none());
    let ack = replica.ack(true).unwrap();
    assert_eq!(ack.results, vec![("test.1234abcd.1".into(), 2)]);
    assert_eq!(ack.event_n, 9);
    assert!(replica.ack(true).is_none());
    replica.push(&store, Push::Drafts { rev: 3, items: vec![json!({"id":"draft"})] }, 100).unwrap();
    replica
        .push(
            &store,
            Push::Presence { rev: 4, actors: vec![json!({"name":"DM"})], focus: BTreeMap::new() },
            100,
        )
        .unwrap();
    replica.push(&store, Push::Book { kind: "gm".into(), key: "gm".into(), rev: 1 }, 100).unwrap();
    replica
        .push(
            &store,
            Push::Session {
                groups: vec!["read".into()],
                policy: mg_live::store::Policy { scripts: false, ..Default::default() },
                limits: serde_json::to_value(Limits::default()).unwrap(),
            },
            100,
        )
        .unwrap();
    for _ in 0..130 {
        replica
            .push(&store, Push::Notice { level: "info".into(), text: "Notice".into() }, 100)
            .unwrap();
    }
    assert_eq!(replica.snapshot.drafts_revision, 3);
    assert_eq!(replica.snapshot.presence_revision, 4);
    assert_eq!(replica.snapshot.book_revision, 1);
    assert_eq!(replica.snapshot.notices.len(), 128);
    assert!(!store.policy().scripts);
    replica
        .push(&store, Push::AreaGone { boot: "boot".into(), area: "square".into() }, 100)
        .unwrap();
    assert!(store.area("square").is_none());
    assert!(replica.resume.as_ref().unwrap().areas.is_empty());
    replica.push(&store, Push::Boot { boot: "next".into(), prev: "boot".into() }, 100).unwrap();
    assert_eq!(store.event_cursor(), 0);
    assert!(replica.snapshot.drafts.is_empty());
}

#[test]
fn fresh_epoch_marks_queued_commands_unknown_and_never_resends_them() {
    let (mut replica, store) = ready();
    let command: Command =
        serde_json::from_value(json!({"op":"stop_patrol","area":"square","target":"a1"})).unwrap();
    let id = store.enqueue(command, 100).unwrap();
    assert!(id.starts_with("test."));
    replica.welcome(&store, welcome("epoch", true)).unwrap();
    assert_eq!(store.mirror_commands(100).unwrap().len(), 1);
    assert!(replica.welcome(&store, welcome("other", true)).is_err());
    replica.welcome(&store, welcome("other", false)).unwrap();
    assert!(store.mirror_commands(100).unwrap().is_empty());
    assert_eq!(store.result(&id).unwrap().code, Some(mg_live::store::ErrorCode::Unknown));
}

#[test]
fn hostile_frames_are_bounded_and_secrets_are_not_diagnostics() {
    let secret = "fixture-session-value";
    for bytes in [b"[]".as_slice(), b"{", b"null", br#"{"t":"push"}"#, br#"{"t":"welcome"}"#] {
        assert_eq!(decode(bytes, secret).unwrap_err(), LinkError::Protocol);
    }
    assert_eq!(
        decode(&vec![b' '; frames::SERVER_FRAME_LIMIT + 1], secret).unwrap_err(),
        LinkError::TooLarge
    );
    let unknown = json!({"t":secret});
    assert!(decode(&serde_json::to_vec(&unknown).unwrap(), secret).unwrap().is_none());
    let remote = json!({"t":"res","id":1,"ok":false,"err":{"code":secret,"msg":secret}});
    let frame = decode(&serde_json::to_vec(&remote).unwrap(), secret).unwrap().unwrap();
    assert!(!format!("{frame:?}").contains(secret));
    let notice = json!({"t":"push","topic":"notice","body":{"level":"warn","text":secret}});
    assert!(
        !format!("{:?}", decode(&serde_json::to_vec(&notice).unwrap(), secret)).contains(secret)
    );
}

#[test]
fn acknowledgements_flush_at_32_items_and_keep_the_highest_revision() {
    let (mut replica, store) = ready();
    for rev in 1..=32 {
        replica
            .push(
                &store,
                Push::Result(MirroredResult {
                    id: "test.1234abcd.1".into(),
                    rev,
                    state: ResultState::Pending,
                    result: None,
                }),
                100,
            )
            .unwrap();
        if rev < 32 {
            assert!(replica.ack(false).is_none());
        }
    }
    assert_eq!(replica.ack(false).unwrap().results, vec![("test.1234abcd.1".into(), 32)]);
    assert!(replica.ack(true).is_none());
}

#[test]
fn restart_with_new_prefix_settles_pending_commands_before_reconnecting() {
    let (mut replica, store) = ready();
    let command: Command =
        serde_json::from_value(json!({"op":"stop_patrol","area":"square","target":"a1"})).unwrap();
    let id = store.enqueue(command.clone(), 100).unwrap();
    store.mirror_mark_sent(std::slice::from_ref(&id)).unwrap();
    let mut next = welcome("restarted", false);
    next.id_prefix = "next.".into();
    replica.welcome(&store, next).unwrap();
    assert_eq!(replica.snapshot.status, LinkStatus::Connected);
    assert_eq!(replica.snapshot.connections, 2);
    assert!(store.mirror_commands(101).unwrap().is_empty(), "old command must never be resent");
    assert_eq!(store.result(&id).unwrap().code, Some(mg_live::store::ErrorCode::Unknown));
    let meta = serde_json::from_value(json!({"topic":"meta","body":{
        "boot":"boot","rev":1,"factions":[],"watched":{"desired":[],"reported":[]},
        "policy":{},"game":{"connected":true,"age_ms":0},"redis":{}
    }}))
    .unwrap();
    replica.push(&store, meta, 101).unwrap();
    assert!(store.enqueue(command, 101).unwrap().starts_with("next."));
}

#[test]
fn malformed_restart_welcome_does_not_reset_pending_commands() {
    let (mut replica, store) = ready();
    let command: Command =
        serde_json::from_value(json!({"op":"stop_patrol","area":"square","target":"a1"})).unwrap();
    let id = store.enqueue(command, 100).unwrap();
    let mut next = welcome("restarted", false);
    next.id_prefix = "invalid/prefix".into();
    assert_eq!(replica.welcome(&store, next), Err(LinkError::Protocol));
    assert_eq!(store.mirror_commands(101).unwrap()[0].command.id, id);
    assert_eq!(store.boot(), "boot");
    assert_eq!(replica.snapshot.connections, 1);
}
