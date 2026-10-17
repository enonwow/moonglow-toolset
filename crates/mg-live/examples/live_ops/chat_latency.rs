//! Actual private engine -> Redis -> service -> authenticated API timing.
use super::Client;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufReader, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) fn run(client: Client, boot: String, baseline: bool) {
    // WSL/Windows wall clocks can slew independently. Timestamp a tiny marker
    // published immediately before native chat on Windows, like store ingestion.
    // This excludes the marker's one Redis/network hop; record its RTT bound.
    let socket = TcpStream::connect("127.0.0.1:6392").unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(45))).unwrap();
    socket.set_nodelay(true).unwrap();
    let mut redis = BufReader::new(socket);
    let mut rtts = Vec::new();
    for _ in 0..7 {
        let timer = Instant::now();
        redis.get_mut().write_all(&mg_live::resp::encode(&[b"PING"])).unwrap();
        assert_eq!(
            mg_live::resp::read_reply(&mut redis).unwrap(),
            mg_live::resp::Reply::Simple("PONG".into())
        );
        rtts.push(timer.elapsed().as_secs_f64() * 1000.0);
    }
    redis
        .get_mut()
        .write_all(&mg_live::resp::encode(&[b"SUBSCRIBE", b"mglive-events.latency-marker"]))
        .unwrap();
    mg_live::resp::read_reply(&mut redis).unwrap();
    let markers = Arc::new(Mutex::new(HashMap::new()));
    let collected = markers.clone();
    let observer = std::thread::spawn(move || {
        for _ in 0..20 {
            let mg_live::resp::Reply::Array(Some(parts)) =
                mg_live::resp::read_reply(&mut redis).unwrap()
            else {
                panic!("marker envelope")
            };
            let instant = Instant::now();
            let time = mg_live::now_ms();
            let mg_live::resp::Reply::Bulk(Some(text)) = &parts[2] else { panic!("marker body") };
            collected
                .lock()
                .unwrap()
                .insert(String::from_utf8(text.clone()).unwrap(), (time, instant));
        }
    });
    let actor =
        client.command(
            "spawn",
            json!({"kind":"creature","r":"nw_bartender","x":3,"y":3,"z":0,"f":0}),
        )["obj"]
            .clone();
    let mut cursor = client.call("events", json!({}))["last"].clone();
    let queued =
        client.call("run_script", json!({"area":"events","target":actor,"script":"mget_latency"}));
    let start = Instant::now();
    let mut samples = Vec::<Value>::new();
    let mut calls = 0;
    while samples.len() < 20 {
        assert!(start.elapsed() < Duration::from_secs(40));
        let mut input = json!({"since":cursor,"kinds":["chat"],"text":"latency-"});
        if !baseline {
            input["wait_ms"] = json!(30000);
        }
        let answer = client.call("events", input);
        let received = mg_live::now_ms();
        let received_instant = Instant::now();
        calls += 1;
        cursor = answer["last"].clone();
        for e in answer["events"].as_array().unwrap() {
            let text = e["data"]["text"].as_str().unwrap();
            let (origin_ms, origin) = *markers
                .lock()
                .unwrap()
                .get(text)
                .expect("origin marker arrived before its chat event");
            let store_ms = e["at_ms"].as_u64().unwrap();
            samples.push(json!({"seq":e["data"]["game_seq"],"text":text,
                "marker_to_store_ms":store_ms as i64-origin_ms as i64,
                "marker_to_api_ms":received_instant.duration_since(origin).as_secs_f64()*1000.0,
                "store_to_api_ms":received as i64-store_ms as i64}));
        }
        if baseline {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    assert_eq!(samples.len(), 20, "immediate/fallback duplicates must be suppressed");
    observer.join().unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        client.call("events", json!({"since":cursor,"kinds":["chat"],"text":"latency-"}))["events"]
            .as_array()
            .unwrap()
            .is_empty(),
        "buffer fallback repeated a fast event"
    );
    let result = client.call("wait", json!({"id":queued["queued"],"timeout_ms":1000}));
    assert_eq!(result["ok"], true);
    let final_seq = client.call("status", json!({}))["seq"].clone();
    client.service.stop();
    println!(
        "{}",
        json!({"passed":true,"boot":boot,"final_seq":final_seq,"baseline":baseline,"calls":calls,"samples":samples,"redis_ping_rtt_ms":rtts,
        "scope":"Fixture marker immediately before native NPC talk; marker receipt and ingestion on same Windows clock; API interval monotonic. Excludes marker one-hop Redis delivery. No connected player or LLM generation time. Baseline polls at 1s."})
    );
}
