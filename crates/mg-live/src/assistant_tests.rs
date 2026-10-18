use super::*;
use crate::access::{ClientGroup, ClientToken};

fn access(delivery: DeliveryMode, groups: Vec<ClientGroup>) -> Arc<Access> {
    Arc::new(
        Access::new(
            "d".repeat(32),
            &[ClientToken {
                name: "Storyteller".into(),
                token: "s".repeat(32),
                groups,
                per_minute: 60,
                delivery,
            }],
        )
        .unwrap(),
    )
}
fn fixture() -> (LiveStore, Arc<Access>) {
    let store = LiveStore::default();
    store.sync(br#"{"v":1,"boot":"one","seq":1,"areas":[{"area":"room","cre":[{"id":"1","tag":"guard","name":"Guard"},{"id":"2","pc":true,"name":"Adventurer"}]}]}"#,100).unwrap();
    let a = access(
        DeliveryMode::Drafts,
        vec![ClientGroup::Read, ClientGroup::ReadChat, ClientGroup::Npc],
    );
    store.set_assistant_access(a.clone());
    (store, a)
}
fn say(id: &str) -> Command {
    serde_json::from_value(
        json!({"id":id,"op":"npc_say","area":"room","target":"1","text":"Hello"}),
    )
    .unwrap()
}
#[test]
fn speech_drafts_are_not_published_and_rejections_are_scoped_results() {
    let (store, access) = fixture();
    let scope = access.scope_named("Storyteller").unwrap();
    let id = store.enqueue_scoped(say("proposal"), 200, &scope).unwrap();
    assert_eq!(store.commands(201)["commands"], json!([]));
    assert_eq!(store.enqueue_scoped(say(&id), 202, &scope).unwrap(), id);
    assert_eq!(store.drafts().len(), 1);
    let pending = store.result(&id).unwrap();
    assert!(pending.pending);
    assert_eq!(pending.value.unwrap()["draft"]["outcome"], "pending");
    store.expire(100_000);
    assert!(store.result(&id).unwrap().pending, "DM review is not the native 30-second queue TTL");
    store.decide_draft(&id, None, 100_001).unwrap();
    let rejected = store.result(&id).unwrap();
    assert!(!rejected.ok && !rejected.pending);
    assert_eq!(rejected.value.unwrap()["draft"]["outcome"], "rejected");
    assert_eq!(store.commands(100_002)["commands"], json!([]));
    assert!(store.decide_draft(&id, Some("Late duplicate"), 100_003).is_err());
    let events = store.events(&crate::events::Query::default()).0;
    assert!(
        events
            .iter()
            .any(|e| e.data["id"] == id && e.data["value"]["draft"]["outcome"] == "rejected")
    );
}
#[test]
fn approval_after_history_rotation_uses_timed_id_and_preserves_draft_link() {
    let (store, access) = fixture();
    let scope = access.scope_named("Storyteller").unwrap();
    store.enqueue_scoped(say("old-proposal"), 200, &scope).unwrap();
    store.decide_draft("old-proposal", None, 201).unwrap();
    let now = crate::command_history::HORIZON_MS + 1000;
    store.expire(now);
    store
        .sync(br#"{"v":1,"boot":"one","seq":2,"areas":[{"area":"room","cre":[{"id":"1"}]}]}"#, now)
        .unwrap();
    let id = format!("proposal.{}", crate::command_counter(now, 0).unwrap());
    store.enqueue_scoped(say(&id), now, &scope).unwrap();
    assert!(store.lock().history.boundary.legacy_closed);
    let native = store.decide_draft(&id, Some("Approved"), now + 1).unwrap();
    assert_eq!(crate::command_issued_ms(&native), Some(now + 1));
    store
        .sync(
            json!({"v":1,"boot":"one","seq":3,"results":[{"id":native,"ok":true}]})
                .to_string()
                .as_bytes(),
            now + 2,
        )
        .unwrap();
    let result = store.result(&id).unwrap();
    assert!(result.ok && !result.pending);
    assert_eq!(
        store.enqueue_scoped(say("old-proposal"), now + 3, &scope).unwrap_err().code,
        ErrorCode::Expired
    );
}

#[test]
fn approval_edits_once_and_retains_the_agent_result_identity() {
    let (store, access) = fixture();
    let scope = access.scope_named("Storyteller").unwrap();
    let original = store.enqueue_scoped(say("proposal"), 200, &scope).unwrap();
    let native = store.decide_draft(&original, Some("Welcome, traveller."), 201).unwrap();
    assert_ne!(original, native);
    let sent = store.commands(202);
    assert_eq!(sent["commands"].as_array().unwrap().len(), 1);
    assert_eq!(sent["commands"][0]["text"], "Welcome, traveller.");
    assert!(store.drafts().is_empty());
    assert!(store.decide_draft(&original, Some("Twice"), 203).is_err());
    store
        .sync(
            json!({"v":1,"boot":"one","seq":2,"results":[{"id":native,"ok":true}],"areas":[]})
                .to_string()
                .as_bytes(),
            204,
        )
        .unwrap();
    let result = store.result(&original).unwrap();
    assert!(result.ok && !result.pending);
    assert_eq!(result.value.as_ref().unwrap()["draft"]["outcome"], "edited");
    assert_eq!(result.value.unwrap()["draft"]["text"], "Welcome, traveller.");
    assert_eq!(store.enqueue_scoped(say(&original), 205, &scope).unwrap(), original);
    assert_eq!(store.commands(206)["commands"], json!([]));
}
#[test]
fn approval_revalidates_permissions_target_and_boot() {
    let (store, access) = fixture();
    let scope = access.scope_named("Storyteller").unwrap();
    store.enqueue_scoped(say("proposal"), 200, &scope).unwrap();
    store.set_assistant_access(super::tests::access(DeliveryMode::Drafts, vec![ClientGroup::Read]));
    assert!(store.decide_draft("proposal", Some("Denied"), 201).is_err());
    store.set_assistant_access(access);
    store.sync(br#"{"v":1,"boot":"one","seq":2,"areas":[{"area":"room","cre":[{"id":"1","dead":true}]}]}"#,202).unwrap();
    assert!(store.decide_draft("proposal", Some("Dead target"), 203).is_err());
    assert_eq!(store.drafts().len(), 1);
    assert_eq!(store.commands(204)["commands"], json!([]));
    store.sync(br#"{"v":1,"boot":"two","seq":1,"areas":[]}"#, 205).unwrap();
    assert!(store.drafts().is_empty());
    assert_eq!(store.result("proposal").unwrap().value.unwrap()["draft"]["outcome"], "expired");
}
#[test]
fn auto_and_local_speech_still_go_straight_to_the_queue() {
    let (store, _) = fixture();
    let a = access(DeliveryMode::Auto, vec![ClientGroup::Npc]);
    store.enqueue_scoped(say("agent-auto"), 200, &a.scope_named("Storyteller").unwrap()).unwrap();
    store.enqueue(say("dm-local"), 201).unwrap();
    assert_eq!(store.commands(202)["commands"].as_array().unwrap().len(), 2);
    assert!(store.drafts().is_empty());
}

#[test]
fn approval_stage_closes_boot_and_access_reconfiguration_races() {
    let (store, access) = fixture();
    let scope = access.scope_named("Storyteller").unwrap();
    store.enqueue_scoped(say("proposal"), 200, &scope).unwrap();
    let draft = store.drafts().pop().unwrap();
    let native = say("native");
    let mut s = store.lock();
    s.assistant
        .sending
        .insert("native".into(), Sending { draft, edited: false, access: Some(access.clone()) });
    let mut approved = scope;
    approved.delivery = DeliveryMode::Auto;
    s.boot = "replacement".into();
    assert!(stage(&mut s, &native, &approved, 201).is_err());
    s.boot = "one".into();
    s.assistant.access = Some(super::tests::access(DeliveryMode::Drafts, vec![ClientGroup::Read]));
    assert!(stage(&mut s, &native, &approved, 202).is_err());
    s.assistant.access = Some(access);
    assert!(!stage(&mut s, &native, &approved, 203).unwrap());
}

#[test]
fn player_speech_is_reviewed_and_approval_rechecks_global_players_permission() {
    let (store, _) = fixture();
    let access = access(
        DeliveryMode::Drafts,
        vec![ClientGroup::Read, ClientGroup::ReadChat, ClientGroup::Players],
    );
    store.set_assistant_access(access.clone());
    store.set_policy(super::super::Policy { players: true, ..Default::default() });
    let scope = access.scope_named("Storyteller").unwrap();
    for (id, op) in [
        ("reply", json!({"op":"reply_to_player","player":"2","text":"Welcome"})),
        (
            "message",
            json!({"op":"pc_message","area":"room","target":"2","mode":"chat","text":"Welcome"}),
        ),
    ] {
        let mut input = op;
        input["id"] = json!(id);
        let command: Command = serde_json::from_value(input).unwrap();
        store.enqueue_scoped(command, 200, &scope).unwrap();
    }
    assert_eq!(store.drafts().len(), 2);
    assert!(store.commands(201)["commands"].as_array().unwrap().is_empty());
    store.set_policy(Default::default());
    assert!(store.decide_draft("reply", Some("No longer allowed"), 202).is_err());
    assert!(store.decide_draft("message", Some("No longer allowed"), 203).is_err());
    assert_eq!(store.drafts().len(), 2);
}

#[test]
fn every_draft_wakes_ui_even_when_client_and_operation_do_not_change() {
    use std::{
        io::{Read, Write},
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let (store, access) = fixture();
    let store = Arc::new(store);
    access.agents.set_allowed(true);
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let listener = crate::api::start_with_access(
        "127.0.0.1:0",
        access,
        store.clone(),
        Arc::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }),
        stop.clone(),
    )
    .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(listener.certificate.clone())).unwrap();
    let config = Arc::new(
        rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth(),
    );
    let mut replies = Vec::new();
    let mut counts = Vec::new();
    for id in ["first-draft", "second-draft"] {
        let client =
            rustls::ClientConnection::new(config.clone(), "moonglow-live".try_into().unwrap())
                .unwrap();
        let socket = std::net::TcpStream::connect(listener.address).unwrap();
        socket.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
        let mut tls = rustls::StreamOwned::new(client, socket);
        let body = json!({"id":id,"area":"room","target":"1","text":"Hello"}).to_string();
        write!(tls,"POST /v1/call/live_npc_say HTTP/1.1\r\nHost: moonglow-live\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}","s".repeat(32),body.len()).unwrap();
        tls.flush().unwrap();
        let mut response = String::new();
        tls.read_to_string(&mut response).unwrap();
        replies.push(response);
        counts.push(wakes.load(Ordering::SeqCst));
    }
    stop.store(true, Ordering::Release);
    listener.worker.join().unwrap();
    assert!(replies.iter().all(|r| r.starts_with("HTTP/1.1 200")), "{replies:?}");
    assert_eq!(counts, [1, 2]);
    assert_eq!(store.drafts().len(), 2);
    assert!(store.commands(crate::now_ms())["commands"].as_array().unwrap().is_empty());
}
