//! Streaming native capture reader. Loops adjust only sequence counters.
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

#[derive(Debug, Deserialize)]
pub(super) struct Row {
    pub at_ms: u64,
    pub channel: String,
    pub data: Value,
}
pub(super) struct Reader(BufReader<Box<dyn Read>>);
impl Reader {
    pub(super) fn open(path: &Path) -> Self {
        let file = File::open(path).expect("replay file");
        let stream: Box<dyn Read> = if path.extension().is_some_and(|e| e == "gz") {
            Box::new(GzDecoder::new(file))
        } else {
            Box::new(file)
        };
        Self(BufReader::new(stream))
    }
    pub(super) fn next(&mut self) -> Option<Row> {
        let mut line = Vec::new();
        let size = self
            .0
            .by_ref()
            .take(8 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .expect("replay read");
        if size == 0 {
            return None;
        }
        assert!(size <= 8 * 1024 * 1024, "replay line exceeds 8 MiB");
        Some(serde_json::from_slice(&line).expect("replay envelope"))
    }
}
pub(super) struct Stream {
    main: Reader,
    side: Option<Reader>,
    a: Option<Row>,
    b: Option<Row>,
}
impl Stream {
    pub(super) fn open(path: &Path, side: Option<&Path>) -> Self {
        let mut main = Reader::open(path);
        let a = main.next();
        let mut side = side.map(Reader::open);
        let b = side.as_mut().and_then(Reader::next);
        Self { main, side, a, b }
    }
    pub(super) fn next(&mut self) -> Option<Row> {
        if self.a.as_ref().is_some_and(|a| self.b.as_ref().is_none_or(|b| a.at_ms <= b.at_ms)) {
            let row = self.a.take();
            self.a = self.main.next();
            row
        } else {
            let row = self.b.take();
            self.b = self.side.as_mut().and_then(Reader::next);
            row
        }
    }
}
pub(super) struct Index {
    pub first_ms: u64,
    pub duration_ms: u64,
    pub report_stride: u64,
    pub event_stride: u64,
    pub areas: Vec<String>,
    pub client: String,
    pub speakers: BTreeMap<String, Vec<String>>,
    pub summary: Value,
}
pub(super) fn inspect(path: &Path, side: Option<&Path>) -> Index {
    let mut reader = Stream::open(path, side);
    let mut first = None;
    let mut last = 0;
    let (mut report_seq, mut event_seq) = (0, 0);
    let mut channels = BTreeMap::<String, u64>::new();
    let mut kinds = BTreeMap::<String, u64>::new();
    let mut areas = BTreeSet::new();
    let mut boots = BTreeSet::new();
    let mut clients = BTreeSet::new();
    let mut speech = BTreeMap::<(String, String), u64>::new();
    while let Some(row) = reader.next() {
        first.get_or_insert(row.at_ms);
        assert!(row.at_ms >= last, "capture timestamps must be monotonic");
        last = row.at_ms;
        *channels.entry(row.channel.clone()).or_default() += 1;
        if row.channel == "draft"
            && let Some(client) = row.data["client"].as_str()
        {
            clients.insert(client.to_owned());
        }
        if row.channel == "state" {
            report_seq = report_seq.max(row.data["seq"].as_u64().unwrap_or(0));
            for a in row.data["areas"].as_array().into_iter().flatten() {
                if let Some(name) = a["area"].as_str() {
                    areas.insert(name.to_owned());
                }
            }
        }
        if let Some(boot) = row.data["boot"].as_str() {
            boots.insert(boot.to_owned());
        }
        for e in row.data["events"].as_array().into_iter().flatten() {
            event_seq = event_seq.max(e["seq"].as_u64().unwrap_or(0));
            if matches!(e["kind"].as_str(), Some("chat" | "dialog"))
                && e["silent"] != true
                && let (Some(area), Some(id)) =
                    (e["area"].as_str(), e["speaker"].as_str().or(e["id"].as_str()))
            {
                *speech.entry((area.into(), id.into())).or_default() += 1;
            }
            *kinds.entry(e["kind"].as_str().unwrap_or("unknown").to_owned()).or_default() += 1;
        }
    }
    let first_ms = first.expect("nonempty capture");
    let duration_ms = (last - first_ms + 1000).max(1000);
    let areas: Vec<_> = areas.into_iter().collect();
    assert!(clients.len() <= 1, "replay fixture currently supports one draft client");
    let client = clients.first().cloned().unwrap_or_else(|| "Replay conversation".into());
    assert!(!areas.is_empty(), "capture contains reported areas");
    let mut ranked: Vec<_> = speech.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut speakers = BTreeMap::<String, Vec<String>>::new();
    for ((area, id), _) in ranked {
        speakers.entry(area).or_default().push(id);
    }
    Index {
        first_ms,
        duration_ms,
        report_stride: report_seq + 1,
        event_stride: event_seq + 1,
        summary: json!({"first_ms":first_ms,"duration_ms":duration_ms,"channels":channels,"event_records_including_duplicates":kinds,"boots":boots,"areas":areas,"draft_clients":clients}),
        areas,
        client,
        speakers,
    }
}
pub(super) fn offset(row: &mut Row, cycle: u64, index: &Index) {
    if row.channel == "state" {
        for key in ["seq", "base_seq"] {
            if let Some(n) = row.data[key].as_u64() {
                row.data[key] = (n + cycle * index.report_stride).into();
            }
        }
    }
    for e in row.data["events"].as_array_mut().into_iter().flatten() {
        if let Some(n) = e["seq"].as_u64() {
            e["seq"] = (n + cycle * index.event_stride).into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loop_offsets_preserve_delta_and_fast_event_identity() {
        let index = Index {
            first_ms: 0,
            duration_ms: 5000,
            report_stride: 20,
            event_stride: 100,
            areas: vec![],
            client: "Replay conversation".into(),
            speakers: Default::default(),
            summary: Value::Null,
        };
        let mut state = Row {
            at_ms: 10,
            channel: "state".into(),
            data: json!({"boot":"same","seq":5,"base_seq":4,"areas":[{"area":"field","cre":[{"id":"f","look_hash":17}]}],"events":[{"seq":9,"text":"unchanged"}]}),
        };
        let mut fast = Row {
            at_ms: 9,
            channel: "event".into(),
            data: json!({"boot":"same","events":[{"seq":9,"text":"unchanged"}]}),
        };
        offset(&mut state, 3, &index);
        offset(&mut fast, 3, &index);
        assert_eq!(state.data["seq"], 65);
        assert_eq!(state.data["base_seq"], 64);
        assert_eq!(state.data["events"], fast.data["events"]);
        assert_eq!(state.data["boot"], "same");
        assert_eq!(state.data["areas"][0]["cre"][0]["look_hash"], 17);
    }
}
