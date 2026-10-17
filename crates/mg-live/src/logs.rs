//! The live events kept on disk: `<data_dir>/live/logs/<boot>/<group>.jsonl`,
//! a folder per server run (its boot), a file per subject (chat, combat,
//! players, server, commands, other), one JSON event per line. Chat (what
//! players write) is kept only when asked. Folders older than
//! [`MAX_AGE`] go, then the oldest while all of them pass [`MAX_BYTES`].

use crate::{events::Event, store::LiveStore};
use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, SystemTime},
};

pub const MAX_AGE: Duration = Duration::from_secs(14 * 24 * 3600);
pub const MAX_BYTES: u64 = 500 * 1024 * 1024;

/// The file an event of `kind` goes to.
pub fn group(kind: &str) -> &'static str {
    match kind {
        "chat" => "chat",
        "attack" | "damage" | "death" | "spell" | "vfx" | "combat_start" | "combat_end"
        | "target" | "healed" => "combat",
        "player_join" | "player_leave" | "entered" | "left" => "players",
        "server_log" => "server",
        "command" => "commands",
        _ => "other",
    }
}

/// The logs' folder under Moonglow's data folder.
pub fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join("live").join("logs")
}

/// Appends `events` of the run `boot` to their files (chat only with `chat`).
pub fn append(dir: &Path, boot: &str, events: &[Event], chat: bool) -> std::io::Result<()> {
    Writer::default().append(dir, boot, events, chat)
}

/// The log itself is the receipt: after a process crash, inbox replay must not
/// append an already written stream event again. Scan each touched log once.
#[derive(Default)]
struct Writer {
    receipts: HashMap<PathBuf, HashMap<String, u64>>,
}

impl Writer {
    fn append(
        &mut self,
        dir: &Path,
        boot: &str,
        events: &[Event],
        chat: bool,
    ) -> std::io::Result<()> {
        let mut groups: BTreeMap<PathBuf, Vec<&Event>> = BTreeMap::new();
        for e in events.iter().filter(|e| chat || e.kind != "chat") {
            let boot: String = e.data["boot"]
                .as_str()
                .unwrap_or(boot)
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .take(64)
                .collect();
            let run = dir.join(if boot.is_empty() { "unknown" } else { &boot });
            groups.entry(run.join(format!("{}.jsonl", group(&e.kind)))).or_default().push(e);
        }
        for (path, rows) in groups {
            std::fs::create_dir_all(path.parent().expect("log parent"))?;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(&path)?;
            let loaded = self.receipts.contains_key(&path);
            let mut marks = self.receipts.get(&path).cloned().unwrap_or_default();
            if !loaded {
                let mut reader = std::io::BufReader::new(&file);
                let mut complete = 0;
                loop {
                    let mut line = Vec::new();
                    let n = reader.read_until(b'\n', &mut line)?;
                    if n == 0 || !line.ends_with(b"\n") {
                        break;
                    }
                    let v: serde_json::Value =
                        serde_json::from_slice(&line).map_err(std::io::Error::other)?;
                    if let (Some(epoch), Some(n)) =
                        (v["data"]["queue_epoch"].as_str(), v["data"]["queue_n"].as_u64())
                    {
                        let latest = marks.entry(epoch.into()).or_default();
                        *latest = (*latest).max(n);
                    }
                    complete += n as u64;
                }
                drop(reader);
                // Only an interrupted final record is removed; the inbox still
                // has it and the next append reconstructs the complete event.
                file.set_len(complete)?;
            }
            file.seek(SeekFrom::End(0))?;
            let mut next = marks;
            let mut text = String::new();
            for e in rows {
                if let (Some(epoch), Some(n)) =
                    (e.data["queue_epoch"].as_str(), e.data["queue_n"].as_u64())
                {
                    let latest = next.entry(epoch.into()).or_default();
                    if n <= *latest {
                        continue;
                    }
                    *latest = n;
                }
                text.push_str(&serde_json::to_string(e).map_err(std::io::Error::other)?);
                text.push('\n');
            }
            if let Err(error) = file.write_all(text.as_bytes()).and_then(|()| file.sync_data()) {
                self.receipts.remove(&path);
                return Err(error);
            }
            self.receipts.insert(path, next);
        }
        Ok(())
    }
}

/// Removes runs older than `max_age` (by their last change), then the
/// oldest while all together are over `max_bytes`.
pub fn prune(
    dir: &Path,
    now: SystemTime,
    max_age: Duration,
    max_bytes: u64,
) -> std::io::Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
    let mut runs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let (mut changed, mut bytes) = (SystemTime::UNIX_EPOCH, 0);
        for file in std::fs::read_dir(&path)?.flatten() {
            let meta = file.metadata()?;
            bytes += meta.len();
            changed = changed.max(meta.modified()?);
        }
        runs.push((changed, bytes, path));
    }
    runs.sort_by_key(|r| r.0);
    let mut total: u64 = runs.iter().map(|r| r.1).sum();
    for (changed, bytes, path) in runs {
        let old = now.duration_since(changed).unwrap_or_default() > max_age;
        if old || total > max_bytes {
            std::fs::remove_dir_all(&path)?;
            total -= bytes;
        }
    }
    Ok(())
}

