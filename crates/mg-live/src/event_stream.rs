//! Ordered Redis stream receipts. The local inbox is committed before XACK;
//! replaying it restores the bounded store after a process crash.
use crate::{
    bridge::{RedisConfig, connect, pause, request},
    resp::Reply,
    store::LiveStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const MAX_ROWS: usize = 10000;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const BATCH: &[u8] = b"128";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub id: String,
    pub n: u64,
    pub epoch: String,
    pub source: String,
    pub kind: String,
    pub codepage: String,
    pub data: Value,
    #[serde(default)]
    pub lost_before: u64,
    #[serde(default)]
    pub reset_before: bool,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Status {
    pub connected: bool,
    pub backlog: u64,
    pub lost: u64,
    pub applied: u64,
    pub last_id: String,
    pub error: String,
    pub persistent: bool,
    pub warning: String,
}

#[derive(Debug)]
pub(crate) struct Inbox {
    root: PathBuf,
    _lock: File,
    file: File,
    pub group: String,
    rows: VecDeque<Entry>,
    bytes: usize,
    written: usize,
    retained_bytes: usize,
}

fn open(path: &Path) -> Result<File, String> {
    let mut o = OpenOptions::new();
    o.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut file = o.open(path).map_err(|e| format!("Live event inbox: {e}"))?;
    file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    Ok(file)
}

impl Inbox {
    pub(crate) fn new(data: &Path, server: &str) -> Result<Self, String> {
        let digest = ring::digest::digest(&ring::digest::SHA256, server.as_bytes());
        let key: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
        let root = data.join("live/event-inbox").join(key);
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let lock = open(&root.join("writer.lock"))?;
        lock.try_lock().map_err(|e| format!("Another Live instance owns the event inbox: {e}"))?;
        let identity = root.join("consumer");
        let group = if identity.exists() {
            fs::read_to_string(&identity).map_err(|e| e.to_string())?
        } else {
            let group = format!("moonglow-{}", crate::service::generate_token()?);
            let mut file = open(&identity)?;
            file.write_all(group.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?;
            group
        };
        if !group.starts_with("moonglow-")
            || group.len() != 73
            || !group[9..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Invalid Live stream consumer identity".into());
        }
        let path = root.join("inbox.jsonl");
        let mut file = open(&path)?;
        if file.metadata().map_err(|e| e.to_string())?.len() > (MAX_BYTES * 3) as u64 {
            return Err("Live event inbox exceeds its bound".into());
        }
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        // An interrupted final append was never acknowledged. Replay only its
        // complete prefix; Redis still owns the torn batch in the pending list.
        let complete = raw.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        file.set_len(complete as u64).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        let mut rows = VecDeque::new();
        let mut written = 0;
        for line in raw[..complete].split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            let entry: Entry =
                serde_json::from_slice(line).map_err(|e| format!("Invalid event inbox: {e}"))?;
            entry.validate()?;
            rows.push_back(entry);
            written += 1;
        }
        let retained_bytes = rows.iter().map(|e| e.data.to_string().len() + 512).sum();
        let mut inbox =
            Self { root, _lock: lock, file, group, rows, bytes: complete, written, retained_bytes };
        inbox.bound();
        Ok(inbox)
    }
    fn bound(&mut self) {
        while self.rows.len() > MAX_ROWS || (self.retained_bytes > MAX_BYTES && self.rows.len() > 1)
        {
            let e = self.rows.pop_front().unwrap();
            self.retained_bytes -= e.data.to_string().len() + 512;
        }
    }
    fn last(&self) -> Option<&Entry> {
        self.rows.back()
    }
    fn contains(&self, entry: &Entry) -> bool {
        self.last().is_some_and(|last| last.epoch == entry.epoch && entry.n <= last.n)
    }
    fn append(&mut self, entries: &[Entry]) -> Result<(), String> {
        // Maintenance must fail before advancing the durable receipt. Otherwise
        // a failed compaction could skip store application, then the retry would
        // mistake the already appended row for an applied duplicate and ACK it.
        if self.written > MAX_ROWS * 2 || self.bytes > MAX_BYTES * 2 {
            self.compact()?;
        }
        let mut data = Vec::new();
        for entry in entries {
            serde_json::to_writer(&mut data, entry).map_err(|e| e.to_string())?;
            data.push(b'\n');
        }
        let original = self.file.metadata().map_err(|e| e.to_string())?.len();
        if let Err(e) = self.file.write_all(&data).and_then(|()| self.file.sync_data()) {
            self.file.set_len(original).map_err(|e| e.to_string())?;
            self.file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
            return Err(format!("Event inbox commit failed; entries remain pending: {e}"));
        }
        self.retained_bytes +=
            entries.iter().map(|e| e.data.to_string().len() + 512).sum::<usize>();
        self.rows.extend(entries.iter().cloned());
        self.bytes += data.len();
        self.written += entries.len();
        self.bound();
        Ok(())
    }
    fn compact(&mut self) -> Result<(), String> {
        let path = self.root.join("compact.tmp");
        let mut file = File::create(&path).map_err(|e| e.to_string())?;
        let mut bytes = 0;
        for row in &self.rows {
            let text = serde_json::to_string(row).map_err(|e| e.to_string())? + "\n";
            file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            bytes += text.len();
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&path, self.root.join("inbox.jsonl")).map_err(|e| e.to_string())?;
        self.file = open(&self.root.join("inbox.jsonl"))?;
        self.bytes = bytes;
        self.written = self.rows.len();
        Ok(())
    }
    pub(crate) fn restore(&self, store: &LiveStore) -> Result<(), String> {
        for entry in &self.rows {
            store.stream_event(entry, crate::now_ms())?;
        }
        Ok(())
    }
    pub(crate) fn restored_status(&self, status: &SharedStatus) {
        let mut s = status.lock().unwrap_or_else(|e| e.into_inner());
        s.last_id = self.last().map_or(String::new(), |e| e.id.clone());
        s.lost = self.rows.iter().map(|e| e.lost_before).sum();
    }
}

impl Entry {
    fn validate(&self) -> Result<(), String> {
        let id = self
            .id
            .split_once('-')
            .is_some_and(|(a, b)| a.parse::<u64>().is_ok() && b.parse::<u64>().is_ok());
        if !id
            || self.n == 0
            || self.epoch.is_empty()
            || self.epoch.len() > 128
            || self.source.is_empty()
            || self.source.len() > 128
        {
            return Err("Invalid stream entry identity".into());
        }
        if self.data.to_string().len() > 65536 {
            return Err("Stream event exceeds 64 KiB".into());
        }
        if self.kind == "game" {
            if !self.data["seq"].as_i64().is_some_and(|n| n > 0)
                || self.data["kind"].as_str().is_none()
            {
                return Err("Invalid game stream event".into());
            }
        } else if self.kind != "log"
            || self.data["kind"] != "server_log"
            || self.data["line"].as_str().is_none()
            || self.data["stream"].as_str().is_none()
            || !self.data["seq"].as_i64().is_some_and(|n| n > 0)
        {
            return Err("Invalid log stream event".into());
        }
        Ok(())
    }
}

fn bulk(r: &Reply) -> Result<&str, String> {
    match r {
        Reply::Bulk(Some(b)) => std::str::from_utf8(b).map_err(|e| e.to_string()),
        _ => Err("Expected Redis stream string".into()),
    }
}
fn array(r: &Reply) -> Result<&[Reply], String> {
    match r {
        Reply::Array(Some(a)) => Ok(a),
        _ => Err(format!("Expected Redis stream array: {r:?}")),
    }
}
fn entries(reply: &Reply) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for row in array(reply)? {
        let row = array(row)?;
        if row.len() != 2 {
            return Err("Invalid stream row".into());
        }
        if row[1] == Reply::Bulk(None) {
            continue;
        } // pending ID trimmed; ordinal gap below reports it
        let fields = array(&row[1])?;
        if fields.len() % 2 != 0 {
            return Err("Invalid stream fields".into());
        }
        let get = |name: &str| -> Result<String, String> {
            for pair in fields.as_chunks::<2>().0 {
                if bulk(&pair[0])? == name {
                    return Ok(bulk(&pair[1])?.to_string());
                }
            }
            Err(format!("Missing stream field {name}"))
        };
        let e = Entry {
            id: bulk(&row[0])?.into(),
            n: get("n")?.parse().map_err(|_| "Invalid stream ordinal")?,
            epoch: get("epoch")?,
            source: get("source")?,
            kind: get("type")?,
            codepage: get("codepage")?,
            data: serde_json::from_str(&get("data")?).map_err(|e| e.to_string())?,
            lost_before: 0,
            reset_before: false,
        };
        e.validate()?;
        out.push(e);
    }
    Ok(out)
}

