use super::*;

#[test]
fn gui_writes_use_gateway_operations_and_preserve_concurrency_revisions() {
    let (service, mut receiver) = crate::link::fixture();
    let card = NpcCard::default();
    let gm = GmCard::default();
    let cases = vec![
        (
            service.decide_draft("draft", Some("Edited reply")).unwrap(),
            "draft.decide",
            json!({"id":"draft","text":"Edited reply"}),
        ),
        (
            service.decide_draft("draft", None).unwrap(),
            "draft.decide",
            json!({"id":"draft","text":null}),
        ),
        (service.set_allow_agents(true).unwrap(), "llm.allow", json!({"on":true})),
        (
            service.set_focus(&[("square".into(), "a1".into())]).unwrap(),
            "focus.set",
            json!({"objects":[{"area":"square","id":"a1"}]}),
        ),
        (
            service.save_npc_card("tag:guard", &card, 7).unwrap(),
            "book.put",
            json!({"kind":"npc","key":"tag:guard","value":card,"if_rev":7}),
        ),
        (
            service.save_gm_card("campaign", &gm, 9).unwrap(),
            "book.put",
            json!({"kind":"gm","key":"campaign","value":gm,"if_rev":9}),
        ),
        (
            service.save_object_note("note-key", "Private note", 11).unwrap(),
            "book.put",
            json!({"kind":"note","key":"note-key","value":"Private note","if_rev":11}),
        ),
        (
            service.events_since(23, 50, &["chat".into()]).unwrap(),
            "events.since",
            json!({"n":23,"limit":50,"kinds":["chat"]}),
        ),
        (service.npc_card("a1").unwrap(), "call", json!({"op":"npc_card","input":{"npc":"a1"}})),
        (service.gm_card().unwrap(), "call", json!({"op":"gm_card","input":{}})),
        (
            service.npc_memory("a1", None, 50).unwrap(),
            "call",
            json!({"op":"npc_memory","input":{"npc":"a1","limit":50}}),
        ),
        (
            service.area_notes("square").unwrap(),
            "call",
            json!({"op":"area_notes","input":{"area":"square"}}),
        ),
        (service.ai_list().unwrap(), "ai.list", json!({})),
        (
            service.ai_create(json!({"name":"Helper"})).unwrap(),
            "ai.create",
            json!({"name":"Helper"}),
        ),
        (service.ai_update(json!({"id":"ai:test"})).unwrap(), "ai.update", json!({"id":"ai:test"})),
        (service.ai_revoke("ai:test").unwrap(), "ai.revoke", json!({"id":"ai:test"})),
    ];
    for (ticket, op, args) in cases {
        let request = receiver.try_recv().unwrap();
        assert_eq!(request.op, op);
        assert_eq!(request.a, args);
        assert!(ticket.try_recv().unwrap().is_none());
        request.reply.send(Ok(json!({"rev":12}))).unwrap();
        assert_eq!(ticket.try_recv().unwrap(), Some(json!({"rev":12})));
    }
    assert_eq!(service.allow_agents(), None, "enqueue does not imply server consent");
    assert_eq!(
        service.set_focus(&vec![("square".into(), "a1".into()); 257]).unwrap_err(),
        LinkError::Protocol
    );
    assert_eq!(service.npc_memory("a1", None, 201).unwrap_err(), LinkError::Protocol);
}

#[test]
fn gui_snapshots_use_push_state_and_not_local_assistant_storage() {
    let (service, _) = crate::link::fixture();
    let draft = Draft {
        command: serde_json::from_value(
            json!({"id":"draft","op":"npc_say","area":"square","target":"a1","text":"Hello"}),
        )
        .unwrap(),
        client: "Helper".into(),
        boot: "boot".into(),
        created_ms: 1,
        reply_to: None,
        name: "Guard".into(),
        area: "square".into(),
        notice: None,
    };
    {
        let mut replica = service.shared.replica.lock().unwrap();
        replica.snapshot.drafts = vec![serde_json::to_value(&draft).unwrap()];
        replica.snapshot.drafts_revision = 7;
        replica.snapshot.presence_revision = 8;
        replica.snapshot.actors = vec![json!({"id":"ai:test","name":"Helper"})];
        replica.snapshot.book_revision = 9;
        replica.snapshot.notices.push_back(("info".into(), "Connected".into()));
    }
    assert_eq!(service.drafts().unwrap()[0].text(), draft.text());
    assert_eq!(service.draft_revision(), 7);
    assert_eq!(service.agents_revision(), 8);
    assert_eq!(service.agents()[0]["name"], "Helper");
    assert_eq!(service.assistant_revision(), (9, 0));
    assert_eq!(service.take_notices().len(), 1);
    assert!(service.take_notices().is_empty());
    assert_eq!(service.store().event_cursor(), 0);
    assert_eq!(service.server_id(), "test");
    assert!(service.is_connected());
}

#[test]
fn offline_watch_changes_are_used_by_the_next_hello() {
    let (service, _) = crate::link::fixture();
    *service.shared.watch.lock().unwrap() = vec!["square".into()];
    service.shared.replica.lock().unwrap().snapshot.status = LinkStatus::Reconnecting;
    assert_eq!(service.set_watched_areas(&[]).unwrap_err(), LinkError::Transport);
    assert!(service.shared.watch.lock().unwrap().is_empty());
}
