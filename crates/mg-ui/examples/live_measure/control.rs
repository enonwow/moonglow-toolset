//! Separate, explicitly synthetic retained-event and conversation-memory control.
//! Twelve areas are needed to exceed 10k retained events with the real 1k/area cap.
use super::{distribution::Distribution, os};
use mg_live::{events::Query, store::LiveStore};
use serde_json::{Value, json};
use std::{path::Path, time::Instant};

fn query(store: &LiveStore, q: &Query) -> Value {
    let mut times = Distribution::default();
    let mut returned = 0;
    for _ in 0..100 {
        let start = Instant::now();
        returned = store.events(q).0.len();
        times.add(start.elapsed().as_secs_f64() * 1000.);
    }
    json!({"timing":times.json(),"returned":returned})
}
pub(super) fn run(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    let store = LiveStore::default();
    let module = root.join("synthetic.mod");
    store.assistant_context(root.into(), module.clone(), "synthetic-only".into()).unwrap();
    let areas: Vec<_> = (0..12)
        .map(|a| {
            let mut creatures = vec![json!({"id":format!("{:x}",4096+a),"name":format!("Player surrogate {a}"),"pc":true})];
            creatures.extend((0..5).map(|n| json!({"id":format!("{:x}",16+a*5+n),"tag":format!("npc_{a}_{n}"),"name":format!("NPC {a}/{n}")})));
            json!({"area":format!("control{a}"),"full":true,"cre":creatures})
        })
        .collect();
    store
        .sync(
            &serde_json::to_vec(&json!({"v":1,"boot":"synthetic","seq":1,"areas":areas})).unwrap(),
            1,
        )
        .unwrap();
    for a in 0..12 {
        for n in 0..5 {
            let id = format!("{:x}", 16 + a * 5 + n);
            let mut card = store.npc_card(&id).unwrap();
            card.who = "Synthetic memory/query control, never a connected player proof".into();
            store.save_npc_card(&id, card).unwrap();
            store.save_object_note(&id, "Synthetic note").unwrap();
        }
    }
    let mut samples = Vec::new();
    let mut ingest = Distribution::default();
    for batch in 0..300 {
        let events: Vec<_> = (0..100).map(|i| {
            let n = batch * 100 + i;
            let a = n % 12;
            let npc = format!("{:x}",16+a*5+(n / 12)%5);
            let pc = format!("{:x}",4096+a);
            let (id, target) = if (n / 60) % 2 == 0 {(npc, pc)} else {(pc, npc)};
            json!({"seq":n+1,"kind":"chat","area":format!("control{a}"),"id":id,"target":target,"channel":1,"text":format!("Synthetic stress conversation turn {n}")})
        }).collect();
        let body = serde_json::to_vec(
            &json!({"v":1,"boot":"synthetic","seq":batch+2,"areas":[],"events":events}),
        )
        .unwrap();
        let start = Instant::now();
        store.sync(&body, (batch + 2) as u64).unwrap();
        ingest.add(start.elapsed().as_secs_f64() * 1000.);
        if (batch + 1) % 100 == 0 {
            store.flush_npc_memory().unwrap();
            let census = store.measure_census();
            assert!(census["events"]["retained"].as_u64().unwrap() <= 12000);
            assert_eq!(census["assistant"]["knowledge"]["memory"]["turns"], 10000);
            samples.push(json!({"processed":(batch+1)*100,"census":census,"memory":os::memory(),
                "files":super::stress::tree_bytes(root),
                "all":query(&store,&Query::default()),
                "tail":query(&store,&Query{since:store.event_cursor().saturating_sub(8),..Default::default()}),
                "tail_text":query(&store,&Query{since:store.event_cursor().saturating_sub(8),text:Some("stress".into()),..Default::default()}),
                "empty_tail":query(&store,&Query{since:store.event_cursor(),..Default::default()}),
                "chat":query(&store,&Query{kinds:vec!["chat".into()],..Default::default()}),
                "text":query(&store,&Query{text:Some("stress".into()),..Default::default()}),
                "area":query(&store,&Query{area:Some("control0".into()),..Default::default()}),
                "object":query(&store,&Query{object:Some("10".into()),..Default::default()}),
                "missing_object":query(&store,&Query{object:Some("ffff".into()),..Default::default()})}));
        }
    }
    assert_eq!(samples.last().unwrap()["object"]["returned"], 200);
    assert_eq!(samples.last().unwrap()["missing_object"]["returned"], 0);
    store.clear_assistant_context();
    let start = Instant::now();
    store.assistant_context(root.into(), module, "synthetic-only".into()).unwrap();
    let restore_ms = start.elapsed().as_secs_f64() * 1000.;
    let restored = store.measure_census();
    assert_eq!(restored["assistant"]["knowledge"]["memory"]["turns"], 10000);
    std::fs::write(root.join("result.json"), serde_json::to_vec_pretty(&json!({
        "method":"Separate synthetic CPU/store control: 12 areas, 60 NPCs, 12 fake PCs, 30k directed turns. No native client, GUI or network. Production retention limits unchanged.",
        "samples":samples,"ingest_100_events_ms":ingest.json(),"restore_ms":restore_ms,"restored":restored
    })).unwrap()).unwrap();
}
