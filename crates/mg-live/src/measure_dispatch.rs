//! Opt-in catalogue/dispatch timings. No socket or engine, no CI timing limits.
use super::*;
use std::{hint::black_box, path::PathBuf};

#[test]
fn measure_live_dispatch() {
    if std::env::var("MGLIVE_MEASURE_REPLAY").as_deref() != Ok("1") {
        return;
    }
    let label = std::env::var("MGLIVE_MEASURE_RUN").expect("named measurement run");
    assert!(!label.is_empty() && label.len() <= 48);
    assert!(label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output/live-measure");
    assert_eq!(std::fs::read_to_string(root.join(".owner")).unwrap().trim(), "live-measure-v1");
    let store = LiveStore::default();
    let time = now_ms();
    store.sync(br#"{"v":1,"boot":"dispatch-measure","seq":1,"areas":[{"area":"events","full":true,"cre":[{"id":"a","name":"Probe"}]}]}"#, time).unwrap();
    let mut result = serde_json::Map::new();
    for route in ["legacy", "call"] {
        for valid in [true, false] {
            let mut input =
                json!({"id":"dispatch-measure","area":"events","target":"a","text":"Probe"});
            if route == "legacy" {
                input["op"] = json!("npc_say");
            }
            if !valid {
                input["unknown"] = json!(1);
            }
            let mut samples = Vec::new();
            for round in 0..501 {
                let input = input.clone(); // outside the measured boundary
                let start = Instant::now();
                let reply = if route == "legacy" {
                    command(black_box(input), &store, time)
                } else {
                    call("live_npc_say", black_box(input), &store, &AtomicBool::new(false))
                };
                let elapsed = start.elapsed().as_secs_f64() * 1e6;
                assert_eq!(reply.is_ok(), valid, "{reply:?}");
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            result.insert(format!("{route}.{}", if valid { "valid" } else { "invalid" }),
                json!({"n":samples.len(),"median_us":samples[samples.len()/2],"p95_us":samples[samples.len()*95/100]}));
        }
    }
    // GUI-style revision/cursor polls, with a populated event ring.
    let events: Vec<_> = (1..=256)
        .map(|n| json!({"seq":n,"kind":"chat","area":"events","id":"a","text":"measurement"}))
        .collect();
    store
        .sync(
            serde_json::to_string(
                &json!({"v":1,"boot":"dispatch-measure","seq":2,"areas":[],"events":events}),
            )
            .unwrap()
            .as_bytes(),
            time,
        )
        .unwrap();
    for name in ["status_for_revision", "events_for_cursor", "revision", "event_cursor"] {
        let mut samples = Vec::new();
        for _ in 0..1000 {
            let start = Instant::now();
            if name == "status_for_revision" {
                black_box(store.status(time));
            } else if name == "revision" {
                black_box(store.revision());
            } else if name == "event_cursor" {
                black_box(store.event_cursor());
            } else {
                black_box(store.events(&crate::events::Query { limit: 1, ..Default::default() }));
            }
            samples.push(start.elapsed().as_secs_f64() * 1e6);
        }
        samples.sort_by(f64::total_cmp);
        result.insert(name.into(),json!({"n":samples.len(),"median_us":samples[samples.len()/2],"p95_us":samples[samples.len()*95/100]}));
    }
    let output = root.join("runs").join(label);
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("rust-dispatch.json"), serde_json::to_vec_pretty(&json!({
        "schema":"live-dispatch-measure-v1", "profile":"test opt-level=1",
        "note":"Production entrypoints incl catalogue/schema, typed validation and enqueue; valid case repeats an idempotent command; cloned input setup excluded; no HTTP/TLS/network latency.",
        "workloads":result
    })).unwrap()).unwrap();
}
