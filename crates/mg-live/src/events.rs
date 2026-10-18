//! The running game's events, kept for the Live Log and for agents: what
//! the game reports (`events` in a state report: chat, attacks, damage,
//! deaths, spells, players, projectiles; `docs/live-events-wire.md`), the
//! server log (`<prefix>.log`), the game's answers to commands, and what
//! Moonglow sees change between two reports of an area (creatures entering
//! and leaving, combat starting and ending, a new attack target, healing).
//!
//! Each area keeps its last [`PER_AREA`] events (the server log and events
//! without an area are kept under `""`). Every event gets Moonglow's own
//! number, `n`, rising across areas, so a reader asks for what is new since
//! the last it saw.

use crate::store::LiveObject;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeSet, BinaryHeap, HashMap, VecDeque};
use std::sync::{Arc, Condvar};

pub const PER_AREA: usize = 1000;
/// At most this many events answer one query.
pub const MAX_QUERY: usize = 500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Moonglow's number for it.
    pub n: u64,
    /// When Moonglow received it.
    pub at_ms: u64,
    pub kind: String,
    /// The area's resref; `""` for the server log and events without one.
    pub area: String,
    pub id: Option<String>,
    pub name: String,
    pub target: Option<String>,
    /// The rest of what it says (the game's own fields as sent).
    pub data: Value,
}

/// What a reader asks for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    /// Only events numbered above this.
    pub since: u64,
    /// One area (`Some("")`: the server log and events without an area).
    pub area: Option<String>,
    /// These kinds only (none: every kind).
    pub kinds: Vec<String>,
    /// Events of this object, as their source or their target.
    pub object: Option<String>,
    /// Text found (any case) in the name or what the event says.
    pub text: Option<String>,
    /// The last this many that match (0: [`MAX_QUERY`]).
    pub limit: usize,
}

#[derive(Debug, Default)]
pub struct EventLog {
    next: u64,
    areas: HashMap<String, VecDeque<Event>>,
    evicted: HashMap<String, u64>,
    /// Fast chat can overtake buffered events. Keep a bounded exact receipt set.
    game: Option<(String, i64)>,
    seen: BTreeSet<i64>,
    pub(crate) changed: Arc<Condvar>,
    /// The server log's last line taken: its stream and number.
    log: Option<(String, i64)>,
    /// How many events the game dropped before sending (its count).
    pub dropped: i64,
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

impl EventLog {
    /// Import finished gateway events without deriving or renumbering anything.
    pub(crate) fn mirror_batch(&mut self, rows: Vec<Event>, through: u64) -> Result<(), String> {
        if rows.windows(2).any(|pair| pair[0].n >= pair[1].n)
            || rows.iter().any(|row| row.n == 0 || row.n > through)
        {
            return Err("Invalid mirror event cursor order".into());
        }
        for row in rows.into_iter().filter(|row| row.n > self.next) {
            let ring = self.areas.entry(row.area.clone()).or_default();
            if ring.len() >= PER_AREA
                && let Some(old) = ring.pop_front()
            {
                self.evicted.insert(row.area.clone(), old.n);
            }
            ring.push_back(row);
        }
        self.next = self.next.max(through);
        self.changed.notify_all();
        Ok(())
    }
    pub(crate) fn mirror_clear(&mut self) {
        let changed = self.changed.clone();
        *self = Self { changed, ..Self::default() };
        self.changed.notify_all();
    }

