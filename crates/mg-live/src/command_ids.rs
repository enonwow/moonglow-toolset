//! Write-ahead ID reservations. No command payloads or credentials are persisted.
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_IDS: usize = 65_536;
const MAX_BYTES: u64 = 32 * 1024 * 1024;

fn hash(value: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, value.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn open(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|e| format!("Command identity journal: {e}"))
}

#[derive(Debug)]
pub(crate) struct Journal {
    root: PathBuf,
    // Own one writer per server namespace, across processes as well as threads.
    _lock: File,
    boot: String,
    file: Option<File>,
    ids: HashSet<String>,
}

impl Journal {
    pub(crate) fn path(data: &Path, server: &str) -> PathBuf {
        data.join("live/command-ids").join(hash(server))
    }

    pub(crate) fn is_at(&self, path: &Path) -> bool {
        self.root == path
    }

    pub(crate) fn new(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|e| format!("Command identity directory: {e}"))?;
        let lock = open(&root.join("writer.lock"))?;
        lock.try_lock()
            .map_err(|e| format!("Another Live service owns this command journal: {e}"))?;
        Ok(Self { root, _lock: lock, boot: String::new(), file: None, ids: HashSet::new() })
    }

    pub(crate) fn select_boot(&mut self, boot: &str) -> Result<(), String> {
        if boot == self.boot || boot.is_empty() {
            return Ok(());
        }
        let mut file = open(&self.root.join(format!("{}.jsonl", hash(boot))))?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
            return Err("Command identity journal exceeds its size bound".into());
        }
        let mut text = String::new();
        file.read_to_string(&mut text).map_err(|e| format!("Command identity read: {e}"))?;
        if !text.is_empty() && !text.ends_with('\n') {
            return Err("Incomplete command identity journal; refusing uncertain replay".into());
        }
        let mut ids = HashSet::new();
        for line in text.lines() {
            let id: String =
                serde_json::from_str(line).map_err(|e| format!("Command identity record: {e}"))?;
            if id.is_empty() || id.chars().count() > 64 || !ids.insert(id) || ids.len() > MAX_IDS {
                return Err("Invalid command identity journal; refusing uncertain replay".into());
            }
        }
        self.boot = boot.into();
        self.ids = ids;
        self.file = Some(file);
        Ok(())
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    /// Reserve before any native enqueue/publisher wake. Even an uncertain
    /// write failure retains the in-memory reservation; callers fail closed.
    pub(crate) fn reserve(&mut self, id: &str) -> Result<(), String> {
        if self.ids.contains(id) {
            return Err(
                "Command id was already reserved in this server boot; it will not run again".into(),
            );
        }
        if self.ids.len() == MAX_IDS {
            return Err("Server boot command identity limit reached (65536); IDs are never silently forgotten".into());
        }
        let file = self.file.as_mut().ok_or("No server boot selected for command identity")?;
        let line = serde_json::to_string(id).map_err(|e| e.to_string())? + "\n";
        self.ids.insert(id.into());
        file.write_all(line.as_bytes()).and_then(|()| file.sync_data()).map_err(|e| {
            format!("Command identity reservation failed; command was not queued: {e}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Command, LiveStore, Op};
    use serde_json::json;

    fn folder() -> PathBuf {
        std::env::temp_dir()
            .join(format!("mg-live-ids-{}", crate::service::generate_token().unwrap()))
    }
    fn command(id: &str) -> Command {
        serde_json::from_value(
            json!({"id":id,"area":"a","op":"set_name","target":"1","name":"changed"}),
        )
        .unwrap()
    }
    fn store(dir: &Path, server: &str, boot: &str) -> LiveStore {
        let live = LiveStore::default();
        live.set_command_identity(dir, server).unwrap();
        live.sync(
            &serde_json::to_vec(&json!({"v":1,"boot":boot,"seq":1,"areas":[{
                "area":"a","full":true,"cre":[{"id":"1","name":"before"}]
            }]}))
            .unwrap(),
            1000,
        )
        .unwrap();
        live
    }

    #[test]
    fn restart_rejects_reserved_ids_and_scopes_them_to_server_and_boot() {
        let dir = folder();
        {
            let live = store(&dir, "server-a", "boot-a");
            let c = command("explicit-once");
            assert_eq!(live.enqueue(c.clone(), 1001).unwrap(), "explicit-once");
            assert_eq!(live.enqueue(c, 1002).unwrap(), "explicit-once");
            assert_eq!(live.commands(1002)["commands"].as_array().unwrap().len(), 1);
            // Drop with an uncertain native outcome: no result is needed to reserve.
        }
        {
            let live = store(&dir, "server-a", "boot-a");
            assert!(
                live.enqueue(command("explicit-once"), 1001)
                    .unwrap_err()
                    .to_string()
                    .contains("already reserved")
            );
            let mut changed = command("explicit-once");
            changed.op = Op::SetName { target: "1".into(), name: "different".into() };
            assert!(live.enqueue(changed, 1001).is_err());
            assert!(live.commands(1002)["commands"].as_array().unwrap().is_empty());
            live.enqueue(command("new"), 1003).unwrap();
        }
        for (server, boot) in [("server-b", "boot-a"), ("server-a", "boot-b")] {
            let live = store(&dir, server, boot);
            live.enqueue(command("explicit-once"), 1001).unwrap();
        }
        assert!(store(&dir, "server-a", "boot-a").enqueue(command("explicit-once"), 1001).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_or_concurrently_owned_journal_fails_closed() {
        let dir = folder();
        let root = Journal::path(&dir, "server");
        let mut journal = Journal::new(root.clone()).unwrap();
        assert!(Journal::new(root.clone()).is_err());
        journal.select_boot("boot").unwrap();
        journal.reserve("id").unwrap();
        drop(journal);
        let path = root.join(format!("{}.jsonl", hash("boot")));
        OpenOptions::new().append(true).open(path).unwrap().write_all(b"\"incomplete").unwrap();
        let live = LiveStore::default();
        live.set_command_identity(&dir, "server").unwrap();
        let error = live.sync(br#"{"v":1,"boot":"boot","seq":1,"areas":[]}"#, 1000).unwrap_err();
        assert!(error.contains("Incomplete"));
        assert!(live.enqueue(command("id"), 1001).is_err());
        drop(live);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn service_startup_gate_and_storage_failures_never_enqueue() {
        let dir = folder();
        let live = LiveStore::default();
        live.require_command_identity();
        assert!(live.enqueue(command("id"), 1000).unwrap_err().to_string().contains("not ready"));
        fs::write(&dir, b"not a directory").unwrap();
        assert!(live.set_command_identity(&dir, "server").is_err());
        assert!(live.commands(1001)["commands"].as_array().unwrap().is_empty());
        fs::remove_file(dir).unwrap();
    }

    #[test]
    fn bounded_window_refuses_new_work_instead_of_forgetting_ids() {
        let dir = folder();
        let mut journal = Journal::new(dir.clone()).unwrap();
        journal.select_boot("b").unwrap();
        journal.ids = (0..MAX_IDS).map(|n| n.to_string()).collect();
        assert!(journal.reserve("new").unwrap_err().contains("limit"));
        assert!(journal.contains("0"));
        drop(journal);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn measure_durable_enqueue() {
        if std::env::var("MGLIVE_MEASURE_IDS").as_deref() != Ok("1") {
            return;
        }
        let dir = folder();
        let durable = store(&dir, "server", "boot");
        let memory = LiveStore::default();
        memory.sync(br#"{"v":1,"boot":"boot","seq":1,"areas":[{"area":"a","full":true,"cre":[{"id":"1"}]}]}"#, 1000).unwrap();
        let mut samples = [Vec::new(), Vec::new()];
        for n in 0..32 {
            for (i, store) in [&memory, &durable].into_iter().enumerate() {
                let start = std::time::Instant::now();
                store.enqueue(command(&format!("id-{n}")), 1001).unwrap();
                samples[i].push(start.elapsed().as_micros());
            }
        }
        for (name, values) in ["memory", "durable"].into_iter().zip(&mut samples) {
            values.sort_unstable();
            println!(
                "ID_ENQUEUE {name} n=32 median_us={} p95_us={} max_us={}",
                values[16], values[30], values[31]
            );
        }
        drop(durable);
        fs::remove_dir_all(dir).unwrap();
    }
}
