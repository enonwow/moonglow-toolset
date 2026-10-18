//! Durable bounded identities: reserve before enqueue, checkpoint before log replacement.
use crate::{
    command_history::{Boundary, Entry, HISTORY_BYTES, History, boot_key},
    store::{CommandError, ErrorCode},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_BYTES: u64 = (HISTORY_BYTES as u64) * 2;
fn hash(value: &str) -> String {
    boot_key(value)
}
fn open(path: &Path) -> Result<File, String> {
    let mut o = OpenOptions::new();
    o.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path).map_err(|e| format!("Command identity journal: {e}"))
}
#[derive(Serialize, Deserialize)]
struct Frame {
    v: u8,
    boundary: Boundary,
    rows: Vec<(String, String, Entry)>,
}
#[derive(Debug)]
pub(crate) struct Journal {
    root: PathBuf,
    _lock: File,
    boot: String,
    file: Option<File>,
    pub(crate) history: History,
    bytes: u64,
    failed: bool,
    #[cfg(test)]
    pub(crate) before_write: Option<std::sync::Arc<std::sync::Barrier>>,
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
        let mut j = Self {
            root,
            _lock: lock,
            boot: String::new(),
            file: None,
            history: History::default(),
            bytes: 0,
            failed: false,
            #[cfg(test)]
            before_write: None,
        };
        let path = j.root.join("current.jsonl");
        if path.exists() {
            let mut file = open(&path)?;
            if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
                return Err("Command identity journal exceeds its size bound".into());
            }
            let mut text = String::new();
            file.read_to_string(&mut text).map_err(|e| e.to_string())?;
            if text.is_empty() || !text.ends_with('\n') {
                return Err("Incomplete command identity journal; refusing uncertain replay".into());
            }
            for line in text.lines() {
                let frame: Frame = serde_json::from_str(line)
                    .map_err(|e| format!("Command identity record: {e}"))?;
                if frame.v != 2 || frame.rows.len() > 100_000 {
                    return Err("Invalid command identity journal".into());
                }
                j.history.merge_boundary(&frame.boundary);
                for (boot, id, entry) in frame.rows {
                    if boot.len() != 64
                        || !boot.bytes().all(|b| b.is_ascii_hexdigit())
                        || id.is_empty()
                        || id.chars().count() > 64
                        || entry.at == u64::MAX
                    {
                        return Err("Invalid command identity row".into());
                    }
                    j.history.insert(boot, id, entry);
                }
                j.history.removed.clear();
            }
            let uncertain: Vec<_> = j
                .history
                .entries
                .iter()
                .filter(|(_, e)| e.outcome.is_none() && e.fingerprint.is_some())
                .map(|((b, id), e)| (b.clone(), id.clone(), e.owner.clone()))
                .collect();
            for (boot, id, owner) in uncertain {
                j.history.outcome(
                    &boot,
                    &id,
                    crate::store::CommandResult {
                        id: id.clone(),
                        by: Some(owner),
                        ok: false,
                        code: Some(ErrorCode::Unknown),
                        err:
                            "Service restarted before a durable result; command will not be retried"
                                .into(),
                        ..Default::default()
                    },
                );
            }
            j.history.removed.clear();
            j.bytes = text.len() as u64;
            j.file = Some(file);
        }
        Ok(j)
    }
    #[cfg(test)]
    pub(crate) fn select_boot(&mut self, boot: &str) -> Result<(), String> {
        self.select_boot_at(boot, self.history.boundary.clock)
    }
    pub(crate) fn select_boot_at(&mut self, boot: &str, now: u64) -> Result<(), String> {
        if self.failed {
            return Err(
                "Command identity storage has an uncertain write; restart after repair".into()
            );
        }
        if boot.is_empty() {
            return Ok(());
        }
        self.history.advance(now);
        if self.file.is_none() {
            self.migrate_legacy(now)?;
            self.checkpoint()?;
        }
        if self.boot != boot {
            if !self.boot.is_empty() {
                self.checkpoint()?;
            }
            self.boot = boot.into();
        }
        self.history.removed.clear();
        Ok(())
    }
    /// Old journals have no fingerprint/owner: retain their reservations but never execute them.
    fn migrate_legacy(&mut self, now: u64) -> Result<(), String> {
        let mut migrated = Vec::new();
        for f in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let f = f.map_err(|e| e.to_string())?;
            let path = f.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl")
                || stem.len() != 64
                || !stem.bytes().all(|b| b.is_ascii_hexdigit())
            {
                continue;
            }
            if !f.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("Invalid legacy journal type".into());
            }
            if f.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
                return Err("Legacy command journal exceeds its size bound".into());
            }
            let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            if !text.is_empty() && !text.ends_with('\n') {
                return Err("Incomplete command identity journal; refusing uncertain replay".into());
            }
            for line in text.lines() {
                let id: String = serde_json::from_str(line)
                    .map_err(|e| format!("Command identity record: {e}"))?;
                if id.is_empty() || id.chars().count() > 64 {
                    return Err("Invalid legacy command identity".into());
                }
                let at = crate::command_issued_ms(&id).unwrap_or(now);
                self.history.insert(
                    stem.into(),
                    id,
                    Entry { at, fingerprint: None, owner: String::new(), outcome: None },
                );
                self.history.removed.clear();
            }
            migrated.push(path);
        }
        if !migrated.is_empty() {
            self.history.boundary.legacy_closed = true;
            self.checkpoint()?;
            // Only validated journal files, after the replacement boundary/rows are durable.
            for path in migrated {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    fn checkpoint(&mut self) -> Result<(), String> {
        let frame = Frame {
            v: 2,
            boundary: self.history.boundary.clone(),
            rows: self
                .history
                .entries
                .iter()
                .map(|((b, id), e)| (b.clone(), id.clone(), e.clone()))
                .collect(),
        };
        let mut bytes = serde_json::to_vec(&frame).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_BYTES / 2 {
            return Err("Identity checkpoint exceeds budget".into());
        }
        let next = self.root.join("pending.jsonl");
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&next).map_err(|e| e.to_string())?;
        f.write_all(&bytes).and_then(|()| f.sync_all()).map_err(|e| e.to_string())?;
        drop(f);
        self.file = None;
        fs::rename(&next, self.root.join("current.jsonl")).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        File::open(&self.root).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        self.file = Some(open(&self.root.join("current.jsonl"))?);
        self.bytes = bytes.len() as u64;
        Ok(())
    }
    fn append(&mut self, rows: Vec<(String, String, Entry)>) -> Result<(), String> {
        let mut bytes =
            serde_json::to_vec(&Frame { v: 2, boundary: self.history.boundary.clone(), rows })
                .map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        let result = if self.bytes + bytes.len() as u64 > MAX_BYTES {
            self.checkpoint()
        } else {
            self.bytes += bytes.len() as u64;
            self.file
                .as_mut()
                .ok_or("No identity journal")?
                .write_all(&bytes)
                .and_then(|()| self.file.as_mut().unwrap().sync_data())
                .map_err(|e| e.to_string())
        };
        if result.is_err() {
            self.failed = true;
        }
        self.history.removed.clear();
        result.map_err(|e| format!("Command identity write failed; command was not queued: {e}"))
    }
    pub(crate) fn contains_boot(&self, boot: &str, id: &str) -> bool {
        self.history.get(&boot_key(boot), id).is_some()
    }
    pub(crate) fn entry(&self, boot: &str, id: &str) -> Option<&Entry> {
        self.history.get(&boot_key(boot), id)
    }
    #[cfg(test)]
    pub(crate) fn reserve(&mut self, id: &str) -> Result<(), String> {
        let e = Entry {
            at: self.history.boundary.clock,
            fingerprint: None,
            owner: String::new(),
            outcome: None,
        };
        self.reserve_entry(id, e, self.history.boundary.clock, &Boundary::default())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    pub(crate) fn reserve_entry(
        &mut self,
        id: &str,
        entry: Entry,
        now: u64,
        boundary: &Boundary,
    ) -> Result<Boundary, CommandError> {
        if self.failed {
            return Err(CommandError::new(
                ErrorCode::Disabled,
                "Command identity storage is uncertain",
            ));
        }
        if self.boot.is_empty() {
            return Err("No server boot selected for command identity".into());
        }
        self.history.merge_boundary(boundary);
        self.history.advance(now);
        let boot = boot_key(&self.boot);
        self.history.check(&boot, id, now)?;
        if self.history.get(&boot, id).is_some() {
            return Err("Command id already reserved; it will not run again".into());
        }
        self.history.insert(boot.clone(), id.into(), entry.clone());
        self.history.check(&boot, id, now)?;
        #[cfg(test)]
        if let Some(barrier) = &self.before_write {
            barrier.wait();
            barrier.wait();
        }
        self.append(vec![(boot, id.into(), entry)])?;
        Ok(self.history.boundary.clone())
    }
    pub(crate) fn outcomes(
        &mut self,
        rows: Vec<(String, String, Entry)>,
        boundary: &Boundary,
    ) -> Result<(), String> {
        self.history.merge_boundary(boundary);
        for (boot, id, e) in &rows {
            self.history.insert(boot.clone(), id.clone(), e.clone());
        }
        self.append(rows)
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
            live.enqueue(command("explicit-once"), 1001).unwrap();
            assert_eq!(live.result("explicit-once").unwrap().code, Some(ErrorCode::Unknown));
            live.sync(br#"{"v":1,"boot":"boot-a","seq":2,"results":[{"id":"explicit-once","ok":false,"code":"not_found","err":"missing"}]}"#,1002).unwrap();
            assert_eq!(live.result("explicit-once").unwrap().code, Some(ErrorCode::NotFound));
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
        let replay = store(&dir, "server-a", "boot-a");
        replay.enqueue(command("explicit-once"), 1001).unwrap();
        assert!(replay.commands(1002)["commands"].as_array().unwrap().is_empty());
        drop(replay);
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
        let path = root.join("current.jsonl");
        OpenOptions::new().append(true).open(path).unwrap().write_all(b"\"incomplete").unwrap();
        let live = LiveStore::default();
        let error = live.set_command_identity(&dir, "server").unwrap_err();
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
    fn eviction_closes_legacy_admission_but_fresh_timed_ids_continue() {
        let dir = folder();
        let mut journal = Journal::new(dir.clone()).unwrap();
        journal.select_boot("boot").unwrap();
        journal.history.budget = 4096;
        journal.reserve("old").unwrap();
        for n in 1..50 {
            let now = n * 1000;
            let id = format!("test.1234abcd.{}", crate::command_counter(now, 0).unwrap());
            let entry = Entry::new(&id, now, [1; 32], "local".into());
            journal.reserve_entry(&id, entry, now, &Boundary::default()).unwrap();
        }
        assert!(journal.history.bytes <= 4096);
        assert!(journal.reserve("old").unwrap_err().contains("Legacy"));
        drop(journal);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn durable_outcome_owner_and_boot_survive_restart() {
        let dir = folder();
        let live = store(&dir, "server", "boot");
        let scope = crate::access::Scope::restricted(
            "operator",
            vec![crate::access::ClientGroup::Npc],
            crate::access::DeliveryMode::Auto,
        );
        live.enqueue_scoped(command("outcome"), 1001, &scope).unwrap();
        live.sync(br#"{"v":1,"boot":"boot","seq":2,"results":[{"id":"outcome","ok":false,"code":"not_found","err":"missing"}]}"#, 1002).unwrap();
        drop(live);
        let live = store(&dir, "server", "boot");
        live.enqueue_scoped(command("outcome"), 1003, &scope).unwrap();
        assert_eq!(live.result("outcome").unwrap().code, Some(ErrorCode::NotFound));
        assert_eq!(live.result("outcome").unwrap().by.as_deref(), Some("operator"));
        assert!(live.commands(1003)["commands"].as_array().unwrap().is_empty());
        let stranger = crate::access::Scope::restricted(
            "other",
            vec![crate::access::ClientGroup::Npc],
            crate::access::DeliveryMode::Auto,
        );
        assert_eq!(
            live.enqueue_scoped(command("outcome"), 1003, &stranger).unwrap_err().code,
            ErrorCode::Forbidden
        );
        drop(live);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_reservations_migrate_without_reexecution_and_truncation_fails_closed() {
        let dir = folder();
        fs::create_dir(&dir).unwrap();
        let old = dir.join(format!("{}.jsonl", boot_key("boot")));
        fs::write(&old, b"\"old\"\n").unwrap();
        let mut j = Journal::new(dir.clone()).unwrap();
        j.select_boot_at("boot", 1000).unwrap();
        assert!(j.entry("boot", "old").unwrap().fingerprint.is_none());
        assert!(j.reserve("old").is_err());
        assert!(j.reserve("new-legacy").is_err());
        assert!(!old.exists());
        drop(j);
        let mut j = Journal::new(dir.clone()).unwrap();
        j.select_boot_at("boot", 1000).unwrap();
        assert!(j.reserve("new-legacy").is_err());
        drop(j);
        // No partial record may silently drop a reservation.
        let path = dir.join("current.jsonl");
        let bytes = fs::read(&path).unwrap();
        for cut in [0, 1, bytes.len() / 2, bytes.len() - 1] {
            fs::write(&path, &bytes[..cut]).unwrap();
            assert!(Journal::new(dir.clone()).is_err());
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn durable_two_hundred_thousand_commands_rotate_and_replay_safely() {
        const START: u64 = 1_800_000_000_000;
        let dir = folder();
        let began = std::time::Instant::now();
        let id =
            |n| format!("test.1234abcd.{}", crate::command_counter(START + n * 1000, 0).unwrap());
        let mut journal = Journal::new(dir.clone()).unwrap();
        journal.select_boot_at("boot", START).unwrap();
        let mut peak = 0;
        for n in 0..200_000 {
            let now = START + n * 1000;
            let key = id(n);
            journal
                .reserve_entry(
                    &key,
                    Entry::new(&key, now, [1; 32], "owner".into()),
                    now,
                    &Boundary::default(),
                )
                .unwrap();
            assert!(journal.history.bytes <= HISTORY_BYTES);
            assert!(journal.bytes <= MAX_BYTES);
            peak = peak.max(journal.bytes);
        }
        let last = id(199_999);
        let last_entry = journal.entry("boot", &last).unwrap().clone();
        assert_eq!(last_entry.fingerprint, Some([1; 32]));
        assert!(
            journal
                .reserve_entry(&last, last_entry, START + 199_999_000, &Boundary::default())
                .is_err()
        );
        let floor = journal.history.boundary.floor;
        assert!(floor > START);
        let bytes = journal.history.bytes;
        let rows = journal.history.entries.len();
        drop(journal);
        let mut recovered = Journal::new(dir.clone()).unwrap();
        recovered.select_boot_at("boot", START).unwrap(); // clock rollback cannot reopen the floor
        let stale = id(0);
        assert_eq!(
            recovered
                .reserve_entry(
                    &stale,
                    Entry::new(&stale, START, [1; 32], "owner".into()),
                    START,
                    &Boundary::default()
                )
                .unwrap_err()
                .code,
            ErrorCode::Expired
        );
        assert_eq!(
            recovered.entry("boot", &last).unwrap().outcome.as_ref().unwrap().code,
            Some(ErrorCode::Unknown)
        );
        assert!(recovered.history.boundary.floor >= floor);
        println!(
            "ID_HORIZON commands=200000 rows={rows} memory_bytes={bytes} peak_journal_bytes={peak} elapsed_ms={}",
            began.elapsed().as_millis()
        );
        drop(recovered);
        assert!(fs::read_dir(&dir).unwrap().count() <= 2);
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