pub(crate) type SharedInbox = Arc<Mutex<Option<Inbox>>>;
pub(crate) type SharedStatus = Arc<Mutex<Status>>;
pub(crate) fn run<A: crate::bridge::Addresses>(
    config: RedisConfig,
    addresses: A,
    inbox: SharedInbox,
    status: SharedStatus,
    store: Arc<LiveStore>,
    stop: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let key = format!("{}.events", config.prefix);
    let mut retry = 1;
    while !stop.load(Ordering::Acquire) {
        let group =
            inbox.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|i| i.group.clone());
        let Some(group) = group else {
            pause(&stop, Duration::from_millis(100));
            continue;
        };
        let result = (|| {
            let mut c = connect(&addresses, &stop)?;
            let info = request(&mut c, &[b"INFO", b"persistence"])?;
            let persistent = bulk(&info)?.lines().any(|line| line.trim() == "aof_enabled:1");
            {
                let mut s = status.lock().unwrap_or_else(|e| e.into_inner());
                s.persistent = persistent;
                s.warning = if persistent {
                    String::new()
                } else {
                    "Redis AOF is disabled: a Redis restart can lose queued events".into()
                };
            }
            match request(
                &mut c,
                &[b"XGROUP", b"CREATE", key.as_bytes(), group.as_bytes(), b"0", b"MKSTREAM"],
            )? {
                Reply::Simple(s) if s == "OK" => {}
                Reply::Error(e) if e.starts_with("BUSYGROUP") => {}
                r => return Err(format!("Redis stream group: {r:?}")),
            }
            // One filesystem lock owns this group, so idle=0 cannot steal live
            // work. A stable consumer name also bounds Redis consumer metadata.
            let consumer = "store";
            let mut claim = "0-0".to_string();
            loop {
                if stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                let reply = request(
                    &mut c,
                    &[
                        b"XAUTOCLAIM",
                        key.as_bytes(),
                        group.as_bytes(),
                        consumer.as_bytes(),
                        b"0",
                        claim.as_bytes(),
                        b"COUNT",
                        BATCH,
                    ],
                )?;
                let rows = array(&reply)?;
                if rows.len() < 2 {
                    return Err("Invalid XAUTOCLAIM reply".into());
                }
                claim = bulk(&rows[0])?.into();
                apply(&mut c, &key, &group, entries(&rows[1])?, &inbox, &status, &store)?;
                if claim == "0-0" {
                    break;
                }
            }
            retry = 1;
            status.lock().unwrap_or_else(|e| e.into_inner()).connected = true;
            status.lock().unwrap_or_else(|e| e.into_inner()).error.clear();
            wake();
            while !stop.load(Ordering::Acquire) {
                let reply = request(
                    &mut c,
                    &[
                        b"XREADGROUP",
                        b"GROUP",
                        group.as_bytes(),
                        consumer.as_bytes(),
                        b"COUNT",
                        BATCH,
                        b"BLOCK",
                        b"250",
                        b"STREAMS",
                        key.as_bytes(),
                        b">",
                    ],
                )?;
                if !matches!(reply, Reply::Array(None) | Reply::Bulk(None)) {
                    for stream in array(&reply)? {
                        let stream = array(stream)?;
                        if stream.len() != 2 || bulk(&stream[0])? != key {
                            return Err("Invalid XREADGROUP reply".into());
                        }
                        apply(&mut c, &key, &group, entries(&stream[1])?, &inbox, &status, &store)?;
                    }
                    wake();
                }
                let meta = request(
                    &mut c,
                    &[
                        b"HMGET",
                        format!("{}.events.meta", config.prefix).as_bytes(),
                        b"epoch",
                        b"n",
                    ],
                )?;
                let meta = array(&meta)?;
                if meta.len() == 2 && meta[0] != Reply::Bulk(None) {
                    let epoch = bulk(&meta[0])?;
                    let n: u64 = bulk(&meta[1])?.parse().map_err(|_| "Invalid stream metadata")?;
                    let guard = inbox.lock().unwrap_or_else(|e| e.into_inner());
                    let applied = guard
                        .as_ref()
                        .and_then(|i| i.last())
                        .filter(|e| e.epoch == epoch)
                        .map_or(0, |e| e.n);
                    status.lock().unwrap_or_else(|e| e.into_inner()).backlog =
                        n.saturating_sub(applied);
                }
            }
            Ok(())
        })();
        status.lock().unwrap_or_else(|e| e.into_inner()).connected = false;
        if let Err(e) = result {
            status.lock().unwrap_or_else(|e| e.into_inner()).error = e;
            wake();
        }
        pause(&stop, Duration::from_secs(retry));
        retry = (retry * 2).min(10);
    }
}