/// Keeps the store's new events on disk every second until `stop`.
pub fn keep(
    store: Arc<LiveStore>,
    data_dir: &Path,
    chat: bool,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    let dir = folder(data_dir);
    std::thread::spawn(move || {
        if let Err(e) = prune(&dir, SystemTime::now(), MAX_AGE, MAX_BYTES) {
            log::warn!("Live logs: pruning {}: {e}", dir.display());
        }
        let mut since = 0;
        let mut writer = Writer::default();
        loop {
            let stopping = stop.load(Ordering::Acquire);
            let (boot, events, last) = store.log_rows(since);
            if !events.is_empty() {
                match writer.append(&dir, &boot, &events, chat) {
                    Ok(()) => since = last,
                    Err(e) => log::warn!("Live logs: writing {}: {e}", dir.display()),
                }
            }
            if stopping {
                break;
            }
            for _ in 0..10 {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn final_flush_drains_more_than_an_api_page() {
        let dir = std::env::temp_dir()
            .join(format!("mg-log-drain-{}", crate::service::generate_token().unwrap()));
        let store = Arc::new(LiveStore::default());
        let events: Vec<_> =
            (1..=600).map(|n| json!({"seq":n,"kind":"chat","text":"line"})).collect();
        store
            .sync(
                &serde_json::to_vec(
                    &json!({"v":1,"boot":"drain","seq":1,"areas":[],"events":events}),
                )
                .unwrap(),
                1,
            )
            .unwrap();
        let stopped = Arc::new(AtomicBool::new(true));
        keep(store, &dir, true, stopped).join().unwrap();
        let text = std::fs::read_to_string(folder(&dir).join("drain/chat.jsonl")).unwrap();
        let seqs: Vec<u64> = text
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["data"]["game_seq"]
                    .as_u64()
                    .unwrap()
            })
            .collect();
        assert_eq!(seqs, (1..=600).collect::<Vec<_>>());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stream_replay_keeps_original_boot_and_deduplicates_across_log_writer_restarts() {
        let dir = std::env::temp_dir()
            .join(format!("mg-log-replay-{}", crate::service::generate_token().unwrap()));
        let mut first = event(1, "chat");
        first.data = json!({"boot":"old","queue_epoch":"epoch1","queue_n":1,"text":"one"});
        let mut second = first.clone();
        second.n = 2;
        second.data["queue_n"] = json!(2);
        append(&dir, "new", &[first.clone(), second.clone()], true).unwrap();
        let path = dir.join("old/chat.jsonl");
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"torn\"")
            .unwrap();
        let mut third = first.clone();
        third.n = 3;
        third.data["queue_epoch"] = json!("epoch2");
        append(&dir, "new", &[first.clone(), second.clone(), third.clone()], true).unwrap();
        append(&dir, "new", &[first, second, third], true).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap().lines().count(), 3);
        assert!(!dir.join("new").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn event(n: u64, kind: &str) -> Event {
        Event {
            n,
            at_ms: 1,
            kind: kind.into(),
            area: "gate".into(),
            id: Some("7".into()),
            name: "Guard".into(),
            target: None,
            data: json!({"text": "Zażółć"}),
        }
    }

    #[test]
    fn events_go_to_their_run_and_subject_and_chat_only_when_asked() {
        let dir = std::env::temp_dir().join(format!("mg-live-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let events = [event(1, "chat"), event(2, "death"), event(3, "command"), event(4, "attack")];
        append(&dir, "ab12", &events, false).unwrap();
        let run = dir.join("ab12");
        assert!(!run.join("chat.jsonl").exists(), "chat is kept only when asked");
        let combat = std::fs::read_to_string(run.join("combat.jsonl")).unwrap();
        assert_eq!(combat.lines().count(), 2);
        let first: serde_json::Value =
            serde_json::from_str(combat.lines().next().unwrap()).unwrap();
        assert_eq!(
            (first["kind"].as_str(), first["data"]["text"].as_str()),
            (Some("death"), Some("Zażółć"))
        );
        append(&dir, "ab12", &[event(5, "chat")], true).unwrap();
        append(&dir, "../x", &[event(6, "left")], true).unwrap();
        assert!(run.join("chat.jsonl").exists() && dir.join("x").join("players.jsonl").exists());

        // Too big together: the oldest run goes first; too old: it goes.
        let now = SystemTime::now();
        prune(&dir, now, MAX_AGE, 1).unwrap();
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert!(left.is_empty(), "both runs are over one byte");
        append(&dir, "cd34", &events, true).unwrap();
        prune(&dir, now + MAX_AGE + Duration::from_secs(60), MAX_AGE, MAX_BYTES).unwrap();
        assert!(!dir.join("cd34").exists(), "a run older than 14 days goes");
        prune(&dir.join("missing"), now, MAX_AGE, MAX_BYTES).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
