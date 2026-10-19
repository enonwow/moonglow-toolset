//! In-process cost of the gateway's per-session delivery: a store fed with game-like reports and
//! N sessions pumped every 50 ms exactly as `mg-gateway` does (`pushes`, then the JSON frame).
//!
//! `cargo run --release -p mg-live --example gateway_bench -- [sessions] [seconds] [focus]`
//!
//! With `focus` every session follows one object, as the load harness' desktops do.
//!
//! It is single threaded on purpose: the numbers are CPU per simulated second, free of
//! scheduling noise, and the live heap is counted exactly (a state that keeps growing under a
//! steady load shows here, unlike in RSS, which also holds the allocator's free memory). The
//! container scenario (`tools/live/gateway_load`) measures the rest.
#![allow(unsafe_code)] // only the counting allocator below
use mg_live::{
    gateway::{Gateway, Push, SessionConfig},
    gateway_frames::{ServerFrame, encode_server},
    store::LiveStore,
};
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
    time::Instant,
};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
// SAFETY: every call is forwarded to the system allocator with the caller's arguments unchanged;
// only a counter is added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size(), Relaxed);
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, which `System` shares.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        // SAFETY: `ptr` and `layout` come from the caller's earlier `alloc` through this type.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        LIVE.fetch_add(new, Relaxed);
        LIVE.fetch_sub(layout.size(), Relaxed);
        // SAFETY: as for `dealloc`; `new` is the caller's size.
        unsafe { System.realloc(ptr, layout, new) }
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;

const AREAS: usize = 50;
const DENSE: usize = 5;

