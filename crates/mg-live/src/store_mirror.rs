//! Passive finished-state replication. No game wire interpretation happens here.
use super::*;
use crate::events::Event;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
const HISTORY_CAPACITY: usize = 4096;
const RESULT_CAPACITY: usize = 256;
const BYTE_BUDGET: usize = 4 * 1024 * 1024;
const RETENTION: Duration = Duration::from_secs(600);
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultState {
    Pending,
    Final,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirroredResult {
    pub id: String,
    pub rev: u64,
    pub state: ResultState,
    pub result: Option<CommandResult>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watched {
    pub desired: Vec<String>,
    pub reported: Vec<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameStatus {
    pub connected: bool,
    pub age_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorCommand {
    #[serde(flatten)]
    pub command: Command,
    pub boot: String,
    pub ttl_ms: u64,
    #[serde(skip)]
    pub sent: bool,
    #[serde(skip)]
    queued_ms: u64,
}

#[derive(Debug, Default)]
pub(super) struct Mirror {
    pub revision: u64,
    pub queue: VecDeque<MirrorCommand>,
    pub(super) results: BTreeMap<String, MirroredResult>,
    pub(super) accepted: HashMap<String, [u8; 32]>,
    touched: HashMap<String, Instant>,
    highest: BTreeMap<String, u64>,
    prefix: String,
    watched: Watched,
    pub game: GameStatus,
}
impl Mirror {
    fn prune(&mut self, now: Instant) {
        let before = (self.accepted.len(), self.results.len());
        let queued: std::collections::HashSet<_> =
            self.queue.iter().map(|q| q.command.id.clone()).collect();
        self.accepted.retain(|id, _| {
            queued.contains(id)
                || self
                    .touched
                    .get(id)
                    .is_some_and(|at| now.saturating_duration_since(*at) < RETENTION)
        });
        self.results.retain(|id, _| {
            self.touched.get(id).is_some_and(|at| now.saturating_duration_since(*at) < RETENTION)
        });
        while self.accepted.len() > HISTORY_CAPACITY {
            let Some(id) = self
                .accepted
                .keys()
                .filter(|id| !queued.contains(*id))
                .min_by_key(|id| self.touched.get(*id))
                .cloned()
            else {
                break;
            };
            self.accepted.remove(&id);
        }
        let mut bytes: usize = self
            .results
            .values()
            .map(|r| serde_json::to_vec(r).expect("result serializes").len())
            .sum();
        while self.results.len() > RESULT_CAPACITY || bytes > BYTE_BUDGET {
            let Some(id) = self.results.keys().min_by_key(|id| self.touched.get(*id)).cloned()
            else {
                break;
            };
            if let Some(row) = self.results.remove(&id) {
                bytes = bytes
                    .saturating_sub(serde_json::to_vec(&row).expect("result serializes").len());
            }
        }
        self.touched.retain(|id, _| {
            self.accepted.contains_key(id) || self.results.contains_key(id) || queued.contains(id)
        });
        if before != (self.accepted.len(), self.results.len()) {
            self.revision += 1;
        }
    }
    fn check_id(&self, id: &str) -> Result<(String, u64), CommandError> {
        let (process, count) = id
            .strip_prefix(&self.prefix)
            .and_then(|s| s.split_once('.'))
            .ok_or("Invalid mirror command ID")?;
        let count = count.parse::<u64>().map_err(|_| "Invalid mirror command counter")?;
        if id.len() > 64
            || process.len() != 8
            || !process.bytes().all(|c| c.is_ascii_hexdigit())
            || count == 0
        {
            return Err("Invalid mirror command ID".into());
        }
        if self.highest.get(process).is_some_and(|last| count <= *last) {
            return Err("Command ID is outside the retained retry window".into());
        }
        if !self.highest.contains_key(process) && self.highest.len() >= 64 {
            return Err("Too many command processes for this mirror prefix".into());
        }
        Ok((process.into(), count))
    }
    pub(super) fn result(&self, id: &str) -> Option<CommandResult> {
        self.results
            .get(id)
            .filter(|_| self.touched.get(id).is_some_and(|at| at.elapsed() < RETENTION))
            .and_then(|row| row.result.clone())
    }
    pub(super) fn expire(&mut self, now_ms: u64) {
        self.prune(Instant::now());
        let expired: Vec<_> = self
            .queue
            .iter()
            .filter(|q| !q.sent && now_ms.saturating_sub(q.queued_ms) >= q.ttl_ms)
            .map(|q| q.command.id.clone())
            .collect();
        for id in expired {
            self.reject(&id, ErrorCode::Expired, "expired: not sent before ttl_ms");
        }
    }
    fn reject(&mut self, id: &str, code: ErrorCode, msg: &str) {
        self.queue.retain(|q| q.command.id != id);
        self.revision += 1;
        self.touched.entry(id.into()).or_insert_with(Instant::now);
        self.results.insert(
            id.into(),
            MirroredResult {
                id: id.into(),
                rev: self.revision,
                state: if code == ErrorCode::Unknown {
                    ResultState::Unknown
                } else {
                    ResultState::Final
                },
                result: Some(failure(id, code, msg)),
            },
        );
        self.prune(Instant::now());
    }
}
fn prune_owners(s: &mut State) {
    if let Some(m) = &s.mirror {
        s.owners.retain(|id, _| m.accepted.contains_key(id) || m.results.contains_key(id));
    }
}

pub(super) fn enqueue(
    s: &mut State,
    mut command: Command,
    now_ms: u64,
    scope: &crate::access::Scope,
) -> Result<String, CommandError> {
    if let Some(error) = policy_error(s.policy, &command.op) {
        return Err(CommandError::new(ErrorCode::Disabled, error));
    }
    if s.boot.is_empty() {
        return Err("No mirrored game boot".into());
    }
    let mirror = s.mirror.as_mut().expect("mirror branch");
    if mirror.prefix.is_empty() {
        return Err("Gateway id prefix is not set".into());
    }
    if command.id.is_empty() {
        static PROCESS: OnceLock<String> = OnceLock::new();
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let process = if let Some(process) = PROCESS.get() {
            process.clone()
        } else {
            let mut bytes = [0; 4];
            getrandom::fill(&mut bytes).map_err(|e| format!("Command id randomness: {e}"))?;
            PROCESS.get_or_init(|| format!("{:08x}", u32::from_ne_bytes(bytes))).clone()
        };
        command.id =
            format!("{}{process}.{}", mirror.prefix, COUNTER.fetch_add(1, Ordering::Relaxed) + 1);
    }
    mirror.prune(Instant::now());
    let fingerprint = command_fingerprint(&command);
    if let Some(old) = mirror.accepted.get(&command.id) {
        return if old == &fingerprint {
            Ok(command.id)
        } else {
            Err("Command id already belongs to another operation".into())
        };
    }
    let (process, counter) = mirror.check_id(&command.id)?;
    let queue_bytes: usize =
        mirror.queue.iter().map(|q| serde_json::to_vec(q).expect("command serializes").len()).sum();
    if queue_bytes
        + serde_json::to_vec(&command).expect("command serializes").len()
        + s.boot.len()
        + 128
        > BYTE_BUDGET
    {
        return Err("Mirror command byte budget exceeded".into());
    }
    if mirror.queue.len() >= MAX_QUEUE {
        return Err("Too many queued mirror commands".into());
    }
    let id = command.id.clone();
    mirror.highest.insert(process, counter);
    mirror.touched.insert(id.clone(), Instant::now());
    mirror.accepted.insert(id.clone(), fingerprint);
    s.owners.insert(id.clone(), scope.name().into());
    mirror.queue.push_back(MirrorCommand {
        command,
        boot: s.boot.clone(),
        ttl_ms: COMMAND_TTL_MS,
        sent: false,
        queued_ms: now_ms,
    });
    mirror.revision += 1;
    mirror.prune(Instant::now());
    prune_owners(s);
    if let Some(kick) = &s.kick {
        let _ = kick.send(());
    }
    Ok(id)
}

impl LiveStore {
    pub fn new_mirror() -> Self {
        Self {
            state: Mutex::new(State { mirror: Some(Mirror::default()), ..State::default() }),
            ..Self::default()
        }
    }
    pub fn is_mirror(&self) -> bool {
        self.lock().mirror.is_some()
    }
    pub fn set_id_prefix(&self, prefix: &str) -> Result<(), String> {
        if prefix.is_empty()
            || prefix.len() > 32
            || !prefix.ends_with('.')
            || !prefix.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return Err("Invalid gateway id prefix".into());
        }
        let mut s = self.lock();
        let mirror = s.mirror.as_mut().ok_or("Not a mirror")?;
        if mirror.prefix != prefix {
            if !mirror.queue.is_empty() {
                return Err("Reset pending commands before changing prefix".into());
            }
            mirror.highest.clear();
        }
        mirror.prefix = prefix.into();
        mirror.revision += 1;
        Ok(())
    }
    pub fn mirror_area(
        &self,
        area: &str,
        mut live: AreaLive,
        received_ms: u64,
    ) -> Result<(), String> {
        let mut s = self.lock();
        s.mirror.as_mut().ok_or("Not a mirror")?.revision += 1;
        live.received_ms = received_ms;
        s.areas.insert(area.to_ascii_lowercase(), live);
        Ok(())
    }
    pub fn mirror_area_gone(&self, area: &str) -> Result<(), String> {
        let mut s = self.lock();
        s.mirror.as_mut().ok_or("Not a mirror")?.revision += 1;
        s.areas.remove(&area.to_ascii_lowercase());
        Ok(())
    }
    pub fn mirror_meta(
        &self,
        boot: String,
        factions: Option<Vec<ServerFaction>>,
        watched: Watched,
        policy: Policy,
        game: GameStatus,
    ) -> Result<(), String> {
        let mut s = self.lock();
        let mirror = s.mirror.as_mut().ok_or("Not a mirror")?;
        mirror.revision += 1;
        mirror.watched = watched.clone();
        mirror.game = game;
        s.boot = boot;
        s.factions = factions;
        s.policy = policy;
        s.watch.reported = Some(watched.reported);
        Ok(())
    }
    pub fn mirror_events(&self, rows: Vec<Event>, through: u64) -> Result<(), String> {
        let mut s = self.lock();
        if s.mirror.is_none() {
            return Err("Not a mirror".into());
        }
        s.events.mirror_batch(rows, through)?;
        s.mirror.as_mut().unwrap().revision += 1;
        Ok(())
    }
    pub fn mirror_result(
        &self,
        id: &str,
        rev: u64,
        state: ResultState,
        result: Option<CommandResult>,
    ) -> Result<(), String> {
        if result.as_ref().is_some_and(|r| r.id != id) {
            return Err("Result id mismatch".into());
        }
        let row = MirroredResult { id: id.into(), rev, state, result };
        if id.len() > 64 || serde_json::to_vec(&row).map_err(|e| e.to_string())?.len() > BYTE_BUDGET
        {
            return Err("Mirror result exceeds byte budget".into());
        }
        let mut s = self.lock();
        let mirror = s.mirror.as_mut().ok_or("Not a mirror")?;
        mirror.prune(Instant::now());
        if mirror
            .results
            .get(id)
            .is_some_and(|old| old.rev >= rev || old.state == ResultState::Final)
        {
            return Ok(());
        }
        mirror.queue.retain(|q| q.command.id != id);
        mirror.touched.entry(id.into()).or_insert_with(Instant::now);
        mirror.results.insert(id.into(), row);
        mirror.prune(Instant::now());
        mirror.revision += 1;
        prune_owners(&mut s);
        s.events.changed.notify_all();
        Ok(())
    }
    pub fn mirror_reject(&self, id: &str, code: ErrorCode, msg: &str) -> Result<(), String> {
        let mut s = self.lock();
        s.mirror.as_mut().ok_or("Not a mirror")?.reject(id, code, msg);
        prune_owners(&mut s);
        s.events.changed.notify_all();
        Ok(())
    }
    pub fn mirror_reset(&self, reason: &str) -> Result<(), String> {
        let mut s = self.lock();
        let mirror = s.mirror.as_mut().ok_or("Not a mirror")?;
        let mut ids: Vec<_> = mirror.queue.iter().map(|q| q.command.id.clone()).collect();
        ids.extend(
            mirror
                .results
                .values()
                .filter(|r| r.state == ResultState::Pending)
                .map(|r| r.id.clone()),
        );
        mirror.results.retain(|id, _| ids.contains(id));
        mirror.accepted.clear();
        for id in ids {
            let gateway_rev = mirror.results.get(&id).map_or(0, |r| r.rev);
            mirror.reject(&id, ErrorCode::Unknown, &format!("unknown: {reason}"));
            // Local reset does not invent a gateway revision. A later boot-bound
            // terminal result must still be able to resolve this unknown outcome.
            if let Some(row) = mirror.results.get_mut(&id) {
                row.rev = gateway_rev;
            }
        }
        mirror.revision += 1;
        mirror.watched = Watched::default();
        mirror.game = GameStatus::default();
        mirror.prune(Instant::now());
        prune_owners(&mut s);
        s.areas.clear();
        s.boot.clear();
        s.factions = None;
        s.watch.reported = None;
        s.events.mirror_clear();
        Ok(())
    }
    /// Pending local envelopes; sent commands remain until a gateway result/rejection.
    pub fn mirror_commands(&self, now_ms: u64) -> Result<Vec<MirrorCommand>, String> {
        let mut s = self.lock();
        let mirror = s.mirror.as_mut().ok_or("Not a mirror")?;
        mirror.expire(now_ms);
        Ok(mirror.queue.iter().cloned().collect())
    }
    pub fn mirror_mark_sent(&self, ids: &[String]) -> Result<(), String> {
        let mut s = self.lock();
        let mirror = s.mirror.as_mut().ok_or("Not a mirror")?;
        for q in &mut mirror.queue {
            if ids.contains(&q.command.id) {
                q.sent = true;
            }
        }
        mirror.revision += 1;
        Ok(())
    }
    pub fn mirror_set_ttl(&self, id: &str, ttl_ms: u64) -> Result<(), String> {
        let mut s = self.lock();
        let mirror = s.mirror.as_mut().ok_or("Not a mirror")?;
        let q =
            mirror.queue.iter_mut().find(|q| q.command.id == id).ok_or("Unknown queued command")?;
        if q.sent || ttl_ms == 0 || ttl_ms > COMMAND_TTL_MS {
            return Err("Invalid unsent command TTL".into());
        }
        q.ttl_ms = ttl_ms;
        mirror.revision += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reset_keeps_waiters_signal_and_cannot_open_a_journal() {
        let s = LiveStore::new_mirror();
        let signal = s.lock().events.changed.clone();
        s.mirror_reset("epoch").unwrap();
        assert!(Arc::ptr_eq(&signal, &s.lock().events.changed));
        assert!(
            s.set_command_identity(std::path::Path::new("unused-mirror-journal"), "server")
                .is_err()
        );
    }
    #[test]
    fn audit_m4_mirror_results_and_fingerprints_are_bounded_and_reset_releases_payloads() {
        let s = LiveStore::new_mirror();
        s.set_id_prefix("test.").unwrap();
        s.mirror_meta(
            "boot".into(),
            None,
            Watched::default(),
            Policy::default(),
            GameStatus::default(),
        )
        .unwrap();
        let mut first = String::new();
        for n in 0..4200 {
            let command = serde_json::from_value(json!({"op":"set_variable","area":"a","target":"1","name":"value","type":"int","value":n})).unwrap();
            let id = s.enqueue(command, 0).unwrap();
            if n == 0 {
                first = id.clone();
            }
            s.mirror_result(
                &id,
                1,
                ResultState::Final,
                Some(CommandResult { id: id.clone(), ok: true, ..Default::default() }),
            )
            .unwrap();
        }
        let guard = s.lock();
        let m = guard.mirror.as_ref().unwrap();
        assert!(m.results.len() <= 256);
        assert!(m.accepted.len() <= 4096);
        assert!(guard.owners.len() <= 4096);
        assert!(m.result(&first).is_none());
        drop(guard);
        s.mirror_reset("epoch").unwrap();
        let guard = s.lock();
        let m = guard.mirror.as_ref().unwrap();
        assert!(m.accepted.is_empty());
        assert!(m.results.is_empty());
        assert!(guard.owners.is_empty());
    }
    #[test]
    fn audit_m4_payload_budgets_retention_and_retired_ids() {
        let s = LiveStore::new_mirror();
        s.set_id_prefix("test.").unwrap();
        s.mirror_meta(
            "boot".into(),
            None,
            Watched::default(),
            Policy { scripts: true, ..Policy::default() },
            GameStatus::default(),
        )
        .unwrap();
        let command: Command = serde_json::from_value(json!({"op":"spawn_json","area":"a","json":{"__data_type":"UTC ","Tag":{"type":"cexostring","value":"x".repeat(120_000)}},"x":0,"y":0,"z":0,"f":0})).unwrap();
        let mut blocked = false;
        for _ in 0..100 {
            if s.enqueue(command.clone(), 0).is_err() {
                blocked = true;
                break;
            }
        }
        assert!(blocked);
        assert!(s.mirror_commands(0).unwrap().len() < 100);
        let first = s.mirror_commands(0).unwrap()[0].command.clone();
        s.mirror_reset("epoch").unwrap();
        s.mirror_meta(
            "new".into(),
            None,
            Watched::default(),
            Policy { scripts: true, ..Policy::default() },
            GameStatus::default(),
        )
        .unwrap();
        assert!(s.enqueue(first, 0).is_err(), "reset must not make old IDs runnable again");
        for n in 0..30 {
            let id = format!("result-{n}");
            let r: CommandResult = serde_json::from_value(
                json!({"id":id,"ok":true,"object_json":{"payload":"x".repeat(300_000)}}),
            )
            .unwrap();
            s.mirror_result(&id, 1, ResultState::Final, Some(r)).unwrap();
        }
        let mut guard = s.lock();
        let m = guard.mirror.as_mut().unwrap();
        assert!(m.results.len() < 30);
        assert!(
            m.results.values().map(|r| serde_json::to_vec(r).unwrap().len()).sum::<usize>()
                <= BYTE_BUDGET
        );
        m.prune(Instant::now() + RETENTION);
        assert!(m.results.is_empty());
        assert!(m.accepted.is_empty());
    }
}