    pub(crate) fn transport_notice(&mut self, kind: &str, line: &str, lost: u64, now_ms: u64) {
        self.push(Event {
            n: 0,
            at_ms: now_ms,
            kind: "server_log".into(),
            area: String::new(),
            id: None,
            name: String::new(),
            target: None,
            data: json!({"line":line,"transport":kind,"lost":lost,"script_error":false}),
        });
    }
    #[cfg(feature = "measure")]
    pub(crate) fn measure_census(&self) -> Value {
        json!({"retained":self.areas.values().map(VecDeque::len).sum::<usize>(),"last":self.next,"seen_game_sequences":self.seen.len(),
            "per_area":self.areas.iter().map(|(a,r)|(a,r.len())).collect::<std::collections::BTreeMap<_,_>>()})
    }
    fn push(&mut self, mut e: Event) {
        self.next += 1;
        e.n = self.next;
        let ring = self.areas.entry(e.area.clone()).or_default();
        if ring.len() >= PER_AREA
            && let Some(old) = ring.pop_front()
        {
            self.evicted.insert(e.area.clone(), old.n);
        }
        ring.push_back(e);
        self.changed.notify_all();
    }

    /// The newest event's number (0: none yet).
    pub fn last(&self) -> u64 {
        self.next
    }

    /// The local disk sink drains every retained row; the API's newest-page
    /// limit must not silently drop a reconnect burst from the log writer.
    pub(crate) fn log_rows(&self, since: u64) -> Vec<Event> {
        let mut rows: Vec<_> = self
            .areas
            .values()
            .flat_map(|r| r.iter().rev().take_while(|e| e.n > since))
            .cloned()
            .collect();
        rows.sort_unstable_by_key(|e| e.n);
        rows
    }

    /// A state report's `events` and `dropped`, each taken once (by its
    /// boot and number: a report resent, or an event in two reports, is
    /// not taken again).
    pub fn game(&mut self, boot: &str, events: &[Value], dropped: i64, now_ms: u64) {
        self.dropped = dropped;
        for v in events {
            let Some(seq) = v["seq"].as_i64() else { continue };
            if seq < 1 {
                continue;
            }
            if self.game.as_ref().is_none_or(|(b, _)| b != boot) {
                self.game = Some((boot.to_string(), 0));
                self.seen.clear();
            }
            let highest = &mut self.game.as_mut().unwrap().1;
            if seq <= highest.saturating_sub(4096) || !self.seen.insert(seq) {
                continue;
            }
            *highest = (*highest).max(seq);
            let floor = highest.saturating_sub(4096);
            while self.seen.first().is_some_and(|n| *n <= floor) {
                self.seen.pop_first();
            }
            let mut data = v.clone();
            if let Some(o) = data.as_object_mut() {
                for key in ["seq", "kind", "id", "name", "area", "target"] {
                    o.remove(key);
                }
                o.insert("game_seq".into(), json!(seq));
            }
            self.push(Event {
                n: 0,
                at_ms: now_ms,
                kind: text(&v["kind"]).unwrap_or_else(|| "unknown".into()),
                area: text(&v["area"]).unwrap_or_default().to_ascii_lowercase(),
                id: text(&v["id"]),
                name: text(&v["name"]).unwrap_or_default(),
                target: text(&v["target"]),
                data,
            });
        }
    }

    /// A message of `<prefix>.log` (`kind: server_log`), taken once (by its
    /// stream and number). Anything else is ignored.
    pub fn server_log(&mut self, message: &Value, now_ms: u64) {
        let (Some(stream), Some(seq), Some(line)) =
            (message["stream"].as_str(), message["seq"].as_i64(), message["line"].as_str())
        else {
            return;
        };
        if message["kind"] != "server_log"
            || self.log.as_ref().is_some_and(|(s, n)| s == stream && seq <= *n)
        {
            return;
        }
        self.log = Some((stream.to_string(), seq));
        let mut data = json!({"line": line, "script_error": message["script_error"] == true,
                             "time": message["time"]});
        for field in ["boot", "queue_epoch", "queue_n"] {
            if let Some(value) = message.get(field).filter(|v| !v.is_null()) {
                data[field] = value.clone();
            }
        }
        self.push(Event {
            n: 0,
            at_ms: now_ms,
            kind: "server_log".into(),
            area: String::new(),
            id: None,
            name: String::new(),
            target: None,
            data,
        });
    }