fn row(area: usize, kind: usize, n: usize, tick: u64) -> Value {
    let id = format!("{area:02x}{kind}{n:05x}");
    let mut o = json!({"id":id,"tag":format!("T{area}_{kind}_{n}"),"r":format!("res_{}",n%40),
        "name":format!("Object {n}"),"x":(n%150) as f64+tick as f64*0.01,"y":(n/3%150) as f64,"z":0.0,"f":90.0,
        "hp":[30,30],"act":0,"own":false});
    if kind == 1 {
        o["faction"] = json!(n % 5);
        o["level"] = json!(1 + n % 20);
        o["lookHash"] = json!(n * 7919);
    } else {
        o["open"] = json!(false);
        o["useable"] = json!(true);
        o["app"] = json!(n % 300);
    }
    o
}
fn objects(area: usize, count: usize, tick: u64, all: bool) -> Value {
    let mut lists = [vec![], vec![], vec![], vec![]];
    let shares = [count / 4, count * 6 / 10, count / 20, count / 10];
    for (kind, share) in shares.iter().enumerate() {
        for n in 0..*share {
            // Deltas carry the creatures that moved (every eighth) and the probe.
            if all || (kind == 0 && (n == 0 || (n + tick as usize).is_multiple_of(8))) {
                lists[kind].push(row(area, kind + 1, n, tick));
            }
        }
    }
    let [cre, plc, doors, items] = lists;
    json!({"area":format!("a{area:02}"),"full":all,"size":[10,10],"cre":cre,"plc":plc,"doors":doors,"items":items})
}
fn report(
    seq: u64,
    full: bool,
    sizes: (usize, usize),
    events: &[Value],
    results: &[Value],
) -> Vec<u8> {
    let areas: Vec<_> = (0..AREAS)
        .map(|a| objects(a, if a < DENSE { sizes.0 } else { sizes.1 }, seq, full))
        .collect();
    let ack: Vec<_> = results.iter().map(|r| r["id"].clone()).collect();
    let mut r = json!({"v":1,"script_version":2,"prefix":"mglive","boot":"bench","seq":seq,
        "ack":ack,"results":results,"events":events,"dropped":0,"areas":areas});
    if !full {
        r["base_seq"] = json!(seq - 1);
    }
    serde_json::to_vec(&r).unwrap()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let sessions: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(30);
    let seconds: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(20);
    let focus = args.next().is_some_and(|v| v == "focus");
    let sizes = (1100, 200);
    let store = Arc::new(LiveStore::default());
    let gateway = Gateway::new(store.clone()).unwrap();
    let mut now = 1_000_000u64;
    let t = Instant::now();
    store.sync(&report(1, true, sizes, &[], &[]), now).unwrap();
    println!("full report ingested in {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    let ids: Vec<String> = (0..sessions).map(|i| format!("dm:load{i:03}")).collect();
    let mut watches = vec![];
    for (i, id) in ids.iter().enumerate() {
        let watch: Vec<String> =
            (0..3 + i % 3).map(|k| format!("a{:02}", (i * 7 + k * 13) % AREAS)).collect();
        let config = SessionConfig {
            id: id.clone(),
            id_prefix: format!("p{i:03}."),
            groups: ["read", "read_chat", "focus", "npc", "speech"].map(String::from).to_vec(),
            watch_limit: 16,
        };
        gateway.connect(config, None, &watch, now).unwrap();
        if focus {
            let area = (i * 11) % AREAS;
            gateway
                .focus_set(id, vec![(format!("a{area:02}"), format!("{area:02x}100003"))])
                .unwrap();
        }
        watches.push(watch);
    }
    gateway.tick(now).unwrap();
    let (mut t_sync, mut t_push, mut t_ser) = (0.0, 0.0, 0.0);
    let (mut bytes, mut frames, mut areas, mut metas) = (0usize, 0usize, 0usize, 0usize);
    let mut worst: f64 = 0.0;
    let (mut event_seq, mut command_no) = (0u64, 0u64);
    let mut results: Vec<Value> = vec![];
    let mut acks: Vec<Vec<(String, u64)>> = vec![vec![]; sessions];
    let baseline = LIVE.load(Relaxed);
    println!("live heap at start: {:.0} MB", baseline as f64 / 1e6);
    let wall = Instant::now();
    for second in 0..seconds {
        // The game's second: ten chat lines, and the results of last second's commands.
        let events: Vec<Value> = (0..10)
            .map(|_| {
                event_seq += 1;
                json!({"seq":event_seq,"kind":"chat","time":"1","id":"01100000","name":"Speaker",
                    "area":format!("a{:02}",event_seq % 50),"target":null,"channel":1,"text":format!("line {event_seq}")})
            })
            .collect();
        let body = report(second + 2, (second + 2) % 30 == 0, sizes, &events, &results);
        let t = Instant::now();
        store.sync(&body, now).unwrap();
        t_sync += t.elapsed().as_secs_f64();
        results.clear();
        for i in (0..sessions).step_by(5) {
            // A desktop speaks every few seconds: accepted now, answered in the next report.
            command_no += 1;
            let id = format!("p{i:03}.1234abcd.{command_no}");
            let area = &watches[i][0];
            let command = json!({"id":id,"boot":"bench","ttl_ms":30000,"op":"npc_say","area":area,
                "target":format!("{}100001",&area[1..]),"text":"Greetings"});
            let reply = gateway.commands(&ids[i], &[command], now).unwrap();
            if !reply.accepted.is_empty() {
                results.push(json!({"id":id,"ok":true}));
            }
        }
        for _ in 0..20 {
            now += 50;
            gateway.tick(now).unwrap();
            for (i, id) in ids.iter().enumerate() {
                let t = Instant::now();
                let pushes = gateway.pushes(id, now).unwrap();
                let spent = t.elapsed().as_secs_f64();
                worst = worst.max(spent);
                t_push += spent;
                let t = Instant::now();
                for push in pushes {
                    areas += usize::from(matches!(push, Push::Area(_)));
                    metas += usize::from(matches!(push, Push::Meta(_)));
                    if let Push::Result(r) = &push {
                        acks[i].push((r.id.clone(), r.rev));
                    }
                    bytes += encode_server(&ServerFrame::Push(push)).unwrap().len();
                    frames += 1;
                }
                t_ser += t.elapsed().as_secs_f64();
            }
        }
        for (i, id) in ids.iter().enumerate() {
            let results = std::mem::take(&mut acks[i]);
            gateway.ack(id, &results, store.event_cursor()).unwrap();
        }
        if (second + 1) % 60 == 0 {
            println!(
                "  after {:>4} s: live heap {:.0} MB",
                second + 1,
                LIVE.load(Relaxed) as f64 / 1e6
            );
        }
    }
    let total = wall.elapsed().as_secs_f64();
    let s = seconds as f64;
    println!(
        "{sessions} sessions, {seconds} simulated seconds: {:.0} ms CPU per second (ingest {:.0}, pushes {:.0}, frames {:.0})",
        total / s * 1e3,
        t_sync / s * 1e3,
        t_push / s * 1e3,
        t_ser / s * 1e3
    );
    println!(
        "per second: {:.0} frames ({:.0} areas, {:.0} meta), {:.1} MB; slowest pushes() {:.1} ms; live heap {:.0} -> {:.0} MB",
        frames as f64 / s,
        areas as f64 / s,
        metas as f64 / s,
        bytes as f64 / s / 1e6,
        worst * 1e3,
        baseline as f64 / 1e6,
        LIVE.load(Relaxed) as f64 / 1e6
    );
}
