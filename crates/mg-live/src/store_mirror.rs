//! Passive finished-state replication. No game wire interpretation happens here.
use super::*;
use crate::events::Event;
use std::collections::BTreeMap;
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
    results: BTreeMap<String, MirroredResult>,
    accepted: HashMap<String, Command>,
    prefix: String,
    watched: Watched,
    pub game: GameStatus,
}
impl Mirror {
    pub(super) fn result(&self, id: &str) -> Option<CommandResult> {
        self.results.get(id).and_then(|row| row.result.clone())
    }
    pub(super) fn expire(&mut self, now_ms: u64) {
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
    }
}

pub(super) fn enqueue(
    s: &mut State,
    mut command: Command,
    now_ms: u64,
    _scope: &crate::access::Scope,
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
    if let Some(old) = mirror.accepted.get(&command.id) {
        return if old == &command {
            Ok(command.id)
        } else {
            Err("Command id already belongs to another operation".into())
        };
    }
    if mirror.queue.len() >= MAX_QUEUE {
        return Err("Too many queued mirror commands".into());
    }
    let id = command.id.clone();
    mirror.accepted.insert(id.clone(), command.clone());
    mirror.queue.push_back(MirrorCommand {
        command,
        boot: s.boot.clone(),
        ttl_ms: COMMAND_TTL_MS,
        sent: false,
        queued_ms: now_ms,
    });
    mirror.revision += 1;
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
        let mut s = self.lock();
        let mirror = s.mirror.as_mut().ok_or("Not a mirror")?;
        if mirror
            .results
            .get(id)
            .is_some_and(|old| old.rev >= rev || old.state == ResultState::Final)
        {
            return Ok(());
        }
        mirror.queue.retain(|q| q.command.id != id);
        mirror.results.insert(id.into(), MirroredResult { id: id.into(), rev, state, result });
        mirror.revision += 1;
        s.events.changed.notify_all();
        Ok(())
    }
    pub fn mirror_reject(&self, id: &str, code: ErrorCode, msg: &str) -> Result<(), String> {
        let mut s = self.lock();
        s.mirror.as_mut().ok_or("Not a mirror")?.reject(id, code, msg);
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
        for id in ids {
            mirror.reject(&id, ErrorCode::Unknown, &format!("unknown: {reason}"));
        }
        mirror.revision += 1;
        mirror.watched = Watched::default();
        mirror.game = GameStatus::default();
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
}
