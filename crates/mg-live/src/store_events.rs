//! Event wake-ups and chat context share the store lock with ingestion.
use super::{LiveStore, State};
use crate::{
    access::{ClientGroup, Scope},
    events::{Event, Query},
};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct ChatFilter {
    pub players: bool,
    pub to_dm: Option<bool>,
    pub boot: Option<String>,
}
#[derive(serde::Serialize)]
pub(crate) struct EventPage {
    pub events: Vec<Event>,
    pub last: u64,
    pub boot: String,
    pub reset: bool,
    pub lost: bool,
}
fn query(s: &State, q: &Query, scope: &Scope, filter: &ChatFilter) -> EventPage {
    let (events, lost) = s.events.query_page(q, |e| {
        if matches!(e.kind.as_str(), "chat" | "dialog" | "server_log")
            && !scope.has(ClientGroup::ReadChat)
        {
            return false;
        }
        if e.kind == "command" && !scope.unrestricted() {
            return e.data["id"].as_str().is_some_and(|id| s.owners.get(id) == Some(&scope.name))
                && !filter.players
                && filter.to_dm.is_none();
        }
        if e.kind == "skill_roll"
            && !scope.unrestricted()
            && !e.data["request"].as_str().is_some_and(|id| s.owners.get(id) == Some(&scope.name))
        {
            return false;
        }
        (!filter.players
            || (e.kind == "chat"
                && e.data["silent"] != true
                && (e.data["pc"] == true || e.data["possessed"] == true)))
            && filter.to_dm.is_none_or(|wanted| {
                e.kind == "chat" && e.data["to_dm"].as_bool().unwrap_or(false) == wanted
            })
    });
    EventPage {
        events,
        last: s.events.last(),
        boot: s.boot.clone(),
        reset: q.since > s.events.last() || filter.boot.as_ref().is_some_and(|b| *b != s.boot),
        lost,
    }
}
impl LiveStore {
    pub(crate) fn log_rows(&self, since: u64) -> (String, Vec<Event>, u64) {
        let s = self.lock();
        (s.boot.clone(), s.events.log_rows(since), s.events.last())
    }
    /// Stream events are independent of state baseline/boot. Historical boots
    /// can drain while the latest state is already from a newer game process.
    pub(crate) fn stream_event(
        &self,
        entry: &crate::bridge::event_stream::Entry,
        now_ms: u64,
    ) -> Result<(), String> {
        if self.is_mirror() {
            return Err("A mirror cannot ingest game events".into());
        }
        if entry.lost_before > 0 {
            self.stream_overflow(entry.lost_before, now_ms);
        }
        if entry.reset_before {
            self.stream_reset(now_ms);
        }
        if entry.kind == "log" {
            let mut message = entry.data.clone();
            message["queue_epoch"] = json!(entry.epoch);
            message["queue_n"] = json!(entry.n);
            self.server_log(&message, now_ms);
            return Ok(());
        }
        let mut packet = json!({"text_codepage":entry.codepage,"events":[entry.data.clone()]});
        packet["events"][0]["boot"] = json!(entry.source);
        packet["events"][0]["stream_id"] = json!(entry.id);
        packet["events"][0]["queue_epoch"] = json!(entry.epoch);
        packet["events"][0]["queue_n"] = json!(entry.n);
        crate::wire_text::report(&mut packet);
        let mut s = self.lock();
        let rows = packet["events"].as_array().ok_or("Invalid stream packet")?;
        // Historical IDs may refer to different objects in the current boot.
        let events = if s.boot == entry.source { enrich(&s, rows, now_ms) } else { rows.clone() };
        let dropped = s.events.dropped;
        s.events.game(&entry.source, &events, dropped, now_ms);
        super::assistant::capture(&mut s);
        Ok(())
    }