    /// The game's answer to a command sent to `area`.
    pub fn command(&mut self, area: &str, result: &Value, now_ms: u64) {
        let mut data = result.clone();
        if let Some(snapshot) = data.as_object_mut().and_then(|o| o.remove("object_json")) {
            data["capture_bytes"] = json!(snapshot.to_string().len());
        }
        self.push(Event {
            n: 0,
            at_ms: now_ms,
            kind: "command".into(),
            area: area.to_ascii_lowercase(),
            id: text(&result["obj"]).filter(|s| !s.is_empty()),
            name: String::new(),
            target: None,
            data,
        });
    }

    /// What changed among an area's creatures between its last report
    /// (`None`: the first seen, which says nothing new) and this one.
    pub fn derive(
        &mut self,
        area: &str,
        old: Option<&[LiveObject]>,
        new: &[LiveObject],
        now_ms: u64,
    ) {
        self.derive_iter(area, old, new.iter(), now_ms);
    }

    pub(crate) fn derive_iter<'a>(
        &mut self,
        area: &str,
        old: Option<&[LiveObject]>,
        new: impl Iterator<Item = &'a LiveObject> + Clone,
        now_ms: u64,
    ) {
        let Some(old) = old else { return };
        let area = area.to_ascii_lowercase();
        let before: HashMap<&str, &LiveObject> = old.iter().map(|o| (o.id.as_str(), o)).collect();
        let now: HashMap<&str, &LiveObject> = new.clone().map(|o| (o.id.as_str(), o)).collect();
        let mut out = Vec::new();
        let event = |kind: &str, o: &LiveObject, target: Option<String>, data: Value| Event {
            n: 0,
            at_ms: now_ms,
            kind: kind.into(),
            area: area.clone(),
            id: Some(o.id.clone()),
            name: o.name.clone(),
            target,
            data,
        };
        for o in new {
            let Some(was) = before.get(o.id.as_str()) else {
                out.push(event("entered", o, None, json!({"r": o.resref, "pc": o.pc})));
                continue;
            };
            if o.combat != was.combat {
                let kind = if o.combat { "combat_start" } else { "combat_end" };
                out.push(event(kind, o, o.target.clone(), json!({})));
            }
            if o.target.is_some() && o.target != was.target {
                out.push(event("target", o, o.target.clone(), json!({})));
            }
            if let (Some([to, max]), Some([from, _])) = (o.hp, was.hp)
                && to > from
                && !o.dead
                && !was.dead
            {
                out.push(event("healed", o, None, json!({"from": from, "to": to, "max": max})));
            }
        }
        for o in old.iter().filter(|o| !now.contains_key(o.id.as_str())) {
            out.push(event("left", o, None, json!({"r": o.resref, "pc": o.pc})));
        }
        for e in out {
            self.push(e);
        }
    }

