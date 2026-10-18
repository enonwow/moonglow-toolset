//! Session views of the authoritative store. No network or authentication provider.
use super::mirror::{GameStatus, MirroredResult, ResultState, Watched};
use super::*;
use crate::{
    access::{ClientGroup, DeliveryMode, Scope},
    events::Event,
};
use std::collections::{BTreeMap, BTreeSet};

pub const RESULT_RETENTION_MS: u64 = 600_000;
pub const RESULT_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Meta {
    pub boot: String,
    pub rev: u64,
    pub factions: Option<Vec<ServerFaction>>,
    pub watched: Watched,
    pub policy: Policy,
    pub game: GameStatus,
    pub redis: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Area {
    pub boot: String,
    pub area: String,
    pub rev: u64,
    pub age_ms: u64,
    #[serde(with = "area_wire")]
    pub live: AreaLive,
}
// received_ms is local to each process; full_ms remains the authoritative baseline identity.
mod area_wire {
    use super::*;
    pub(super) fn serialize<S: serde::Serializer>(
        live: &AreaLive,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut value = serde_json::to_value(live).map_err(serde::ser::Error::custom)?;
        value.as_object_mut().expect("AreaLive object").remove("received_ms");
        value.serialize(serializer)
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<AreaLive, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        value
            .as_object_mut()
            .ok_or_else(|| serde::de::Error::custom("Expected area object"))?
            .insert("received_ms".into(), json!(0));
        serde_json::from_value(value).map_err(serde::de::Error::custom)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Events {
    pub rows: Vec<Event>,
    pub through: u64,
    pub lost: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "topic", content = "body", rename_all = "snake_case")]
pub enum Push {
    Meta(Meta),
    Area(Area),
    AreaGone { boot: String, area: String },
    Boot { boot: String, prev: String },
    Events(Events),
    Result(MirroredResult),
    Drafts { rev: u64, items: Vec<Value> },
    Presence { rev: u64, actors: Vec<Value>, focus: BTreeMap<String, Vec<String>> },
    Book { kind: String, key: String, rev: u64 },
    Session { groups: Vec<String>, policy: Policy, limits: Value },
    Notice { level: String, text: String },
}
impl Push {
    /// LinkService handles non-store topics; true means this push changed the mirror.
    pub fn apply(&self, mirror: &LiveStore, now_ms: u64) -> Result<bool, String> {
        match self {
            Self::Meta(m) => mirror.mirror_meta(
                m.boot.clone(),
                m.factions.clone(),
                m.watched.clone(),
                m.policy,
                m.game.clone(),
            )?,
            Self::Area(a) => {
                if mirror.boot() != a.boot {
                    return Err("Area boot differs from mirror".into());
                }
                mirror.mirror_area(&a.area, a.live.clone(), now_ms.saturating_sub(a.age_ms))?;
            }
            Self::AreaGone { boot, area } => {
                if mirror.boot() != *boot {
                    return Err("Area boot differs from mirror".into());
                }
                mirror.mirror_area_gone(area)?;
            }
            Self::Boot { .. } => mirror.mirror_reset("boot")?,
            Self::Events(e) => mirror.mirror_events(e.rows.clone(), e.through)?,
            Self::Result(r) => mirror.mirror_result(&r.id, r.rev, r.state, r.result.clone())?,
            _ => return Ok(false),
        };
        Ok(true)
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Resume {
    pub gw_epoch: String,
    pub boot: String,
    pub event_n: u64,
    pub areas: BTreeMap<String, u64>,
}
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub id: String,
    pub id_prefix: String,
    pub groups: Vec<String>,
    pub watch_limit: usize,
}
impl SessionConfig {
    fn scope(&self) -> Scope {
        let mut groups = Vec::new();
        for group in &self.groups {
            if let Some(g) = match group.as_str() {
                "read" => Some(ClientGroup::Read),
                "read_chat" => Some(ClientGroup::ReadChat),
                "variables" => Some(ClientGroup::Variables),
                "npc" => Some(ClientGroup::Npc),
                "speech" => Some(ClientGroup::Speech),
                "spawn" => Some(ClientGroup::Spawn),
                "spawn_scripted" => Some(ClientGroup::SpawnScripted),
                "scripts" => Some(ClientGroup::Scripts),
                "chunks" => Some(ClientGroup::Chunks),
                "players" => Some(ClientGroup::Players),
                _ => None,
            } {
                groups.push(g);
            }
        }
        Scope::restricted(&self.id, groups, DeliveryMode::Auto)
    }
    fn has(&self, group: &str) -> bool {
        self.groups.iter().any(|g| g == group)
    }
}
#[derive(Debug)]
struct Outbox {
    reservation: u64,
    row: MirroredResult,
    created_ms: u64,
    acked: u64,
    sent: u64,
    boot: String,
    ttl_ms: u64,
}
#[derive(Debug)]
struct Session {
    generation: u64,
    config: SessionConfig,
    watch: BTreeSet<String>,
    focus: Vec<(String, String)>,
    boot: String,
    areas: BTreeMap<String, u64>,
    area_sent: BTreeMap<String, u64>,
    event_n: u64,
    event_ack: u64,
    outbox: BTreeMap<String, Outbox>,
    completed: BTreeMap<String, u64>,
    meta: Option<Meta>,
    resume_valid: bool,
    auxiliary: VecDeque<Push>,
}
impl Session {
    fn prune_completed(&mut self, now_ms: Option<u64>) {
        if let Some(now) = now_ms {
            self.completed.retain(|_, at| now.saturating_sub(*at) < RESULT_RETENTION_MS);
        }
        while self.completed.len() > 4096 {
            let id = self.completed.iter().min_by_key(|(_, at)| *at).unwrap().0.clone();
            self.completed.remove(&id);
        }
    }
}
#[derive(Debug, Default)]
pub(super) struct GatewayState {
    watch_stamp: Option<WatchStamp>,
    #[cfg(test)]
    watch_recomputations: std::sync::atomic::AtomicU64,
    generation: u64,
    reservation: u64,
    pub area_revs: BTreeMap<String, u64>,
    meta: Option<Meta>,
    sessions: BTreeMap<String, Session>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchStamp {
    generation: u64,
    boot: String,
    seq: u64,
    owners: BTreeMap<String, BTreeSet<String>>,
}
impl WatchStamp {
    fn new(s: &State) -> Self {
        Self {
            generation: s.gateway.generation,
            boot: s.boot.clone(),
            seq: s.seq,
            owners: s
                .watch
                .owners
                .iter()
                .filter(|(id, _)| id.as_str() != "gateway")
                .map(|(id, areas)| (id.clone(), areas.clone()))
                .collect(),
        }
    }
}
/// Captured independently of outbox retention, checked under the store accept lock.
#[derive(Debug, Clone)]
pub(crate) struct Acceptance {
    command_id: String,
    generation: u64,
    boot: String,
    groups: Vec<String>,
}
pub(super) fn validate_accept(s: &State, scope: &Scope) -> Result<(), CommandError> {
    let Some(binding) = &scope.gateway_accept else {
        return Ok(());
    };
    if binding.boot != s.boot || binding.boot.is_empty() {
        return Err(CommandError::new(ErrorCode::Expired, "Game boot changed before acceptance"));
    }
    if !s.gateway.sessions.get(scope.name()).is_some_and(|session| {
        session.generation == binding.generation && session.config.groups == binding.groups
    }) {
        return Err(CommandError::new(
            ErrorCode::Forbidden,
            "Gateway session changed before acceptance",
        ));
    }
    if !s.accepted.contains_key(&binding.command_id)
        && !s.gateway.sessions[scope.name()].outbox.contains_key(&binding.command_id)
    {
        return Err(CommandError::new(
            ErrorCode::Expired,
            "Command reservation expired before acceptance",
        ));
    }
    Ok(())
}
pub(super) fn command_binding<'a>(g: &'a GatewayState, id: &str) -> Option<(&'a str, u64)> {
    g.sessions.values().find_map(|s| s.outbox.get(id)).map(|o| (o.boot.as_str(), o.ttl_ms))
}
fn result_state(r: &CommandResult) -> ResultState {
    if r.code == Some(ErrorCode::Unknown) {
        ResultState::Unknown
    } else if r.pending {
        ResultState::Pending
    } else {
        ResultState::Final
    }
}
pub(super) fn record_result(g: &mut GatewayState, r: &CommandResult) {
    for session in g.sessions.values_mut() {
        if let Some(out) = session.outbox.get_mut(&r.id)
            && out.row.state != ResultState::Final
            && out.row.result.as_ref() != Some(r)
        {
            out.row.rev += 1;
            out.row.state = result_state(r);
            out.row.result = Some(r.clone());
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rejected {
    pub id: String,
    pub code: String,
    pub msg: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CommandsReply {
    pub accepted: Vec<String>,
    pub rejected: Vec<Rejected>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatchReply {
    pub accepted: Vec<String>,
    pub waiting: Vec<String>,
}

#[derive(Debug)]
pub struct Gateway {
    store: Arc<LiveStore>,
    epoch: String,
    watch_update: Mutex<()>,
    tick_at: Mutex<Option<u64>>,
}
impl Gateway {
    pub fn new(store: Arc<LiveStore>) -> Result<Self, String> {
        if store.is_mirror() {
            return Err("Gateway requires an authoritative store".into());
        }
        let epoch = crate::service::generate_token()?;
        // A new gateway epoch cannot resume old session delivery state.
        {
            let mut s = store.lock();
            s.gateway.sessions.clear();
            s.gateway.generation += 1;
            s.gateway.watch_stamp = None;
        }
        Ok(Self { store, epoch, watch_update: Mutex::new(()), tick_at: Mutex::new(None) })
    }
    /// Dispatch public core requests. Host-owned book/AI operations return unsupported.
    pub fn handle(
        &self,
        session: &str,
        request: crate::gateway_frames::Request,
        now_ms: u64,
    ) -> crate::gateway_frames::Response {
        use crate::gateway_frames::{FrameError, Response};
        let result = (|| -> Result<Value, FrameError> {
            let invalid = |e: String| FrameError::new("invalid", e);
            let config = self
                .store
                .lock()
                .gateway
                .sessions
                .get(session)
                .ok_or_else(|| FrameError::new("forbidden", "Unknown session"))?
                .config
                .clone();
            let a = request.a;
            match request.op.as_str() {
                "cmds" => {
                    let commands = a["commands"]
                        .as_array()
                        .ok_or_else(|| invalid("Expected commands array".into()))?;
                    if commands.len() > 256 {
                        return Err(invalid("Too many commands".into()));
                    }
                    Ok(serde_json::to_value(
                        self.commands(session, commands, now_ms).map_err(invalid)?,
                    )
                    .unwrap())
                }
                "watch.set" => {
                    if !config.has("read") {
                        return Err(FrameError::new("forbidden", "Read permission required"));
                    }
                    let areas: Vec<String> = serde_json::from_value(a["areas"].clone())
                        .map_err(|e| invalid(e.to_string()))?;
                    Ok(serde_json::to_value(
                        self.watch_set(session, &areas, now_ms).map_err(invalid)?,
                    )
                    .unwrap())
                }
                "focus.set" => {
                    let objects = a["objects"]
                        .as_array()
                        .ok_or_else(|| invalid("Expected objects array".into()))?;
                    let objects = objects
                        .iter()
                        .map(|o| {
                            Ok((
                                o["area"]
                                    .as_str()
                                    .ok_or_else(|| invalid("Missing focus area".into()))?
                                    .to_string(),
                                o["id"]
                                    .as_str()
                                    .ok_or_else(|| invalid("Missing focus ID".into()))?
                                    .to_string(),
                            ))
                        })
                        .collect::<Result<Vec<_>, FrameError>>()?;
                    self.focus_set(session, objects).map_err(invalid)?;
                    Ok(json!({}))
                }
                "events.since" => {
                    let n =
                        a["n"].as_u64().ok_or_else(|| invalid("Invalid event cursor".into()))?;
                    let limit = a["limit"].as_u64().unwrap_or(100).clamp(1, 500) as usize;
                    let kinds: Vec<String> =
                        serde_json::from_value(a.get("kinds").cloned().unwrap_or(json!([])))
                            .map_err(|e| invalid(e.to_string()))?;
                    let s = self.store.lock();
                    let mut rows: Vec<_> = s
                        .events
                        .log_rows(n)
                        .into_iter()
                        .filter(|e| {
                            visible_event(e, &config)
                                && (kinds.is_empty() || kinds.contains(&e.kind))
                        })
                        .collect();
                    let through = if rows.len() > limit {
                        rows.truncate(limit);
                        rows.last().unwrap().n
                    } else {
                        s.events.last()
                    };
                    Ok(serde_json::to_value(Events {
                        rows,
                        through,
                        lost: lost_since(&s.events, n),
                    })
                    .unwrap())
                }
                "call" => {
                    let op = a["op"]
                        .as_str()
                        .unwrap_or_default()
                        .strip_prefix("live_")
                        .unwrap_or(a["op"].as_str().unwrap_or_default());
                    if !matches!(
                        op,
                        "find"
                            | "status"
                            | "npc_card"
                            | "gm_card"
                            | "npc_memory"
                            | "area_notes"
                            | "events"
                            | "area_state"
                    ) {
                        return Err(FrameError::new(
                            "unsupported",
                            "Call is not a read-only gateway operation",
                        ));
                    }
                    let input = a.get("input").cloned().unwrap_or(json!({}));
                    if input.get("wait_ms").is_some_and(|v| v.as_u64() != Some(0)) {
                        return Err(invalid("Waiting calls are not supported".into()));
                    }
                    // Route events through the gateway filter, including tell permission.
                    if op == "events" {
                        let s = self.store.lock();
                        let since = input["since"].as_u64().unwrap_or(0);
                        let q = crate::events::Query {
                            since,
                            area: input["area"].as_str().map(str::to_owned),
                            kinds: serde_json::from_value(
                                input.get("kinds").cloned().unwrap_or(json!([])),
                            )
                            .map_err(|e| invalid(e.to_string()))?,
                            object: input["object"].as_str().map(str::to_owned),
                            text: input["text"].as_str().map(str::to_owned),
                            limit: input["limit"].as_u64().unwrap_or(100).min(500) as usize,
                        };
                        return Ok(
                            json!({"events":s.events.query_filtered(&q,|e|visible_event(e,&config)),"last":s.events.last(),"boot":s.boot}),
                        );
                    }
                    crate::operations::call_scoped(
                        &format!("live_{op}"),
                        input,
                        &self.store,
                        &std::sync::atomic::AtomicBool::new(false),
                        &config.scope(),
                    )
                    .map_err(|e| {
                        FrameError::new(
                            serde_json::to_value(e.code).unwrap().as_str().unwrap(),
                            e.error,
                        )
                    })
                }
                _ => Err(FrameError::new("unsupported", "Unknown or host-owned operation")),
            }
        })();
        Response { id: request.id, result }
    }
    pub fn epoch(&self) -> &str {
        &self.epoch
    }
    pub fn store(&self) -> &Arc<LiveStore> {
        &self.store
    }
    pub fn connect(
        &self,
        config: SessionConfig,
        resume: Option<&Resume>,
        watch: &[String],
        now_ms: u64,
    ) -> Result<bool, String> {
        if config.id.is_empty() || config.id_prefix.is_empty() || !config.id_prefix.ends_with('.') {
            return Err("Invalid session identity".into());
        }
        let watches = validate_watch(watch, config.watch_limit)?;
        let mut s = self.store.lock();
        if s.gateway
            .sessions
            .iter()
            .any(|(id, session)| id != &config.id && session.config.id_prefix == config.id_prefix)
        {
            return Err("Session prefix already in use".into());
        }
        let retained = resume.is_some_and(|r| {
            r.gw_epoch == self.epoch
                && r.boot == s.boot
                && r.event_n <= s.events.last()
                && lost_since(&s.events, r.event_n) == 0
                && r.areas.iter().all(|(area, rev)| {
                    s.gateway.area_revs.get(area).is_some_and(|current| rev <= current)
                })
        });
        s.gateway.generation += 1;
        let generation = s.gateway.generation;
        let old = s.gateway.sessions.remove(&config.id);
        let resumed = retained
            && old.as_ref().is_some_and(|old| {
                old.resume_valid
                    && old.config.groups == config.groups
                    && old.config.id_prefix == config.id_prefix
                    && old
                        .outbox
                        .values()
                        .all(|o| now_ms.saturating_sub(o.created_ms) < RESULT_RETENTION_MS)
            });
        let mut session = old.unwrap_or_else(|| Session {
            generation,
            config: config.clone(),
            watch: BTreeSet::new(),
            focus: vec![],
            boot: s.boot.clone(),
            areas: BTreeMap::new(),
            area_sent: BTreeMap::new(),
            event_n: 0,
            event_ack: 0,
            outbox: BTreeMap::new(),
            completed: BTreeMap::new(),
            meta: None,
            resume_valid: true,
            auxiliary: VecDeque::new(),
        });
        session.generation = generation;
        session.prune_completed(Some(now_ms));
        session.config = config;
        session.watch = watches;
        session.meta = None;
        session.area_sent.clear();
        session.boot = s.boot.clone();
        if resumed {
            let r = resume.unwrap();
            session.areas = r.areas.clone();
            session.event_n = r.event_n;
        } else {
            session.areas.clear();
            session.event_n = 0;
            session.event_ack = 0;
        }
        for out in session.outbox.values_mut() {
            out.sent = 0;
        }
        session.resume_valid = true;
        s.gateway.sessions.insert(session.config.id.clone(), session);
        drop(s);
        self.update_watch(now_ms)?;
        Ok(resumed)
    }
    pub fn disconnect(&self, id: &str, now_ms: u64) -> Result<(), String> {
        // Retain session outbox for resume; dropping a link does not discard its watches.
        if !self.store.lock().gateway.sessions.contains_key(id) {
            return Err("Unknown session".into());
        }
        let _ = now_ms;
        Ok(())
    }
    pub fn remove_session(&self, id: &str, now_ms: u64) -> Result<(), String> {
        {
            let mut s = self.store.lock();
            if s.gateway.sessions.remove(id).is_some() {
                s.gateway.generation += 1;
            }
        }
        self.update_watch(now_ms)
    }
    fn update_watch(&self, now_ms: u64) -> Result<(), String> {
        let _update = self.watch_update.lock().unwrap_or_else(|e| e.into_inner());
        let (active, stamp) = {
            let s = self.store.lock();
            let stamp = WatchStamp::new(&s);
            if s.gateway.watch_stamp.as_ref() == Some(&stamp) {
                return Ok(());
            }
            #[cfg(test)]
            {
                s.gateway.watch_recomputations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            (active_watch(&s), stamp)
        };
        self.store.set_watched_areas_owned(
            "gateway",
            &active.iter().cloned().collect::<Vec<_>>(),
            now_ms,
        )?;
        self.store.lock().gateway.watch_stamp = Some(stamp);
        Ok(())
    }
    /// Host maintenance, independent of the number of session pumps.
    pub fn tick(&self, now_ms: u64) -> Result<(), String> {
        let mut last = self.tick_at.lock().unwrap_or_else(|e| e.into_inner());
        if *last == Some(now_ms) {
            return Ok(());
        }
        *last = Some(now_ms);
        self.store.expire(now_ms);
        self.update_watch(now_ms)
    }
    pub fn watch_set(&self, id: &str, areas: &[String], now_ms: u64) -> Result<WatchReply, String> {
        let mut s = self.store.lock();
        let session = s.gateway.sessions.get_mut(id).ok_or("Unknown session")?;
        let next = validate_watch(areas, session.config.watch_limit)?;
        let changed = next != session.watch;
        session.watch = next;
        let desired = session.watch.clone();
        if changed {
            s.gateway.generation += 1;
        }
        let active = active_watch(&s);
        drop(s);
        self.update_watch(now_ms)?;
        Ok(WatchReply {
            accepted: desired.intersection(&active).cloned().collect(),
            waiting: desired.difference(&active).cloned().collect(),
        })
    }
    pub fn focus_set(&self, id: &str, objects: Vec<(String, String)>) -> Result<(), String> {
        if objects.len() > 256 {
            return Err("Too many focus objects".into());
        }
        validate_watch(&objects.iter().map(|(area, _)| area.clone()).collect::<Vec<_>>(), 256)?;
        if objects.iter().any(|(_, id)| {
            id.is_empty() || id.len() > 16 || !id.bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err("Invalid focus object ID".into());
        }
        let mut s = self.store.lock();
        let session = s.gateway.sessions.get_mut(id).ok_or("Unknown session")?;
        if !session.config.has("focus") {
            return Err("Focus permission required".into());
        }
        let changed = session.focus != objects;
        session.focus = objects;
        if changed {
            s.gateway.generation += 1;
        }
        drop(s);
        self.update_watch(crate::now_ms())
    }
    pub fn commands(
        &self,
        id: &str,
        commands: &[Value],
        now_ms: u64,
    ) -> Result<CommandsReply, String> {
        let (config, generation) = {
            let s = self.store.lock();
            let session = s.gateway.sessions.get(id).ok_or("Unknown session")?;
            (session.config.clone(), session.generation)
        };
        let mut scope = config.scope();
        scope.gateway_accept = Some(Acceptance {
            command_id: String::new(),
            generation,
            boot: String::new(),
            groups: config.groups.clone(),
        });
        let mut reply = CommandsReply::default();
        for envelope in commands {
            let command_id = envelope["id"].as_str().unwrap_or_default().to_string();
            let outcome = self.accept_command(id, &config, &scope, envelope.clone(), now_ms);
            match outcome {
                Ok(()) => reply.accepted.push(command_id),
                Err((code, msg)) => reply.rejected.push(Rejected { id: command_id, code, msg }),
            }
        }
        Ok(reply)
    }
    fn accept_command(
        &self,
        id: &str,
        config: &SessionConfig,
        scope: &Scope,
        mut envelope: Value,
        now_ms: u64,
    ) -> Result<(), (String, String)> {
        let reject = |code: &str, msg: &str| (code.to_owned(), msg.to_owned());
        let object =
            envelope.as_object_mut().ok_or_else(|| reject("invalid", "Expected command object"))?;
        let boot = object
            .remove("boot")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| reject("expired", "Missing command boot"))?;
        let ttl = object
            .remove("ttl_ms")
            .unwrap_or(json!(COMMAND_TTL_MS))
            .as_u64()
            .filter(|v| *v > 0 && *v <= COMMAND_TTL_MS)
            .ok_or_else(|| reject("invalid", "TTL must be 1..30000 ms"))?;
        let command_id = object.get("id").and_then(Value::as_str).unwrap_or_default().to_owned();
        if !valid_id(&command_id, &config.id_prefix) {
            return Err(reject("invalid", "Command ID does not match session prefix"));
        }
        let op = object.get("op").and_then(Value::as_str).unwrap_or_default();
        if matches!(op, "watch_areas" | "resync") {
            return Err(reject("forbidden", "Control commands are gateway-owned"));
        }
        let mut scope = scope.clone();
        let binding = scope.gateway_accept.as_mut().expect("gateway scope");
        binding.boot = boot.clone();
        binding.command_id = command_id.clone();
        let mut s = self.store.lock();
        if !s.gateway.sessions.get(id).is_some_and(|session| {
            session.generation == scope.gateway_accept.as_ref().unwrap().generation
        }) {
            return Err(reject("forbidden", "Gateway session changed before reservation"));
        }
        if boot != s.boot || boot.is_empty() {
            return Err(reject("expired", "Game boot changed"));
        }
        if let Some(old) = s.accepted.get(&command_id) {
            let mut parsed: Command = serde_json::from_value(envelope.clone())
                .map_err(|e| reject("invalid", &e.to_string()))?;
            parsed.area.make_ascii_lowercase();
            if old != &command_fingerprint(&parsed) {
                return Err(reject("conflict", "ID belongs to another command"));
            }
        }
        s.gateway.reservation += 1;
        let reservation = s.gateway.reservation;
        let session =
            s.gateway.sessions.get_mut(id).ok_or_else(|| reject("invalid", "Unknown session"))?;
        session.prune_completed(Some(now_ms));
        let inserted = !session.outbox.contains_key(&command_id)
            && !session.completed.contains_key(&command_id);
        if inserted {
            if session.outbox.len() >= RESULT_CAPACITY {
                return Err(reject("rate_limited", "Result outbox is full; acknowledge results"));
            }
            session.outbox.insert(
                command_id.clone(),
                Outbox {
                    reservation,
                    row: MirroredResult {
                        id: command_id.clone(),
                        rev: 1,
                        state: ResultState::Pending,
                        result: None,
                    },
                    created_ms: now_ms,
                    acked: 0,
                    sent: 0,
                    boot: boot.clone(),
                    ttl_ms: ttl,
                },
            );
        }
        drop(s);
        match crate::operations::command_scoped(envelope, &self.store, now_ms, &scope) {
            Ok(_) => {
                let mut s = self.store.lock();
                if let Some((_, r)) = s.results.iter().find(|(_, r)| r.id == command_id).cloned() {
                    record_result(&mut s.gateway, &r);
                }
                Ok(())
            }
            Err(error) => {
                if inserted {
                    let mut s = self.store.lock();
                    if let Some(session) = s.gateway.sessions.get_mut(id)
                        && session.generation == scope.gateway_accept.as_ref().unwrap().generation
                        && session
                            .outbox
                            .get(&command_id)
                            .is_some_and(|out| out.reservation == reservation)
                    {
                        session.outbox.remove(&command_id);
                    }
                }
                Err((
                    serde_json::to_value(error.code).unwrap().as_str().unwrap().into(),
                    error.error,
                ))
            }
        }
    }
    /// Host supplies already authorized drafts, presence, book invalidations and session updates.
    pub fn publish_session(&self, id: &str, push: Push) -> Result<(), String> {
        if !matches!(
            push,
            Push::Drafts { .. }
                | Push::Presence { .. }
                | Push::Book { .. }
                | Push::Session { .. }
                | Push::Notice { .. }
        ) {
            return Err("State pushes come from the authoritative store".into());
        }
        let mut s = self.store.lock();
        let session = s.gateway.sessions.get_mut(id).ok_or("Unknown session")?;
        if session.auxiliary.len() >= 256 {
            return Err("Session push queue is full".into());
        }
        session.auxiliary.push_back(push);
        Ok(())
    }
    pub fn ack(&self, id: &str, results: &[(String, u64)], event_n: u64) -> Result<(), String> {
        let mut s = self.store.lock();
        let session = s.gateway.sessions.get_mut(id).ok_or("Unknown session")?;
        if event_n > session.event_n {
            return Err("Acknowledgement is ahead of delivered events".into());
        }
        session.event_ack = session.event_ack.max(event_n);
        for (id, rev) in results {
            if let Some(out) = session.outbox.get_mut(id)
                && *rev == out.row.rev
                && *rev <= out.sent
            {
                out.acked = *rev;
            }
        }
        session.outbox.retain(|id, o| {
            let complete = o.acked == o.row.rev && o.row.state == ResultState::Final;
            if complete {
                session.completed.insert(id.clone(), o.created_ms);
            }
            !complete
        });
        session.prune_completed(None);
        Ok(())
    }
    pub fn pushes(&self, id: &str, now_ms: u64) -> Result<Vec<Push>, String> {
        let mut s = self.store.lock();
        let mut session = s.gateway.sessions.remove(id).ok_or("Unknown session")?;
        session.prune_completed(Some(now_ms));
        let mut out = Vec::new();
        if session.boot != s.boot {
            out.push(Push::Boot { boot: s.boot.clone(), prev: session.boot.clone() });
            session.boot = s.boot.clone();
            session.areas.clear();
            session.area_sent.clear();
            session.event_n = 0;
        }
        let mut meta = Meta {
            boot: s.boot.clone(),
            rev: s.gateway.meta.as_ref().map_or(0, |m| m.rev),
            factions: s.factions.clone(),
            watched: Watched {
                desired: s.watch.desired.clone().unwrap_or_default().into_iter().collect(),
                reported: s.watch.reported.clone().unwrap_or_default(),
            },
            policy: s.policy,
            game: GameStatus {
                connected: !s.boot.is_empty(),
                age_ms: now_ms.saturating_sub(s.received_ms),
            },
            redis: s.redis.as_ref().map(|r| crate::bridge::status(r)).unwrap_or(Value::Null),
        };
        if s.gateway.meta.as_ref() != Some(&meta) {
            meta.rev += 1;
            s.gateway.meta = Some(meta.clone());
        }
        if session.meta.as_ref() != Some(&meta) {
            session.meta = Some(meta.clone());
            out.push(Push::Meta(meta));
        }
        let visible: BTreeSet<_> = s
            .areas
            .iter()
            .filter(|(area, live)| {
                session.config.has("read")
                    && (session.watch.contains(*area)
                        || live.creatures.iter().any(|o| o.pc)
                        || session.focus.iter().any(|(_, id)| live.find(id).is_some()))
            })
            .map(|(a, _)| a.clone())
            .collect();
        for gone in
            session.areas.keys().filter(|a| !visible.contains(*a)).cloned().collect::<Vec<_>>()
        {
            session.areas.remove(&gone);
            out.push(Push::AreaGone { boot: s.boot.clone(), area: gone });
        }
        for area in visible {
            let rev = s.gateway.area_revs.get(&area).copied().unwrap_or(0);
            if session.areas.get(&area) == Some(&rev)
                || session
                    .area_sent
                    .get(&area)
                    .is_some_and(|last| now_ms.saturating_sub(*last) < 250)
            {
                continue;
            }
            let live = s.areas[&area].clone();
            out.push(Push::Area(Area {
                boot: s.boot.clone(),
                area: area.clone(),
                rev,
                age_ms: now_ms.saturating_sub(live.received_ms),
                live,
            }));
            session.areas.insert(area.clone(), rev);
            session.area_sent.insert(area, now_ms);
        }
        session.outbox.retain(|_, item| {
            if now_ms.saturating_sub(item.created_ms) >= RESULT_RETENTION_MS {
                session.resume_valid = false;
                session.completed.insert(item.row.id.clone(), item.created_ms);
                if item.row.state != ResultState::Final {
                    let mut row = item.row.clone();
                    row.rev += 1;
                    row.state = ResultState::Unknown;
                    row.result = Some(failure(
                        &row.id,
                        ErrorCode::Unknown,
                        "unknown: result retention elapsed",
                    ));
                    out.push(Push::Result(row));
                }
                false
            } else {
                if item.sent < item.row.rev {
                    item.sent = item.row.rev;
                    out.push(Push::Result(item.row.clone()));
                }
                true
            }
        });
        session.prune_completed(Some(now_ms));
        if s.events.last() > session.event_n {
            let events = event_batch(&s.events, session.event_n, &session.config);
            session.event_n = events.through;
            out.push(Push::Events(events));
        }
        out.extend(session.auxiliary.drain(..));
        s.gateway.sessions.insert(id.into(), session);
        Ok(out)
    }
}
fn event_batch(log: &crate::events::EventLog, since: u64, config: &SessionConfig) -> Events {
    const BUDGET: usize = 1024 * 1024;
    let mut rows = Vec::new();
    let mut bytes = 128; // Envelope, cursor/lost digits and array punctuation.
    let mut through = since;
    let mut retained = 0;
    let mut oversized = 0;
    let mut complete = true;
    for event in log.log_rows(since) {
        let visible = visible_event(&event, config);
        let size = if visible {
            serde_json::to_vec(&event).expect("event serializes").len() + 1
        } else {
            0
        };
        if visible && !rows.is_empty() && (rows.len() >= 256 || bytes + size > BUDGET) {
            complete = false;
            break;
        }
        through = event.n;
        retained += 1;
        if visible {
            if size + 128 > BUDGET {
                oversized += 1;
                continue;
            }
            bytes += size;
            rows.push(event);
        }
    }
    if complete {
        through = log.last();
    }
    Events {
        rows,
        through,
        lost: through.saturating_sub(since).saturating_sub(retained) + oversized,
    }
}
fn active_watch(s: &State) -> BTreeSet<String> {
    let wanted: BTreeSet<_> = s
        .gateway
        .sessions
        .values()
        .flat_map(|session| {
            session
                .watch
                .iter()
                .cloned()
                .chain(session.focus.iter().map(|(area, _)| area.to_ascii_lowercase()))
        })
        .collect();
    let mut reserved: BTreeSet<_> = s
        .areas
        .iter()
        .filter(|(_, a)| a.creatures.iter().any(|o| o.pc))
        .map(|(name, _)| name.clone())
        .collect();
    reserved.extend(
        s.watch
            .owners
            .iter()
            .filter(|(owner, _)| owner.as_str() != "gateway")
            .flat_map(|(_, areas)| areas.iter().cloned()),
    );
    let mut active = BTreeSet::new();
    for area in wanted {
        if reserved.contains(&area) || reserved.len() < MAX_AREAS {
            reserved.insert(area.clone());
            active.insert(area);
        }
    }
    active
}
fn validate_watch(areas: &[String], limit: usize) -> Result<BTreeSet<String>, String> {
    if areas.len() > limit
        || areas.iter().any(|a| {
            a.is_empty()
                || a.len() > 16
                || !a.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
    {
        return Err("Invalid watch set".into());
    }
    Ok(areas.iter().map(|a| a.to_ascii_lowercase()).collect())
}
fn valid_id(id: &str, prefix: &str) -> bool {
    let Some(tail) = id.strip_prefix(prefix) else { return false };
    let Some((process, counter)) = tail.split_once('.') else { return false };
    id.len() <= 64
        && process.len() == 8
        && process.bytes().all(|b| b.is_ascii_hexdigit())
        && !counter.is_empty()
        && counter.bytes().all(|b| b.is_ascii_digit())
        && counter.parse::<u64>().is_ok_and(|n| n > 0)
}
fn lost_since(log: &crate::events::EventLog, since: u64) -> u64 {
    log.last().saturating_sub(since).saturating_sub(log.log_rows(since).len() as u64)
}
fn visible_event(e: &Event, config: &SessionConfig) -> bool {
    if !config.has("read") {
        return false;
    }
    if matches!(e.kind.as_str(), "chat" | "dialog" | "server_log") && !config.has("read_chat") {
        return false;
    }
    if e.kind == "chat"
        && (e.data["channel"] == "tell" || matches!(e.data["channel"].as_u64(), Some(4 | 20)))
        && !config.has("read_tells")
    {
        return false;
    }
    true
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    fn config() -> SessionConfig {
        SessionConfig {
            id: "dm:one".into(),
            id_prefix: "one.".into(),
            groups: vec!["read".into(), "npc".into()],
            watch_limit: 16,
        }
    }
    fn setup() -> Arc<Gateway> {
        let s = Arc::new(LiveStore::default());
        s.sync(br#"{"v":1,"boot":"boot","seq":1,"areas":[{"area":"a","full":true,"cre":[{"id":"1"}]}]}"#, 0).unwrap();
        let g = Arc::new(Gateway::new(s).unwrap());
        g.connect(config(), None, &[], 0).unwrap();
        g
    }
    fn command(n: u64) -> Value {
        json!({"id":format!("one.1234abcd.{n}"),"boot":"boot","area":"a","op":"stop_patrol","target":"1"})
    }
    #[test]
    fn audit_h1_final_accept_rechecks_removed_replaced_and_boot_changed_sessions() {
        for scenario in 0..3 {
            let g = setup();
            let writer = g.store.command_io.lock().unwrap();
            let worker = {
                let g = g.clone();
                std::thread::spawn(move || g.commands("dm:one", &[command(1)], 0))
            };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !g.store.lock().gateway.sessions["dm:one"].outbox.contains_key("one.1234abcd.1") {
                assert!(std::time::Instant::now() < deadline, "command did not reach reservation");
                std::thread::yield_now();
            }
            match scenario {
                0 => {
                    g.remove_session("dm:one", 0).unwrap();
                }
                1 => {
                    let mut c = config();
                    c.groups = vec!["read".into()];
                    g.connect(c, None, &[], 0).unwrap();
                }
                _ => {
                    g.remove_session("dm:one", 0).unwrap();
                    g.store.sync(br#"{"v":1,"boot":"new","seq":1,"areas":[{"area":"a","full":true,"cre":[{"id":"1"}]}]}"#, 1).unwrap();
                }
            }
            drop(writer);
            let reply = worker.join().expect("removed session must not panic").unwrap();
            assert!(reply.accepted.is_empty(), "stale session accepted scenario {scenario}");
            assert_eq!(reply.rejected.len(), 1);
            assert!(!g.store.lock().accepted.contains_key("one.1234abcd.1"));
        }
    }
    #[test]
    fn audit_h1_failed_resend_preserves_original_pending_result() {
        let g = setup();
        assert_eq!(g.commands("dm:one", &[command(1)], 0).unwrap().accepted.len(), 1);
        g.store.set_agent_consent(false);
        let bad = command(1);
        assert_eq!(g.commands("dm:one", &[bad], 1).unwrap().rejected.len(), 1);
        assert!(g.store.lock().gateway.sessions["dm:one"].outbox.contains_key("one.1234abcd.1"));
    }
    #[test]
    fn audit_m1_scripted_spawn_does_not_grant_script_execution_on_any_route() {
        for path in 0..3 {
            let g = setup();
            g.store.set_policy(Policy { scripts: true, ..Policy::default() });
            let mut c = config();
            c.groups = vec!["spawn".into(), "spawn_scripted".into()];
            let scope = c.scope();
            let mut input = json!({"id":"script-denied","area":"a","op":"run_script","target":"1","script":"example"});
            if path != 0 {
                input.as_object_mut().unwrap().remove("op");
            }
            match path {
                0 => assert_eq!(
                    crate::operations::command_scoped(input, &g.store, 0, &scope).unwrap_err().code,
                    ErrorCode::Forbidden
                ),
                1 => assert_eq!(
                    crate::operations::call_scoped(
                        "live_run_script",
                        input,
                        &g.store,
                        &std::sync::atomic::AtomicBool::new(false),
                        &scope
                    )
                    .unwrap_err()
                    .code,
                    ErrorCode::Forbidden
                ),
                _ => assert_eq!(
                    crate::api::route(
                        "POST",
                        "/v1/call/live_run_script",
                        &[],
                        &serde_json::to_vec(&input).unwrap(),
                        &scope,
                        &g.store
                    )
                    .status,
                    403
                ),
            }
            let mut input = json!({"id":"spawn-allowed","area":"a","op":"spawn_json","json":{"__data_type":"UTC "},"x":0,"y":0,"z":0,"f":0});
            if path != 0 {
                input.as_object_mut().unwrap().remove("op");
            }
            match path {
                0 => {
                    crate::operations::command_scoped(input, &g.store, 0, &scope).unwrap();
                }
                1 => {
                    crate::operations::call_scoped(
                        "live_spawn_json",
                        input,
                        &g.store,
                        &std::sync::atomic::AtomicBool::new(false),
                        &scope,
                    )
                    .unwrap();
                }
                _ => assert_eq!(
                    crate::api::route(
                        "POST",
                        "/v1/call/live_spawn_json",
                        &[],
                        &serde_json::to_vec(&input).unwrap(),
                        &scope,
                        &g.store
                    )
                    .status,
                    200
                ),
            }
        }
    }
    #[test]
    fn audit_m4_completed_result_history_is_bounded() {
        let g = setup();
        for n in 0..5000 {
            let id = format!("one.1234abcd.{n}");
            g.store.lock().gateway.sessions.get_mut("dm:one").unwrap().outbox.insert(
                id.clone(),
                Outbox {
                    reservation: n,
                    row: MirroredResult {
                        id: id.clone(),
                        rev: 1,
                        state: ResultState::Final,
                        result: None,
                    },
                    created_ms: 0,
                    acked: 0,
                    sent: 1,
                    boot: "boot".into(),
                    ttl_ms: 30_000,
                },
            );
            g.ack("dm:one", &[(id, 1)], 0).unwrap();
        }
        assert!(g.store.lock().gateway.sessions["dm:one"].completed.len() <= 4096);
        g.pushes("dm:one", RESULT_RETENTION_MS).unwrap();
        assert!(g.store.lock().gateway.sessions["dm:one"].completed.is_empty());
    }
    #[test]
    fn audit_m5_readers_share_command_log_but_results_remain_private() {
        let g = setup();
        let mut other = config();
        other.id = "ai:two".into();
        other.id_prefix = "two.".into();
        other.groups = vec!["read".into()];
        g.connect(other, None, &[], 0).unwrap();
        assert_eq!(g.commands("dm:one", &[command(1)], 0).unwrap().accepted.len(), 1);
        g.store
            .sync(
                serde_json::to_vec(&json!({"v":1,"boot":"boot","seq":2,
            "events":[{"seq":1,"kind":"skill_roll","area":"a","request":"one.1234abcd.1"}],
            "results":[{"id":"one.1234abcd.1","ok":true}]}))
                .unwrap()
                .as_slice(),
                1,
            )
            .unwrap();
        for id in ["dm:one", "ai:two"] {
            let pushes = g.pushes(id, 1).unwrap();
            let rows: Vec<_> = pushes
                .iter()
                .filter_map(|p| if let Push::Events(e) = p { Some(&e.rows) } else { None })
                .flatten()
                .collect();
            assert!(rows.iter().any(|e| e.kind == "command"), "{id} misses shared command log");
            assert!(rows.iter().any(|e| e.kind == "skill_roll"), "{id} misses shared roll");
            assert_eq!(pushes.iter().any(|p| matches!(p, Push::Result(_))), id == "dm:one");
        }
    }

    #[test]
    fn audit_m3_unknown_ack_keeps_outbox_for_late_final() {
        let g = setup();
        g.commands("dm:one", &[command(1)], 0).unwrap();
        g.store.sync(br#"{"v":1,"boot":"boot","seq":2,"ack":["one.1234abcd.1"]}"#, 1).unwrap();
        let pushes = g.pushes("dm:one", 1).unwrap();
        let row = pushes
            .into_iter()
            .find_map(|p| if let Push::Result(r) = p { Some(r) } else { None })
            .unwrap();
        assert_eq!(row.state, ResultState::Unknown);
        g.ack("dm:one", &[(row.id.clone(), row.rev)], 0).unwrap();
        assert!(g.store.lock().gateway.sessions["dm:one"].outbox.contains_key(&row.id));
    }
    #[test]
    fn audit_e1_session_pumps_do_not_expire_shared_store() {
        let g = setup();
        let mut c = command(1);
        c["ttl_ms"] = json!(1);
        g.commands("dm:one", &[c], 0).unwrap();
        for _ in 0..10 {
            g.pushes("dm:one", 10).unwrap();
        }
        assert!(g.store.result("one.1234abcd.1").is_none(), "expiration belongs to the host tick");
        let count = || {
            g.store.lock().gateway.watch_recomputations.load(std::sync::atomic::Ordering::Relaxed)
        };
        let before = count();
        g.tick(10).unwrap();
        assert_eq!(g.store.result("one.1234abcd.1").unwrap().code, Some(ErrorCode::Expired));
        for now in 11..100 {
            g.tick(now).unwrap();
            g.pushes("dm:one", now).unwrap();
        }
        assert_eq!(count(), before);
        g.watch_set("dm:one", &["a".into()], 100).unwrap();
        assert_eq!(count(), before + 1);
        let restarted = Gateway::new(g.store.clone()).unwrap();
        restarted.tick(101).unwrap();
        assert!(!g.store.lock().watch.owners.contains_key("gateway"));
    }
    #[test]
    fn audit_e2_events_are_batched_without_skipping_hidden_or_large_rows() {
        let g = setup();
        let rows:Vec<_>=(1..=700).map(|n|json!({"seq":n,"kind":if n%5==0 {"chat"}else{"attack"},"area":format!("a{}",n/100),"text":"x".repeat(12_000)})).collect();
        g.store.lock().events.game("boot", &rows, 0, 0);
        let last = g.store.event_cursor();
        let mut through = 0;
        let mut visible = Vec::new();
        while through < last {
            let e = g
                .pushes("dm:one", 0)
                .unwrap()
                .into_iter()
                .find_map(|p| if let Push::Events(e) = p { Some(e) } else { None })
                .unwrap();
            assert!(e.rows.len() <= 256);
            assert!(serde_json::to_vec(&Push::Events(e.clone())).unwrap().len() <= 1024 * 1024);
            assert!(e.through > through);
            through = e.through;
            visible.extend(
                e.rows
                    .into_iter()
                    .filter(|e| e.kind == "attack")
                    .map(|e| e.data["game_seq"].as_u64().unwrap()),
            );
        }
        assert_eq!(visible, (1..=700).filter(|n| n % 5 != 0).collect::<Vec<u64>>());
        g.store.lock().events.game(
            "boot",
            &[
                json!({"seq":701,"kind":"attack","area":"a","text":"x".repeat(1024*1024)}),
                json!({"seq":702,"kind":"attack","area":"a"}),
            ],
            0,
            0,
        );
        let batch = g
            .pushes("dm:one", 0)
            .unwrap()
            .into_iter()
            .find_map(|p| if let Push::Events(e) = p { Some(e) } else { None })
            .unwrap();
        assert_eq!(batch.lost, 1);
        assert_eq!(batch.rows.len(), 1);
        assert_eq!(batch.through, g.store.event_cursor());
    }
}
