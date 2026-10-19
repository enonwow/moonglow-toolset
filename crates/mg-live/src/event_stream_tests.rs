use super::*;
use crate::events::Query;
use serde_json::json;
use std::net::SocketAddr;
fn directory() -> PathBuf {
    std::env::temp_dir().join(format!("mg-stream-{}", crate::service::generate_token().unwrap()))
}
fn entry(n: u64) -> Entry {
    Entry {
        id: format!("{n}-0"),
        n,
        epoch: "epoch".into(),
        source: "boot".into(),
        kind: "game".into(),
        codepage: "cp1250".into(),
        data: json!({"seq":n,"kind":"chat","area":"a","text":"hello"}),
        lost_before: 0,
        reset_before: false,
    }
}
fn connection(ack: bool) -> (std::io::BufReader<std::net::TcpStream>, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let worker = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut r = std::io::BufReader::new(stream);
        let request = crate::resp::read_reply(&mut r).unwrap();
        let args = array(&request).unwrap();
        assert_eq!(bulk(&args[0]).unwrap(), "XACK");
        if ack {
            r.get_mut().write_all(b":1\r\n").unwrap();
        }
    });
    (std::io::BufReader::new(stream), worker)
}
#[test]
fn crash_after_apply_before_ack_replays_once_and_resumes() {
    let dir = directory();
    let status = Arc::new(Mutex::new(Status::default()));
    let group;
    {
        let journal = Inbox::new(&dir, "server").unwrap();
        group = journal.group.clone();
        let inbox = Arc::new(Mutex::new(Some(journal)));
        let store = LiveStore::default();
        let (mut c, t) = connection(false);
        assert!(
            apply(&mut c, "s", &group, vec![entry(1), entry(2)], &inbox, &status, &store).is_err()
        );
        t.join().unwrap();
        assert_eq!(store.events(&Query::default()).0.len(), 2);
    }
    let journal = Inbox::new(&dir, "server").unwrap();
    assert_eq!(journal.group, group);
    let store = LiveStore::default();
    journal.restore(&store).unwrap();
    let inbox = Arc::new(Mutex::new(Some(journal)));
    let (mut c, t) = connection(true);
    apply(&mut c, "s", &group, vec![entry(1), entry(2), entry(3)], &inbox, &status, &store)
        .unwrap();
    t.join().unwrap();
    let events = store.events(&Query::default()).0;
    assert_eq!(
        events.iter().map(|e| e.data["game_seq"].as_u64().unwrap()).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(status.lock().unwrap().lost, 0);
    drop(inbox);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn inbox_replay_across_two_boots_does_not_duplicate_history_or_notices() {
    let dir = directory();
    let mut inbox = Inbox::new(&dir, "server").unwrap();
    let first = entry(1);
    let mut second = entry(2);
    second.source = "second-boot".into();
    second.data["seq"] = json!(1);
    second.lost_before = 1;
    inbox.append(&[first, second]).unwrap();
    let store = LiveStore::default();
    inbox.restore(&store).unwrap();
    let before = store.events(&Query::default());
    inbox.restore(&store).unwrap();
    assert_eq!(store.events(&Query::default()), before);
    assert_eq!(before.0.len(), 3);
    drop(inbox);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn overflow_is_exact_and_duplicate_ack_does_not_repeat_it() {
    let dir = directory();
    let journal = Inbox::new(&dir, "server").unwrap();
    let group = journal.group.clone();
    let inbox = Arc::new(Mutex::new(Some(journal)));
    let status = Arc::new(Mutex::new(Status::default()));
    let store = LiveStore::default();
    for _ in 0..2 {
        let (mut c, t) = connection(true);
        apply(&mut c, "s", &group, vec![entry(8), entry(9)], &inbox, &status, &store).unwrap();
        t.join().unwrap();
    }
    assert_eq!(status.lock().unwrap().lost, 7);
    let events = store.events(&Query::default()).0;
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].data["line"], "7 events lost: queue overflow");
    drop(inbox);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn inbox_torn_tail_lock_and_bounded_compaction() {
    let dir = directory();
    let mut inbox = Inbox::new(&dir, "server").unwrap();
    assert!(Inbox::new(&dir, "server").is_err());
    inbox.append(&[entry(1), entry(2)]).unwrap();
    inbox.file.write_all(b"{\"torn\"").unwrap();
    drop(inbox);
    let mut inbox = Inbox::new(&dir, "server").unwrap();
    assert_eq!(inbox.last().unwrap().n, 2);
    inbox.append(&[entry(3)]).unwrap();
    inbox.compact().unwrap();
    drop(inbox);
    let inbox = Inbox::new(&dir, "server").unwrap();
    assert_eq!(inbox.rows.len(), 3);
    assert_eq!(inbox.last().unwrap().n, 3);
    drop(inbox);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn compaction_failure_does_not_advance_the_ack_receipt() {
    let dir = directory();
    let mut inbox = Inbox::new(&dir, "server").unwrap();
    inbox.append(&[entry(1)]).unwrap();
    inbox.written = MAX_ROWS * 2 + 1;
    let obstruction = inbox.root.join("compact.tmp");
    fs::create_dir(&obstruction).unwrap();
    assert!(inbox.append(&[entry(2)]).is_err());
    assert_eq!(inbox.last().unwrap().n, 1);
    assert!(!inbox.contains(&entry(2)));
    fs::remove_dir(obstruction).unwrap();
    inbox.append(&[entry(2)]).unwrap();
    drop(inbox);
    let inbox = Inbox::new(&dir, "server").unwrap();
    assert_eq!(inbox.last().unwrap().n, 2);
    assert_eq!(inbox.rows.len(), 2);
    drop(inbox);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn stream_is_independent_of_state_boot_and_legacy_fast_copies() {
    let store = LiveStore::default();
    store.sync(br#"{"v":1,"boot":"new","seq":1,"event_stream":true,"events":[{"seq":9999,"kind":"chat"}],"areas":[]}"#,1).unwrap();
    assert!(!store.event_packet(br#"{"event_stream":true}"#, 1).unwrap());
    store.stream_event(&entry(1), 2).unwrap();
    store.stream_event(&entry(1), 3).unwrap();
    assert_eq!(store.events(&Query::default()).0.len(), 1);
    assert_eq!(store.status(4)["boot"], "new");
    assert_eq!(store.events(&Query::default()).0[0].data["boot"], "boot");
}
#[test]
fn malformed_stream_rows_are_refused_including_truncated_resp() {
    for v in [json!(null), json!({}), json!({"seq":0,"kind":"chat"})] {
        let mut e = entry(1);
        e.data = v;
        assert!(e.validate().is_err());
    }
    assert!(entries(&Reply::Array(Some(vec![Reply::Int(1)]))).is_err());
    let wire = b"*1\r\n*2\r\n$3\r\n1-0\r\n$-1\r\n";
    for n in 0..wire.len() {
        assert!(crate::resp::read_reply(&mut std::io::Cursor::new(&wire[..n])).is_err());
    }
    assert!(
        entries(&crate::resp::read_reply(&mut std::io::Cursor::new(wire)).unwrap())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn real_redis_reclaims_pending_dedupes_producer_and_detects_trim() {
    let Ok(port) = std::env::var("MGLIVE_STREAM_TEST_REDIS") else { return };
    let port: u16 = port.parse().unwrap();
    assert_eq!(port, 6393, "Only the owned private Redis helper");
    let stop = Arc::new(AtomicBool::new(false));
    let addresses = vec![SocketAddr::from(([127, 0, 0, 1], port))];
    let mut c = connect(&addresses, &stop).unwrap();
    let prefix = format!("mg-test-{}", &crate::service::generate_token().unwrap()[..16]);
    let key = format!("{prefix}.events");
    let config = RedisConfig { host: "127.0.0.1".into(), port, prefix: prefix.clone() };
    let dir = directory();
    let journal = Inbox::new(&dir, &prefix).unwrap();
    let group = journal.group.clone();
    let inbox = Arc::new(Mutex::new(Some(journal)));
    let status = Arc::new(Mutex::new(Status::default()));
    let store = Arc::new(LiveStore::default());
    let produce = |c: &mut std::io::BufReader<std::net::TcpStream>, first: u64, last: u64| {
        let mut args: Vec<String> = vec![
            "EVAL".into(),
            include_str!("../../../tools/live/event-stream.lua").into(),
            "3".into(),
            key.clone(),
            format!("{key}.meta"),
            format!("{key}.producer:game:boot"),
            "boot".into(),
            "game".into(),
            "cp1250".into(),
        ];
        for n in first..=last {
            args.push(n.to_string());
            args.push(entry(n).data.to_string());
        }
        assert_eq!(
            request(c, &args.iter().map(|s| s.as_bytes()).collect::<Vec<_>>()).unwrap(),
            Reply::Int(last as i64)
        );
    };
    produce(&mut c, 1, 3);
    produce(&mut c, 1, 3);
    assert_eq!(request(&mut c, &[b"XLEN", key.as_bytes()]).unwrap(), Reply::Int(3));
    assert_eq!(
        request(&mut c, &[b"XGROUP", b"CREATE", key.as_bytes(), group.as_bytes(), b"0"]).unwrap(),
        Reply::Simple("OK".into())
    );
    // A crashed consumer left an entire page pending before application.
    request(
        &mut c,
        &[
            b"XREADGROUP",
            b"GROUP",
            group.as_bytes(),
            b"dead",
            b"COUNT",
            b"3",
            b"STREAMS",
            key.as_bytes(),
            b">",
        ],
    )
    .unwrap();
    let start = |stop: Arc<AtomicBool>, store: Arc<LiveStore>, inbox: SharedInbox| {
        let (config, addresses, status) = (config.clone(), addresses.clone(), status.clone());
        std::thread::spawn(move || {
            run(config, addresses, inbox, status, store, stop, Arc::new(|| {}))
        })
    };
    let worker = start(stop.clone(), store.clone(), inbox.clone());
    let wait = |expected: u64| {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let last = inbox.lock().unwrap().as_ref().unwrap().last().map_or(0, |e| e.n);
            if last == expected && status.lock().unwrap().backlog == 0 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{}",
                serde_json::to_string(&*status.lock().unwrap()).unwrap()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    wait(3);
    stop.store(true, Ordering::Release);
    worker.join().unwrap();
    assert_eq!(store.events(&Query::default()).0.len(), 3);
    // Queue overflow while Moonglow is absent, including a trimmed pending row.
    produce(&mut c, 4, 4);
    request(
        &mut c,
        &[
            b"XREADGROUP",
            b"GROUP",
            group.as_bytes(),
            b"dead",
            b"COUNT",
            b"1",
            b"STREAMS",
            key.as_bytes(),
            b">",
        ],
    )
    .unwrap();
    for first in (5..=10204).step_by(128) {
        produce(&mut c, first, (first + 127).min(10204));
    }
    let first = entries(
        &request(&mut c, &[b"XRANGE", key.as_bytes(), b"-", b"+", b"COUNT", b"1"]).unwrap(),
    )
    .unwrap()[0]
        .n;
    assert!(first > 4);
    stop.store(false, Ordering::Release);
    let worker = start(stop.clone(), store.clone(), inbox.clone());
    wait(10204);
    stop.store(true, Ordering::Release);
    worker.join().unwrap();
    let pending = request(&mut c, &[b"XPENDING", key.as_bytes(), group.as_bytes()]).unwrap();
    assert_eq!(array(&pending).unwrap()[0], Reply::Int(0));
    assert_eq!(status.lock().unwrap().lost, first - 4);
    assert_eq!(inbox.lock().unwrap().as_ref().unwrap().last().unwrap().n, 10204);
    let expected_lost = first - 4;
    // A lost Redis database with the SAME game process must get a new epoch.
    // Using only the game boot here would silently discard new ordinals <= the
    // previous receipt. Only this random test namespace is removed.
    for name in [&key, &format!("{key}.meta"), &format!("{key}.producer:game:boot")] {
        request(&mut c, &[b"DEL", name.as_bytes()]).unwrap();
    }
    produce(&mut c, 10205, 10205);
    stop.store(false, Ordering::Release);
    let worker = start(stop.clone(), store.clone(), inbox.clone());
    wait(1);
    stop.store(true, Ordering::Release);
    worker.join().unwrap();
    assert!(store.events(&Query::default()).0.iter().any(|e| e.data["transport"] == "queue_reset"));
    assert_eq!(inbox.lock().unwrap().as_ref().unwrap().last().unwrap().data["seq"], 10205);
    println!(
        "Redis stream proof: initial=3 retry_duplicates=0 pending_reclaimed=3 trimmed_pending=1 produced=10204 overflow={expected_lost} backlog=0 pending=0 same_boot_queue_reset=passed"
    );
    drop(inbox);
    fs::remove_dir_all(dir).unwrap();
    // Only this test's random namespace, never FLUSHDB or shared Redis keys.
    for name in [&key, &format!("{key}.meta"), &format!("{key}.producer:game:boot")] {
        request(&mut c, &[b"DEL", name.as_bytes()]).unwrap();
    }
}
