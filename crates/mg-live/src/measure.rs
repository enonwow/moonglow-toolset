//! Opt-in replay benchmark of the production store and its individual stages.
//! Timings are observations, never CI performance assertions.
use super::*;
use crate::events::{EventLog, Query};
use std::{hint::black_box, io::Cursor, path::PathBuf, time::Instant};

fn stats(mut values: Vec<f64>) -> Value {
    if values.is_empty() {
        return json!({"n":0});
    }
    values.sort_by(f64::total_cmp);
    json!({"n":values.len(),"median_us":values[values.len()/2],
        "p95_us":values[(values.len()*95/100).min(values.len()-1)],
        "min_us":values[0],"max_us":values[values.len()-1]})
}

fn elapsed(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e6
}

#[test]
fn measure_live_replay() {
    if std::env::var("MGLIVE_MEASURE_REPLAY").as_deref() != Ok("1") {
        return;
    }
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-measure")
        .canonicalize()
        .unwrap();
    assert_eq!(std::fs::read_to_string(out.join(".owner")).unwrap().trim(), "live-measure-v1");
    let input = if let Ok(label) = std::env::var("MGLIVE_MEASURE_INPUT_RUN") {
        assert!(!label.is_empty() && label.len() <= 48);
        assert!(label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
        out.join("runs").join(label)
    } else {
        out.clone()
    };
    let session = std::env::var("MGLIVE_MEASURE_SESSION").ok().map(|label| {
        assert!(!label.is_empty() && label.len() < 40);
        assert!(label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
        out.parent().unwrap().join("live-stress").join(label).join("sample-reports.json")
    });
    let capture = |name: &str| {
        if let Some(path) = &session {
            return path.clone();
        }
        let path = input.join(format!("{name}-reports.json"));
        if name == "native" && !path.exists() { input.join("detail-reports.json") } else { path }
    };
    let mut all = serde_json::Map::new();
    let workloads = [
        "native",
        "delta",
        "one_look_surrogate",
        "one_look_delta",
        "three_look_surrogates",
        "synthetic_218",
    ];
    for name in if session.is_some() { &workloads[..1] } else { &workloads[..] } {
        let name = *name;
        let path = capture(if name == "synthetic_218" { "native" } else { name });
        let mut frames: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        // Begin at a baseline: otherwise the first deltas measure gap refusal,
        // not ordinary merge. Setup packets before this point are discarded.
        let baseline = frames
            .iter()
            .position(|f| f["base_seq"].is_null() && f["areas"][0]["full"] == true)
            .expect("captured full baseline");
        frames.drain(..baseline);
        if name == "synthetic_218" {
            // Explicit 218 REPORTABLE row stress case, not the 218-object demo
            // (whose waypoints/store do not cross the wire).
            frames.retain(|f| f["areas"][0]["full"] == true);
            for f in &mut frames {
                let a = &mut f["areas"][0];
                let others: usize = ["plc", "doors", "items"]
                    .iter()
                    .map(|k| a[*k].as_array().map_or(0, Vec::len))
                    .sum();
                let rows = a["cre"].as_array_mut().unwrap();
                let sample = rows[0].clone();
                while rows.len() + others < 218 {
                    let mut row = sample.clone();
                    row["id"] = json!(format!("{:x}", 0x100000 + rows.len()));
                    rows.push(row);
                }
            }
        }
        assert!(!frames.is_empty());
        let packets: Vec<_> = frames.iter().map(|f| serde_json::to_vec(f).unwrap()).collect();
        let mut measures: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
        for round in 0..21 {
            let store = LiveStore::default();
            for (frame, body) in frames.iter().zip(&packets) {
                let label = if frame["base_seq"].is_number() {
                    "delta"
                } else if frame["areas"][0]["full"] == true {
                    "full"
                } else {
                    "partial"
                };
                let mut push = |key: &str, value| {
                    if round > 0 {
                        measures.entry(format!("{label}.{key}")).or_default().push(value);
                    }
                };
                let start = Instant::now();
                let parsed: Value = serde_json::from_slice(black_box(body)).unwrap();
                push("json_parse", elapsed(start));
                let mut normalized = parsed.clone();
                let start = Instant::now();
                crate::wire_text::report(&mut normalized);
                push("text_boundary", elapsed(start));
                let start = Instant::now();
                let typed: SyncRequest = serde_json::from_value(normalized).unwrap();
                push("typed_deserialize", elapsed(start));
                if typed.base_seq.is_some() {
                    let state = store.lock();
                    for a in &typed.areas {
                        if let Some(old) = state.areas.get(&a.area) {
                            // Owned input preparation is outside the timed merge.
                            let mut old = old.clone();
                            let mut update = a.clone();
                            let start = Instant::now();
                            let plan = DeltaPlan::new(&old, &update).unwrap();
                            plan.apply(&mut old, &mut update);
                            black_box(old);
                            push("one_merge_delta", elapsed(start));
                        }
                    }
                }
                let start = Instant::now();
                black_box(
                    store.sync(black_box(body), 1000 + frame["seq"].as_u64().unwrap()).unwrap(),
                );
                push("store_sync_inclusive", elapsed(start));
                let mut envelope = format!(
                    "*3\r\n$7\r\nmessage\r\n$19\r\nmglive-events.state\r\n${}\r\n",
                    body.len()
                )
                .into_bytes();
                envelope.extend(body);
                envelope.extend(b"\r\n");
                let start = Instant::now();
                black_box(crate::resp::read_reply(&mut Cursor::new(&envelope)).unwrap());
                push("resp_decode_memory", elapsed(start));
            }
        }
        let result: serde_json::Map<_, _> =
            measures.into_iter().map(|(k, v)| (k, stats(v))).collect();
        all.insert(name.into(), json!({"frames":frames.len(), "packet_bytes":packets.iter().map(Vec::len).collect::<Vec<_>>(), "timings":result}));
    }
    let full: Vec<Value> =
        serde_json::from_slice(&std::fs::read(capture("native")).unwrap()).unwrap();
    let frame = full.iter().find(|f| f["areas"][0]["full"] == true).unwrap().clone();
    let mut normalized = frame;
    crate::wire_text::report(&mut normalized);
    let typed: SyncRequest = serde_json::from_value(normalized).unwrap();
    let rows = &typed.areas[0].cre;
    let mut changed = rows.clone();
    for row in &mut changed {
        row.combat = !row.combat;
    }
    let mut extra = serde_json::Map::new();
    for count in [0, 10, 256] {
        let mut ring = EventLog::default();
        let mut times = Vec::new();
        for round in 0..101 {
            let events: Vec<_> = (0..count).map(|i|json!({"seq":round*256+i+1,"kind":"chat","area":"demo_area","name":"Probe","id":"ab","text":"measurement event"})).collect();
            let start = Instant::now();
            ring.game("measure", &events, 0, 1);
            if round > 0 {
                times.push(elapsed(start));
            }
        }
        extra.insert(format!("events_game_{count}"), stats(times));
    }
    for (name, after) in [("derive_unchanged", rows), ("derive_all_combat_changed", &changed)] {
        let mut ring = EventLog::default();
        let mut times = Vec::new();
        for _ in 0..100 {
            let start = Instant::now();
            ring.derive("demo_area", Some(rows), after, 1);
            times.push(elapsed(start));
        }
        extra.insert(name.into(), stats(times));
    }
    let logs = out.join("bench-logs");
    assert!(!logs.exists(), "Prior log benchmark artifacts require reconciliation");
    std::fs::create_dir(&logs).unwrap();
    let mut deleted = 0;
    for count in [1, 10, 256] {
        let mut ring = EventLog::default();
        let events: Vec<_> = (0..count).map(|i|json!({"seq":i+1,"kind":if i%2==0 {"chat"} else {"damage"},"area":"demo_area","name":"Probe","id":"ab","text":"measurement event"})).collect();
        ring.game("measure", &events, 0, 1);
        let events = ring.query(&Query { limit: 500, ..Default::default() });
        let mut times = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            crate::logs::append(&logs, "measure", &events, true).unwrap();
            times.push(elapsed(start));
        }
        extra.insert(format!("logs_append_{count}"), stats(times));
    }
    let files: Vec<_> =
        std::fs::read_dir(logs.join("measure")).unwrap().map(Result::unwrap).collect();
    for file in &files {
        deleted += file.metadata().unwrap().len();
    }
    let profile = std::env::var("MGLIVE_MEASURE_PROFILE").unwrap_or_else(|_| "test1".into());
    assert!(["test1", "dev0"].contains(&profile.as_str()));
    let mut result = json!({"schema":"live-rust-measure-v1","profile":profile,"profile_note":"test1: ordinary profile.test opt-level=1; dev0: explicit profile.test.package.mg-live.opt-level=0; dependencies unchanged; neither is release","workloads":all,"isolated":extra,"cleanup_logical_bytes":deleted,"cleanup_complete":false,"limits":"RESP from memory excludes socket wait; no GUI wake/render; store_sync inclusive includes parse/text/validation/lock/merge/events; logs writer runs on separate thread in app"});
    let output_dir = if let Some(path) = &session {
        path.parent().unwrap().to_path_buf()
    } else if let Ok(label) = std::env::var("MGLIVE_MEASURE_RUN") {
        assert!(!label.is_empty() && label.len() <= 48);
        assert!(label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
        let dir = out.join("runs").join(label);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    } else {
        out.clone()
    };
    let output = output_dir.join(format!("rust-{profile}.json"));
    result["input_directory"] = json!(input);
    result["session_sample"] = json!(session);
    // Preserve observations before removing only this invocation's throwaway log.
    std::fs::write(&output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    for file in files {
        std::fs::remove_file(file.path()).unwrap();
    }
    std::fs::remove_dir(logs.join("measure")).unwrap();
    std::fs::remove_dir(&logs).unwrap();
    result["cleanup_complete"] = json!(true);
    std::fs::write(&output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
}