fn apply(
    c: &mut std::io::BufReader<std::net::TcpStream>,
    key: &str,
    group: &str,
    rows: Vec<Entry>,
    inbox: &SharedInbox,
    status: &SharedStatus,
    store: &LiveStore,
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut guard = inbox.lock().unwrap_or_else(|e| e.into_inner());
    let inbox = guard.as_mut().ok_or("Event inbox is not configured")?;
    let mut fresh: Vec<_> = rows.iter().filter(|e| !inbox.contains(e)).cloned().collect();
    let previous = inbox.last().cloned();
    // Validate the entire page before committing any prefix; malformed entries
    // remain pending and visible in status instead of being silently discarded.
    for e in &fresh {
        e.validate()?;
    }
    let mut last = previous.map(|p| (p.epoch, p.n));
    for e in &mut fresh {
        e.reset_before = last.as_ref().is_some_and(|(epoch, _)| *epoch != e.epoch);
        let before = last.as_ref().filter(|(epoch, _)| *epoch == e.epoch).map_or(0, |(_, n)| *n);
        e.lost_before = e.n.saturating_sub(before + 1);
        last = Some((e.epoch.clone(), e.n));
    }
    inbox.append(&fresh)?;
    for e in &fresh {
        store.stream_event(e, crate::now_ms())?;
        let mut s = status.lock().unwrap_or_else(|e| e.into_inner());
        s.lost += e.lost_before;
        s.last_id = e.id.clone();
        s.applied += 1;
    }
    drop(guard);
    let mut args: Vec<&[u8]> = vec![b"XACK", key.as_bytes(), group.as_bytes()];
    args.extend(rows.iter().map(|e| e.id.as_bytes()));
    match request(c, &args)? {
        Reply::Int(_) => Ok(()),
        r => Err(format!("Redis XACK: {r:?}")),
    }
}

#[cfg(test)]
#[path = "event_stream_tests.rs"]
mod tests;
