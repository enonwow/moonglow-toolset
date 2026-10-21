mod link_support;
use link_support::*;
use mg_live::store::Command;
use mg_live_link::{LinkError, LinkService, LinkStatus};
use serde_json::json;
use std::{sync::Arc, time::Duration};

struct SecretLog;
static LEAKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
impl log::Log for SecretLog {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        if format!("{}", record.args()).contains(SECRET) {
            LEAKED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn flush(&self) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_or_oversized_server_frames_stop_cleanly_without_secret_logs() {
    let _ = log::set_logger(&SecretLog);
    log::set_max_level(log::LevelFilter::Trace);
    let garbage = format!("{{\"t\":\"{SECRET}\"}}");
    let mut unknown = vec![129, garbage.len() as u8];
    unknown.extend(garbage.as_bytes());
    let malformed = format!("{{\"{SECRET}\":");
    unknown.extend([129, malformed.len() as u8]);
    unknown.extend(malformed.as_bytes());
    let mut oversized = vec![129, 127];
    oversized.extend_from_slice(&((16 * 1024 * 1024 + 1) as u64).to_be_bytes());
    for (bytes, error) in [(unknown, LinkError::Protocol), (oversized, LinkError::TooLarge)] {
        let host = Host::new().await;
        let service = LinkService::start(host.config(), &tokens(), Arc::new(|| {})).unwrap();
        until(|| service.snapshot().status == LinkStatus::Connected).await;
        host.inject(bytes);
        until(|| service.snapshot().status == LinkStatus::Stopped).await;
        assert_eq!(service.snapshot().error, Some(error));
        assert!(!format!("{service:?} {:?}", service.snapshot()).contains(SECRET));
    }
    assert!(
        !LEAKED.load(std::sync::atomic::Ordering::Relaxed),
        "session credential entered a log record"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_connect_mirror_script_resume_without_duplicates_and_result_ack() {
    let host = Host::new().await;
    host.report(1, json!([]), json!([])).await;
    let service = LinkService::start(host.config(), &tokens(), Arc::new(|| {})).unwrap();
    until(|| service.store().area("square").is_some()).await;
    assert_eq!(service.snapshot().welcome.unwrap().user.id, "dm:test");
    host.report(
        2,
        json!([{"seq":1,"kind":"chat","area":"square","channel":1,"text":"First"}]),
        json!([]),
    )
    .await;
    until(|| {
        service.store().event_cursor() == host.store.event_cursor()
            && service.store().area("square").is_some_and(|a| a.seq == 2)
    })
    .await;
    equal(&service, &host);
    let command: Command =
        serde_json::from_value(json!({"op":"stop_patrol","area":"square","target":"a1"})).unwrap();
    let id = service.enqueue(command).unwrap();
    let prefix = service.snapshot().welcome.unwrap().id_prefix;
    let tail = id.strip_prefix(&prefix).unwrap();
    let (process, counter) = tail.split_once('.').unwrap();
    assert_eq!(process.len(), 8);
    assert!(process.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(counter.parse::<u64>().unwrap() > 0);
    until(|| !host.store.commands(mg_live::now_ms())["commands"].as_array().unwrap().is_empty())
        .await;
    host.report(3, json!([]), json!([{"id":id,"ok":true}])).await;
    until(|| service.store().result(&id).is_some_and(|r| r.ok && !r.pending)).await;
    until(|| {
        host.frames().iter().any(|f| {
            f["t"] == "ack"
                && f["results"].as_array().is_some_and(|rows| {
                    rows.iter().any(|r| r[0] == id && r[1].as_u64().unwrap() > 0)
                })
        })
    })
    .await;
    let cursor = service.store().event_cursor();
    host.drop_sockets();
    host.report(
        4,
        json!([{"seq":2,"kind":"chat","area":"square","channel":1,"text":"Second"}]),
        json!([]),
    )
    .await;
    until(|| {
        service.snapshot().connections >= 2
            && service.store().event_cursor() == host.store.event_cursor()
            && service.store().area("square").is_some_and(|a| a.seq == 4)
    })
    .await;
    assert!(service.snapshot().welcome.unwrap().resumed);
    let hellos: Vec<_> = host.frames().into_iter().filter(|f| f["t"] == "hello").collect();
    assert_eq!(hellos[1]["resume"]["event_n"], cursor);
    assert_eq!(hellos[1]["resume"]["boot"], "boot");
    assert!(hellos[1]["resume"]["areas"]["square"].is_number());
    equal(&service, &host);
    let events = service.store().events(&Default::default());
    assert_eq!(events.0.iter().filter(|e| e.data["text"] == "First").count(), 1);
    assert_eq!(events.0.iter().filter(|e| e.data["text"] == "Second").count(), 1);
    let reply = service.call("status", json!({})).unwrap();
    assert!(reply_async(reply).await.unwrap().is_object());
    service.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_pin_never_sends_authorization_or_reconnects() {
    let host = Host::new().await;
    let mut config = host.config();
    config.gateway = mg_live_link::Gateway::new(&host.origin, Some(&"00".repeat(32))).unwrap();
    let service = LinkService::start(config, &tokens(), Arc::new(|| {})).unwrap();
    until(|| service.snapshot().status == LinkStatus::Stopped).await;
    assert_eq!(service.snapshot().error, Some(LinkError::PinMismatch));
    assert!(host.frames().is_empty());
    assert!(!format!("{service:?} {:?}", service.snapshot()).contains(SECRET));
}

fn equal(service: &LinkService, host: &Host) {
    let mut source = host.store.area("square").unwrap();
    let mut mirror = service.store().area("square").unwrap();
    source.received_ms = 0;
    mirror.received_ms = 0;
    assert_eq!(source, mirror);
    assert_eq!(service.store().factions(), host.store.factions());
    assert_eq!(service.store().events(&Default::default()), host.store.events(&Default::default()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_requests_fail_locally_and_calls_are_bounded() {
    let host = Host::new().await;
    let service = LinkService::start(host.config(), &tokens(), Arc::new(|| {})).unwrap();
    until(|| service.snapshot().status == LinkStatus::Connected).await;
    assert_eq!(
        service.call("status", json!({"extra":"x".repeat(65536)})).unwrap_err(),
        LinkError::TooLarge
    );
    // Reserve all permits independently of worker scheduling to exercise the same
    // public admission path under deterministic server backpressure (see unit test).
    assert_eq!(
        service.set_watched_areas(&vec!["square".into(); 17]).unwrap_err(),
        LinkError::Protocol
    );
    assert!(reply_async(service.set_watched_areas(&[]).unwrap()).await.is_ok());
    service.stop();
    tokio::time::sleep(Duration::from_millis(20)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_timeout_closes_the_socket_and_never_replays_the_call() {
    let host = Host::new().await;
    let mut config = host.config();
    config.request_timeout = Duration::from_millis(100);
    let service = LinkService::start(config, &tokens(), Arc::new(|| {})).unwrap();
    until(|| service.snapshot().status == LinkStatus::Connected).await;
    host.inject(Vec::new());
    tokio::time::sleep(Duration::from_millis(40)).await;
    let reply = service.call("status", json!({})).unwrap();
    assert_eq!(reply_async(reply).await.unwrap_err(), LinkError::Timeout);
    until(|| service.snapshot().status == LinkStatus::Reconnecting).await;
    until(|| service.snapshot().connections >= 2).await;
    assert_eq!(host.frames().iter().filter(|f| f["t"] == "req" && f["op"] == "call").count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn facade_reads_auxiliary_pushes_and_preserves_host_refusals() {
    use mg_live::{gateway::Push, gateway_frames::Limits, store::Policy};
    let host = Host::new().await;
    host.report(1, json!([]), json!([])).await;
    let service = LinkService::start(host.config(), &tokens(), Arc::new(|| {})).unwrap();
    until(|| service.store().area("square").is_some()).await;
    let draft = json!({"command":{"id":"draft","op":"npc_say","area":"square","target":"a1","text":"Hello"},
        "client":"Helper","boot":"boot","created_ms":1,"reply_to":null,"name":"Guard","area":"square","notice":null});
    for push in [
        Push::Drafts { rev: 7, items: vec![draft] },
        Push::Presence {
            rev: 8,
            actors: vec![json!({"id":"dm:test","name":"Test"})],
            focus: Default::default(),
        },
        Push::Book { kind: "npc".into(), key: "tag:guard".into(), rev: 9 },
        Push::Session {
            groups: vec!["read".into()],
            policy: Policy { scripts: false, ..Default::default() },
            limits: serde_json::to_value(Limits::default()).unwrap(),
        },
        Push::Notice { level: "info".into(), text: "Ready".into() },
    ] {
        host.core.publish("dm:test".into(), push).await.unwrap();
    }
    until(|| service.snapshot().notices.len() == 1).await;
    assert_eq!(service.drafts().unwrap()[0].text(), "Hello");
    assert_eq!(service.draft_revision(), 7);
    assert_eq!(service.agents_revision(), 8);
    assert_eq!(service.agents()[0]["id"], "dm:test");
    assert_eq!(service.book_revision(), 1);
    assert!(!service.store().policy().scripts);
    assert_eq!(service.take_notices(), vec![("info".into(), "Ready".into())]);
    assert!(
        reply_async(service.set_focus(&[("square".into(), "a1".into())]).unwrap()).await.is_ok()
    );
    assert_eq!(
        reply_async(service.set_allow_agents(true).unwrap()).await.unwrap_err(),
        LinkError::Forbidden
    );
    assert_eq!(service.allow_agents(), None);
    // The fixture has no campaign book configured; a failed write stays a
    // failure even now that the development host implements book.put.
    assert_eq!(
        reply_async(service.save_object_note("note-key", "Text", 0).unwrap()).await.unwrap_err(),
        LinkError::Rejected
    );
    assert_eq!(
        reply_async(service.request("fixture.unsupported", json!({})).unwrap()).await.unwrap_err(),
        LinkError::Unsupported
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_idle_or_closed_bye_reconnects_instead_of_stopping() {
    use mg_live::gateway_frames::{Bye, ServerFrame};
    for code in ["idle", "closed"] {
        let host = Host::new().await;
        let service = LinkService::start(host.config(), &tokens(), Arc::new(|| {})).unwrap();
        until(|| service.snapshot().status == LinkStatus::Connected).await;
        let first = service.snapshot().connections;
        let bye =
            serde_json::to_string(&ServerFrame::Bye(Bye { code: code.into(), msg: code.into() }))
                .unwrap();
        let mut frame = vec![129, bye.len() as u8];
        frame.extend(bye.as_bytes());
        host.inject(frame);
        // A quiet desktop (sleep, a busy machine) is not a protocol fault: it comes back.
        until(|| service.snapshot().connections > first).await;
        assert_eq!(service.snapshot().status, LinkStatus::Connected, "{code}");
    }
}