    pub(crate) fn stream_overflow(&self, lost: u64, now_ms: u64) {
        if self.is_mirror() {
            return;
        }
        self.lock().events.transport_notice(
            "queue_overflow",
            &format!("{lost} events lost: queue overflow"),
            lost,
            now_ms,
        );
    }
    pub(crate) fn stream_reset(&self, now_ms: u64) {
        if self.is_mirror() {
            return;
        }
        self.lock().events.transport_notice("queue_reset", "Event queue was reset: Redis persistence was lost; an unknown number of events may be missing", 0, now_ms);
    }
    pub fn events(&self, q: &Query) -> (Vec<Event>, u64) {
        self.events_scoped(q, &Scope::default())
    }
    pub fn events_scoped(&self, q: &Query, scope: &Scope) -> (Vec<Event>, u64) {
        let page = query(&self.lock(), q, scope, &ChatFilter::default());
        (page.events, page.last)
    }
    pub(crate) fn wait_events(
        &self,
        q: &Query,
        scope: &Scope,
        filter: &ChatFilter,
        wait_ms: u64,
        stop: &AtomicBool,
    ) -> Result<EventPage, String> {
        let deadline = Instant::now() + Duration::from_millis(wait_ms.min(30000));
        let mut s = self.lock();
        let signal = s.events.changed.clone();
        loop {
            if stop.load(Ordering::Acquire) {
                return Err("Live service stopped".into());
            }
            let found = query(&s, q, scope, filter);
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !found.events.is_empty() || found.reset || found.lost || remaining.is_zero() {
                return Ok(found);
            }
            // Holding the ingestion mutex across check -> wait avoids lost wakeups.
            s = signal.wait_timeout(s, remaining).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
    pub(crate) fn wake_event_waiters(&self) {
        self.lock().events.changed.notify_all();
    }

    /// Fast events never refresh state age, advance report seq or acknowledge commands.
    pub(crate) fn event_packet(&self, body: &[u8], now_ms: u64) -> Result<bool, String> {
        if self.is_mirror() {
            return Err("A mirror cannot ingest game events".into());
        }
        if body.len() > 65536 {
            return Err("Event packet too large".into());
        }
        let mut packet: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        if packet["event_stream"] == true {
            return Ok(false);
        }
        crate::wire_text::report(&mut packet);
        let boot = packet["boot"]
            .as_str()
            .filter(|b| !b.is_empty() && b.len() <= 64)
            .ok_or("Invalid event boot")?;
        let events = packet["events"]
            .as_array()
            .filter(|e| e.len() == 1)
            .ok_or("One fast event required")?;
        if packet["v"] != 1
            || events[0]["kind"] != "chat"
            || !events[0]["seq"].as_i64().is_some_and(|n| n > 0)
        {
            return Err("Invalid fast chat packet".into());
        }
        let mut s = self.lock();
        if s.boot != boot || !s.has_baseline {
            return Ok(false);
        }
        let events = enrich(&s, events, now_ms);
        let old = s.events.last();
        let dropped = s.events.dropped;
        s.events.game(boot, &events, dropped, now_ms);
        super::assistant::capture(&mut s);
        Ok(old != s.events.last())
    }
}

pub(super) fn enrich(s: &State, events: &[Value], now_ms: u64) -> Vec<Value> {
    events.iter().map(|event| {
        let mut e=event.clone();
        let command=e["command_id"].as_str().or_else(||e["request"].as_str());
        if let Some(by)=command.and_then(|id|s.owners.get(id)).filter(|by|by.as_str()!="local") { e["by"]=json!(by); }
        if e["kind"]!="chat" { return e; }
        let area=e["area"].as_str().and_then(|a|s.areas.get(&a.to_ascii_lowercase()));
        let speaker=area.and_then(|a|a.creatures.iter().find(|o|e["id"]==o.id));
        if e.get("pc").is_none() { e["pc"]=json!(speaker.is_some_and(|o|o.pc)); }
        if e.get("possessed").is_none() { e["possessed"]=json!(speaker.is_some_and(|o|o.possessed)); }
        if e["silent"]==true || (e["pc"]!=true && e["possessed"]!=true) { return e; }
        let position= match (e["position"]["x"].as_f64(),e["position"]["y"].as_f64(),e["position"]["z"].as_f64()) {
            (Some(x),Some(y),Some(z))=>Some([x,y,z]),
            _=>speaker.map(|o|[o.x.into(),o.y.into(),o.z.into()]),
        };
        let mut nearby=Vec::new();
        if let (Some(a),Some([x,y,z]))=(area,position) {
            for o in &a.creatures {
                if e["id"]==o.id { continue; }
                let distance=((f64::from(o.x)-x).powi(2)+(f64::from(o.y)-y).powi(2)+(f64::from(o.z)-z).powi(2)).sqrt();
                if distance<=20.0 { nearby.push((distance,o)); }
            }
        }
        nearby.sort_by(|(a,x),(b,y)|a.total_cmp(b).then_with(||x.id.cmp(&y.id)));
        e["nearby"]=json!(nearby.into_iter().take(5).map(|(distance,o)|json!({"id":o.id,"name":o.name,"distance":distance,"pc":o.pc})).collect::<Vec<_>>());
        e["nearby_age_ms"]=json!(area.map(|a|now_ms.saturating_sub(a.received_ms)));
        e
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, thread};
    fn store() -> Arc<LiveStore> {
        let s = Arc::new(LiveStore::default());
        let rows:Vec<_>=(1..=8).map(|n|json!({"id":format!("{n:x}"),"name":format!("NPC {n}"),"pc":n==1,"x":n,"y":0,"z":0})).collect();
        s.sync(json!({"v":1,"boot":"b","seq":1,"ack":[],"areas":[{"area":"room","full":true,"cre":rows}]}).to_string().as_bytes(),1000).unwrap();
        s
    }
    #[test]
    fn agent_identity_follows_exact_command_not_speaker_or_payload() {
        use crate::access::{Access, ClientToken, DeliveryMode};
        let s = store();
        let mut client = ClientToken::conversation("Storyteller", "c".repeat(32));
        client.delivery = DeliveryMode::Auto;
        let access = Access::new("d".repeat(32), &[client]).unwrap();
        let scope = access.scope_named("Storyteller").unwrap();
        let command: crate::store::Command = serde_json::from_value(
            json!({"id":"speak","area":"room","op":"npc_say","target":"2","text":"hello"}),
        )
        .unwrap();
        let id = s.enqueue_scoped(command, 1001, &scope).unwrap();
        let wire = s.sync(br#"{"v":1,"boot":"b","seq":2,"areas":[]}"#, 1002).unwrap();
        assert_eq!(wire["commands"][0]["by"], "Storyteller");
        s.event_packet(json!({"v":1,"boot":"b","events":[{"seq":1,"kind":"chat","id":"2","text":"hello","command_id":id,"by":"spoof"}]}).to_string().as_bytes(),1003).unwrap();
        s.event_packet(
            br#"{"v":1,"boot":"b","events":[{"seq":2,"kind":"chat","id":"2","text":"hello"}]}"#,
            1004,
        )
        .unwrap();
        let (events, _) = s.events(&Query::default());
        assert_eq!(events[0].data["by"], "Storyteller");
        assert!(events[1].data.get("by").is_none());
        s.sync(br#"{"v":1,"boot":"b","seq":3,"ack":["speak"],"results":[{"id":"speak","ok":true,"by":"spoof"}],"areas":[]}"#,1005).unwrap();
        assert_eq!(s.result("speak").unwrap().by.as_deref(), Some("Storyteller"));
        // Speech scope never supplies either consent switch.
        let reply: crate::store::Command =
            serde_json::from_value(json!({"op":"reply_to_player","player":"1","text":"hello"}))
                .unwrap();
        assert_eq!(
            s.enqueue_scoped(reply, 1006, &scope).unwrap_err().code,
            crate::store::ErrorCode::Disabled
        );
    }

    fn packet(s: &LiveStore, seq: i32, pc: bool, to_dm: bool) {
        s.event_packet(json!({"v":1,"boot":"b","events":[{"seq":seq,"kind":"chat","area":"room","id":"1","pc":pc,"to_dm":to_dm,"text":"hello","position":{"x":1,"y":0,"z":0}}]}).to_string().as_bytes(),1100).unwrap();
    }
    #[test]
    fn fast_events_deduplicate_without_losing_overtaken_events_or_refreshing_state() {
        let s = store();
        packet(&s, 3, true, true);
        packet(&s, 3, true, true);
        s.sync(br#"{"v":1,"boot":"b","seq":2,"ack":[],"events":[{"seq":2,"kind":"attack"},{"seq":3,"kind":"chat"}],"areas":[]}"#,1200).unwrap();
        let (events, _) = s.events(&Query::default());
        assert_eq!(
            events.iter().map(|e| e.data["game_seq"].as_i64().unwrap()).collect::<Vec<_>>(),
            [3, 2]
        );
        let before = s.status(1300);
        packet(&s, 4, true, false);
        assert_eq!(before["seq"], s.status(1400)["seq"]);
        assert_eq!(s.lock().received_ms, 1200);
        assert!(
            !s.event_packet(br#"{"v":1,"boot":"old","events":[{"seq":5,"kind":"chat"}]}"#, 1500)
                .unwrap()
        );
        assert!(
            s.event_packet(br#"{"v":1,"boot":"b","events":[{"seq":5,"kind":"damage"}]}"#, 1500)
                .is_err()
        );
    }
    #[test]
    fn player_chat_has_five_nearest_creatures_and_native_byte_decoding() {
        let s = store();
        s.event_packet(br#"{"v":1,"boot":"b","events":[{"seq":1,"kind":"chat","id":"1","area":"room","text_bytes":"ju%C5%BC","text":"broken"}]}"#,1100).unwrap();
        let e = &s.events(&Query::default()).0[0];
        assert_eq!(e.data["text"], "już");
        assert_eq!(e.data["pc"], true);
        assert_eq!(e.data["nearby_age_ms"], 100);
        let rows = e.data["nearby"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0], json!({"id":"2","name":"NPC 2","distance":1.0,"pc":false}));
        assert_eq!(rows[4]["id"], "6");
    }
    #[test]
    fn long_poll_waits_for_match_times_out_and_wakes_on_shutdown() {
        let s = store();
        let stop = Arc::new(AtomicBool::new(false));
        let (reader, cancel) = (s.clone(), stop.clone());
        let worker = thread::spawn(move || {
            reader
                .wait_events(
                    &Query::default(),
                    &Scope::default(),
                    &ChatFilter { players: true, to_dm: Some(true), ..Default::default() },
                    2000,
                    &cancel,
                )
                .unwrap()
        });
        packet(&s, 1, false, true);
        packet(&s, 2, true, false);
        thread::sleep(Duration::from_millis(25));
        assert!(!worker.is_finished());
        packet(&s, 3, true, true);
        assert_eq!(worker.join().unwrap().events[0].data["game_seq"], 3);
        let q = Query { since: 3, ..Default::default() };
        let start = Instant::now();
        assert!(
            s.wait_events(&q, &Scope::default(), &ChatFilter::default(), 30, &stop)
                .unwrap()
                .events
                .is_empty()
        );
        assert!(start.elapsed() >= Duration::from_millis(25));
        let (reader, cancel) = (s.clone(), stop.clone());
        let worker = thread::spawn(move || {
            reader.wait_events(&q, &Scope::default(), &ChatFilter::default(), 30000, &cancel)
        });
        stop.store(true, Ordering::Release);
        s.wake_event_waiters();
        assert!(worker.join().unwrap().is_err());
    }
    #[test]
    fn players_filter_includes_possession_but_never_ai_shouts() {
        let s = store();
        for (seq, possessed, silent) in [(1, true, false), (2, true, true), (3, false, false)] {
            s.event_packet(json!({"v":1,"boot":"b","events":[{"seq":seq,"kind":"chat","id":"2","area":"room","pc":false,"possessed":possessed,"silent":silent}]}).to_string().as_bytes(),1100).unwrap();
        }
        let page = s
            .wait_events(
                &Query::default(),
                &Scope::default(),
                &ChatFilter { players: true, ..Default::default() },
                0,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(page.last, 3);
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].data["game_seq"], 1);
    }

    #[test]
    fn watch_reports_reset_eviction_and_limit_loss_without_waiting() {
        let s = store();
        for n in 1..=1002 {
            packet(&s, n, true, false);
        }
        let read = |q: &Query, filter: &ChatFilter| {
            s.wait_events(q, &Scope::default(), filter, 30000, &AtomicBool::new(false)).unwrap()
        };
        let page = read(&Query { since: 1003, ..Default::default() }, &ChatFilter::default());
        assert!(page.reset);
        assert_eq!(page.last, 1002);
        assert_eq!(page.boot, "b");
        let page = read(
            &Query { since: 1002, ..Default::default() },
            &ChatFilter { boot: Some("old".into()), ..Default::default() },
        );
        assert!(page.reset);
        let page =
            read(&Query { since: 1, limit: 500, ..Default::default() }, &ChatFilter::default());
        assert!(page.lost);
        assert_eq!(page.events.len(), 500);
        let page =
            read(&Query { since: 1000, limit: 1, ..Default::default() }, &ChatFilter::default());
        assert!(page.lost);
        assert_eq!(page.events[0].n, 1002);
    }
    #[test]
    fn an_empty_new_boot_wakes_a_watch() {
        let s = store();
        let reader = s.clone();
        let worker = thread::spawn(move || {
            reader
                .wait_events(
                    &Query::default(),
                    &Scope::default(),
                    &ChatFilter { boot: Some("b".into()), ..Default::default() },
                    30000,
                    &AtomicBool::new(false),
                )
                .unwrap()
        });
        thread::sleep(Duration::from_millis(20));
        let start = Instant::now();
        s.sync(br#"{"v":1,"boot":"new","seq":1,"areas":[]}"#, 1200).unwrap();
        assert!(worker.join().unwrap().reset);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