    /// The events matching `q`, oldest first.
    pub fn query(&self, q: &Query) -> Vec<Event> {
        self.query_filtered(q, |_| true)
    }
    pub(crate) fn query_filtered(&self, q: &Query, visible: impl Fn(&Event) -> bool) -> Vec<Event> {
        self.query_page(q, visible).0
    }
    pub(crate) fn query_page(
        &self,
        q: &Query,
        visible: impl Fn(&Event) -> bool,
    ) -> (Vec<Event>, bool) {
        if q.since >= self.next {
            return (Vec::new(), false);
        }
        let needle = q.text.as_ref().map(|t| t.to_lowercase()).filter(|t| !t.is_empty());
        let wanted = |e: &&Event| {
            visible(e)
                && (q.kinds.is_empty() || q.kinds.contains(&e.kind))
                && q.object.as_ref().is_none_or(|o| {
                    e.id.as_deref() == Some(o.as_str()) || e.target.as_deref() == Some(o.as_str())
                })
                && needle.as_ref().is_none_or(|t| {
                    e.name.to_lowercase().contains(t)
                        || e.data.to_string().to_lowercase().contains(t)
                })
        };
        let rings: Vec<_> = match &q.area {
            Some(area) => self.areas.get(&area.to_ascii_lowercase()).into_iter().collect(),
            None => self.areas.values().collect(),
        };
        // Each ring is ordered. Merge its newest matching rows and stop after
        // one bounded page plus a lookahead for truncation. Filtering before
        // the merge also avoids a heap operation for every nonmatching row.
        let mut streams: Vec<_> = rings
            .into_iter()
            .map(|ring| ring.iter().rev().take_while(|e| e.n > q.since).filter(&wanted).peekable())
            .collect();
        let mut heads: BinaryHeap<_> = streams
            .iter_mut()
            .enumerate()
            .filter_map(|(i, stream)| stream.peek().map(|e| (e.n, i)))
            .collect();
        let limit = if q.limit == 0 { MAX_QUERY } else { q.limit.min(MAX_QUERY) };
        let mut found = Vec::new();
        while let Some((_, i)) = heads.pop() {
            found.push(streams[i].next().expect("head came from this stream"));
            if found.len() > limit {
                break;
            }
            if let Some(next) = streams[i].peek() {
                heads.push((next.n, i));
            }
        }
        let more = found.len() > limit;
        if more {
            found.pop();
        }
        found.reverse();
        // Eviction is conservative: an evicted row may have matched filters.
        let evicted = match &q.area {
            Some(a) => self.evicted.get(&a.to_ascii_lowercase()).is_some_and(|n| *n > q.since),
            None => self.evicted.values().any(|n| *n > q.since),
        };
        (found.into_iter().cloned().collect(), more || evicted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creature(id: &str, combat: bool, target: Option<&str>, hp: i32) -> LiveObject {
        LiveObject {
            id: id.into(),
            name: format!("C{id}"),
            resref: "nw_bandit001".into(),
            combat,
            target: target.map(str::to_string),
            hp: Some([hp, 10]),
            ..Default::default()
        }
    }

    #[test]
    fn game_events_are_taken_once_per_boot_and_number() {
        let mut log = EventLog::default();
        let chat = json!([
            {"seq":1,"kind":"chat","time":"1.5","id":"2","name":"Bandit","area":"Events","target":null,"channel":1,"text":"Hello"},
            {"seq":2,"kind":"death","time":"1.6","id":"5","name":"Bartender","area":"events","target":null,"hp":-11,"killer":"2"}]);
        let chat = chat.as_array().unwrap();
        log.game("boot-a", chat, 2, 10);
        log.game("boot-a", chat, 2, 11);
        assert_eq!(log.dropped, 2);
        let all = log.query(&Query::default());
        assert_eq!(all.len(), 2, "a report resent is not taken again");
        assert_eq!((all[0].kind.as_str(), all[0].area.as_str()), ("chat", "events"));
        assert_eq!(all[0].data, json!({"time":"1.5","channel":1,"text":"Hello","game_seq":1}));
        assert_eq!(all[1].data["killer"], "2");
        // A new boot numbers from 1 again.
        log.game("boot-b", &[json!({"seq":1,"kind":"chat","text":"again"})], 0, 12);
        let last = log.query(&Query { since: 2, ..Default::default() });
        assert_eq!((last.len(), last[0].area.as_str(), last[0].n), (1, "", 3));
    }

    #[test]
    fn changes_between_reports_are_events() {
        let mut log = EventLog::default();
        let first = [creature("1", false, None, 10), creature("2", false, None, 4)];
        log.derive("room", None, &first, 1);
        assert_eq!(log.last(), 0, "the first report says nothing new");
        let second = [creature("2", true, Some("3"), 9), creature("3", false, None, 10)];
        log.derive("room", Some(&first), &second, 2);
        let kinds: Vec<_> =
            log.query(&Query::default()).into_iter().map(|e| (e.kind, e.id)).collect();
        let some = |s: &str| Some(s.to_string());
        assert_eq!(
            kinds,
            [
                ("combat_start".to_string(), some("2")),
                ("target".to_string(), some("2")),
                ("healed".to_string(), some("2")),
                ("entered".to_string(), some("3")),
                ("left".to_string(), some("1")),
            ]
        );
        let third = [creature("2", false, Some("3"), 9), creature("3", false, None, 10)];
        log.derive("room", Some(&second), &third, 3);
        let end = log.query(&Query { since: 5, ..Default::default() });
        assert_eq!(end.len(), 1, "the same target is no new event");
        assert_eq!(end[0].kind, "combat_end");
    }

    #[test]
    fn queries_filter_and_areas_keep_their_last_thousand() {
        let mut log = EventLog::default();
        let events: Vec<Value> = (1..=1005)
            .map(|i| json!({"seq":i,"kind":if i % 2 == 0 {"chat"} else {"attack"},"area":"a","id":"7","target":"8","text":format!("line {i}")}))
            .collect();
        log.game("b", &events, 0, 1);
        log.server_log(&json!({"v":1,"kind":"server_log","stream":"s","seq":1,"line":"Script error: x","script_error":true}), 2);
        log.server_log(
            &json!({"v":1,"kind":"server_log","stream":"s","seq":1,"line":"Script error: x"}),
            3,
        );
        log.command("A", &json!({"id":"c1","ok":false,"err":"target is dead","code":"dead"}), 4);
        assert_eq!(
            log.query(&Query { area: Some("a".into()), limit: 2000, ..Default::default() }).len(),
            500
        );
        assert_eq!(log.areas["a"].len(), PER_AREA, "an area keeps its last thousand");
        let chat = log.query(&Query { kinds: vec!["chat".into()], limit: 3, ..Default::default() });
        assert_eq!(
            chat.iter().map(|e| e.data["text"].as_str().unwrap()).collect::<Vec<_>>(),
            ["line 1000", "line 1002", "line 1004"]
        );
        let errors = log.query(&Query { area: Some(String::new()), ..Default::default() });
        assert_eq!(errors.len(), 1, "a log line resent is taken once");
        assert_eq!(errors[0].data["script_error"], true);
        assert_eq!(
            log.query(&Query { text: Some("TARGET IS DEAD".into()), ..Default::default() })[0].kind,
            "command"
        );
        assert_eq!(
            log.query(&Query { object: Some("8".into()), limit: 1, ..Default::default() })[0].data
                ["text"],
            "line 1005"
        );
        assert!(log.query(&Query { object: Some("9".into()), ..Default::default() }).is_empty());
    }
}

#[cfg(test)]
mod bounded_query_tests {
    use super::*;

    fn populated() -> EventLog {
        let mut log = EventLog::default();
        for i in 0..4100 {
            log.push(Event {
                n: 0,
                at_ms: i,
                kind: ["chat", "damage", "dialog"][i as usize % 3].into(),
                area: ["a", "b", "c", ""][i as usize % 4].into(),
                id: Some(format!("{:x}", i % 17 + 1)),
                target: (i % 5 == 0).then(|| format!("{:x}", (i + 3) % 17 + 1)),
                name: if i % 7 == 0 { "ŻÓŁĆ" } else { "Guard" }.into(),
                data: json!({"text":if i % 11 == 0 { "Rare HELLO" } else { "line" },"i":i}),
            });
        }
        log
    }

    #[test]
    fn newest_pages_match_full_scan_with_scope_filters_limits_and_eviction() {
        let log = populated();
        let visible = |e: &Event| e.area != "b" && !e.n.is_multiple_of(7);
        for area in [None, Some("A"), Some(""), Some("missing")] {
            for since in [0, 50, log.last() - 8, log.last(), u64::MAX] {
                for limit in [0, 1, 17, MAX_QUERY, MAX_QUERY + 1] {
                    for filter in 0..6 {
                        let mut q = Query {
                            area: area.map(str::to_owned),
                            since,
                            limit,
                            ..Default::default()
                        };
                        match filter {
                            1 => q.kinds = vec!["chat".into()],
                            2 => q.object = Some("7".into()),
                            3 => q.text = Some("hello".into()),
                            4 => q.text = Some("żółć".into()),
                            5 => {
                                q.kinds = vec!["damage".into()];
                                q.text = Some("missing".into());
                            }
                            _ => {}
                        }
                        // Deliberately exhaustive reference: collect every matching
                        // retained row, then sort and choose the last page.
                        let mut all: Vec<_> = log
                            .areas
                            .values()
                            .flatten()
                            .filter(|e| {
                                e.n > since
                                    && visible(e)
                                    && area.is_none_or(|a| e.area == a.to_ascii_lowercase())
                                    && (q.kinds.is_empty() || q.kinds.contains(&e.kind))
                                    && q.object.as_ref().is_none_or(|id| {
                                        e.id.as_ref() == Some(id) || e.target.as_ref() == Some(id)
                                    })
                                    && q.text.as_ref().is_none_or(|t| {
                                        e.name.to_lowercase().contains(&t.to_lowercase())
                                            || e.data
                                                .to_string()
                                                .to_lowercase()
                                                .contains(&t.to_lowercase())
                                    })
                            })
                            .collect();
                        all.sort_by_key(|e| e.n);
                        let cap = if limit == 0 { MAX_QUERY } else { limit.min(MAX_QUERY) };
                        let start = all.len().saturating_sub(cap);
                        let lost = log.evicted.iter().any(|(a, n)| {
                            *n > since
                                && area.is_none_or(|wanted| a == &wanted.to_ascii_lowercase())
                        });
                        let expected: (Vec<Event>, bool) = (
                            all[start..].iter().map(|e| (*e).clone()).collect(),
                            start > 0 || lost,
                        );
                        assert_eq!(log.query_page(&q, visible), expected, "{q:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn tail_and_small_pages_do_not_visit_all_retained_events() {
        let log = populated();
        let calls = std::cell::Cell::new(0);
        let visible = |_: &Event| {
            calls.set(calls.get() + 1);
            true
        };
        let page = log.query_page(&Query { limit: 8, ..Default::default() }, visible);
        assert_eq!(page.0.len(), 8);
        assert!(page.1);
        assert!(calls.get() <= 8 + log.areas.len() + 1, "one page and stream heads suffice");
        calls.set(0);
        assert!(
            log.query_page(&Query { since: log.last(), ..Default::default() }, visible)
                .0
                .is_empty()
        );
        assert_eq!(calls.get(), 0, "an exhausted cursor does not inspect retained payloads");
    }

    #[test]
    fn newest_limit_is_applied_after_scope_and_filters_across_areas() {
        let mut log = EventLog::default();
        let events = [
            json!({"seq":1,"area":"a","kind":"attack","id":"7","text":"alpha"}),
            json!({"seq":2,"area":"b","kind":"chat","id":"7","text":"beta"}),
            json!({"seq":3,"area":"a","kind":"attack","id":"8","target":"7","text":"beta"}),
            json!({"seq":4,"area":"","kind":"chat","id":"7","text":"beta"}),
            json!({"seq":5,"area":"b","kind":"attack","id":"7","text":"beta"}),
            json!({"seq":6,"area":"a","kind":"attack","id":"7","text":"other"}),
        ];
        log.game("boot", &events, 0, 1);
        let mut q = Query {
            kinds: vec!["attack".into()],
            object: Some("7".into()),
            text: Some("BETA".into()),
            limit: 2,
            ..Default::default()
        };
        let ids = |events: Vec<Event>| events.iter().map(|e| e.n).collect::<Vec<_>>();
        assert_eq!(ids(log.query(&q)), [3, 5]);
        q.limit = 1;
        assert_eq!(ids(log.query(&q)), [5]);
        assert_eq!(ids(log.query_filtered(&q, |e| e.area != "b")), [3]);
        q.area = Some("A".into());
        assert_eq!(ids(log.query(&q)), [3]);
        q.since = 3;
        assert!(log.query(&q).is_empty());
        q.area = None;
        assert_eq!(ids(log.query(&q)), [5]);
    }
}
