use super::*;
use serde_json::json;
fn member(id: &str, tag: &str) -> Member {
    Member::from_object(
        "street".into(),
        ObjectKind::Creature,
        &LiveObject { id: id.into(), tag: tag.into(), name: tag.into(), ..Default::default() },
        "boot",
    )
}
fn entry() -> Entry {
    Entry {
        name: "Watch".into(),
        members: vec![member("a", "guard")],
        rows: VecDeque::new(),
        unread: 0,
        watched_from_ms: 0,
    }
}
fn event(channel: i64, target: Option<&str>, data: serde_json::Value) -> Event {
    let mut e = Event {
        n: 1,
        at_ms: 1,
        kind: "chat".into(),
        area: "street".into(),
        id: Some("b".into()),
        name: "Visitor".into(),
        target: target.map(str::to_owned),
        data,
    };
    e.data["channel"] = json!(channel);
    e
}
#[test]
fn nearby_direct_and_private_audiences_are_distinct() {
    let watch = entry();
    assert!(matches(&watch, &event(1, None, json!({"position":{"x":20,"y":0}}))));
    assert!(!matches(&watch, &event(1, None, json!({"position":{"x":20.01,"y":0}}))));
    assert!(matches(&watch, &event(1, None, json!({"position":{"x":0,"y":0}}))));
    assert!(!matches(&watch, &event(1, None, json!({"position":{"x":21,"y":0}}))));
    assert!(!matches(&watch, &event(1, None, json!({}))));
    assert!(matches(&watch, &event(3, Some("a"), json!({"text":"whisper"}))));
    assert!(matches(&watch, &event(20, None, json!({"recipient":"a","text":"DM tell"}))));
    assert!(!matches(
        &watch,
        &event(4, Some("c"), json!({"position":{"x":0,"y":0},"text":"someone else's tell"}))
    ));
    assert!(!matches(
        &watch,
        &event(6, None, json!({"position":{"x":0,"y":0},"text":"private party"}))
    ));
    assert!(!matches(&watch, &event(2, Some("a"), json!({"silent":true,"text":"AI"}))));
}
#[test]
fn moving_after_speech_does_not_reclassify_history_or_duplicate_group_lines() {
    let store = LiveStore::default();
    let mut focus = LiveFocus::default();
    focus.entries.push(entry());
    focus.entries.push(entry());
    focus.entries[1].members.push(member("b", "visitor"));
    let e = event(1, None, json!({"position":{"x":2,"y":0},"text":"Hello"}));
    focus.ingest(&store, vec![e.clone()]);
    focus.entries[0].members[0].position = [900., 900., 0.];
    focus.entries[0].members[0].area = "elsewhere".into();
    focus.ingest(&store, vec![e]);
    assert_eq!(focus.entries[0].rows[0].members, vec!["a"]);
    assert_eq!(focus.entries[0].rows.len(), 1);
    assert_eq!(focus.entries[0].unread, 1);
    assert_eq!(focus.entries[1].rows.len(), 1);
    let mut old = entry();
    old.watched_from_ms = 2;
    assert!(!matches(&old, &event(1, None, json!({"position":{"x":2,"y":0}}))));
}
#[test]
fn matching_reused_ids_and_ambiguous_tags_never_picks_a_different_actor() {
    let store = LiveStore::default();
    let mut focus = LiveFocus::default();
    focus.entries.push(entry());
    let sync = |seq, cre| {
        store.sync(json!({"v":1,"boot":"new","seq":seq,"areas":[{"area":"street","full":true,"cre":cre}]}).to_string().as_bytes(),1).unwrap()
    };
    sync(
        1,
        json!([{"id":"a","tag":"unrelated"},{"id":"b","tag":"guard"},{"id":"c","tag":"guard"}]),
    );
    focus.resolve(&store, "new");
    assert!(!focus.entries[0].members[0].reported);
    let mut reused = event(4, None, json!({"text":"Unrelated actor"}));
    reused.id = Some("a".into());
    focus.ingest(&store, vec![reused]);
    assert!(focus.entries[0].rows.is_empty());
    sync(2, json!([{"id":"a","tag":"unrelated"},{"id":"c","tag":"guard"}]));
    focus.resolve(&store, "new");
    assert_eq!(focus.entries[0].members[0].id, "c");
    assert!(focus.entries[0].members[0].reported);
}
#[test]
fn group_reply_targets_players_only_and_honours_policy() {
    let store = LiveStore::default();
    store.sync(json!({"v":1,"boot":"boot","seq":1,"areas":[{"area":"street","full":true,"cre":[{"id":"a"},{"id":"b","pc":true},{"id":"c","pc":true}]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
    let mut e = entry();
    for id in ["b", "c"] {
        let (a, k, o) = store.locate(id).unwrap();
        e.members.push(Member::from_object(a, k, &o, "boot"));
    }
    assert!(
        window::reply_orders(&store, &e.members, e.members.len() > 1, Default::default(), "Hello")
            .is_err()
    );
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    let commands =
        window::reply_orders(&store, &e.members, e.members.len() > 1, Default::default(), "Hello")
            .unwrap();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].target(), Some("b"));
    assert_eq!(commands[1].target(), Some("c"));
}

#[test]
fn corrupt_watch_file_is_preserved_and_module_paths_are_distinct() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-focus/file-test");
    let module = dir.join("a").join("example.mod");
    let mut writer = LiveFocus { data_dir: Some(dir.clone()), ..Default::default() };
    writer.context(Some(module.clone()));
    let path = writer.path().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"invalid existing JSON").unwrap();
    let mut reader = LiveFocus { data_dir: Some(dir.clone()), ..Default::default() };
    reader.context(Some(module));
    assert!(reader.notice.is_some());
    assert!(reader.save().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"invalid existing JSON");
    reader.context(Some(dir.join("b").join("example.mod")));
    assert_ne!(reader.path().unwrap(), path);
}

#[test]
fn acknowledgements_are_explicit_and_failures_are_not_conversation() {
    let status = json!({"results":[{"command":{"id":"c1","op":"pc_message","target":"a","text":"Hello"}},
        {"command":{"id":"c2","op":"give_item","target":"a","resref":"sword"}}]});
    let mut e = event(1, None, json!({"id":"c1","ok":false}));
    e.kind = "command".into();
    command_echo(&mut e, &status);
    assert_eq!(e.kind, "command");
    e.data["ok"] = json!(true);
    command_echo(&mut e, &status);
    assert_eq!(e.kind, "dm_message");
    assert!(matches(&entry(), &e));
    assert_eq!(e.target.as_deref(), Some("a"));
    e.kind = "command".into();
    e.data["id"] = json!("c2");
    command_echo(&mut e, &status);
    assert_eq!(e.kind, "inventory_command");
    assert!(matches(&entry(), &e));
}

#[test]
fn conversation_defaults_to_speaker_not_recipient_or_neighbour() {
    let members = vec![member("a", "guard"), member("b", "visitor")];
    let mut row = Row {
        event: event(1, Some("a"), json!({"text":"Hello"})),
        members: vec!["a".into(), "b".into()],
        read: false,
        text: String::new(),
        speaker: Some(crate::live_log::Speaker::Npc),
    };
    let own = ConversationFilter::default();
    assert!(own.shows(&row, &members));
    assert!(!own.shows(&row, &members[..1]));
    assert!(ConversationFilter { nearby: true, ..own }.shows(&row, &members[..1]));
    row.event.kind = "dialog".into();
    row.event.data["speaker"] = json!("a");
    assert!(own.shows(&row, &members[..1]));
    row.event.kind = "chat".into();
    row.event.id = None;
    row.speaker = Some(crate::live_log::Speaker::Server);
    assert!(!own.shows(&row, &members));
    assert!(!ConversationFilter { nearby: true, ..own }.shows(&row, &members));
    assert!(ConversationFilter { server: true, ..own }.shows(&row, &members));
    row.event.kind = "dm_message".into();
    row.speaker = Some(crate::live_log::Speaker::Dm);
    assert!(!own.shows(&row, &members));
    assert!(ConversationFilter { server: true, ..own }.shows(&row, &members));
}
