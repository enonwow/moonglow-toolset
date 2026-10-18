//! Named API credentials, operation scopes, command quotas and payload-free audit.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientGroup {
    Read,
    ReadChat,
    Variables,
    Npc,
    /// Speech and conversation summaries only; never creature control.
    Speech,
    Spawn,
    Scripts,
    Chunks,
    Players,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    #[default]
    Auto,
    Drafts,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClientToken {
    pub name: String,
    pub token: String,
    pub groups: Vec<ClientGroup>,
    /// Command attempts per fixed 60-second window. Reads/waits are unmetered.
    pub per_minute: u32,
    pub delivery: DeliveryMode,
}
impl Default for ClientToken {
    fn default() -> Self {
        Self {
            name: String::new(),
            token: String::new(),
            groups: vec![ClientGroup::Read],
            per_minute: 60,
            delivery: DeliveryMode::Auto,
        }
    }
}
impl ClientToken {
    /// A ready conversation assistant. Speech is held for local approval.
    pub fn conversation(name: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            token: token.into(),
            groups: vec![ClientGroup::Read, ClientGroup::ReadChat, ClientGroup::Speech],
            per_minute: 60,
            delivery: DeliveryMode::Drafts,
        }
    }
}
impl std::fmt::Debug for ClientToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientToken")
            .field("name", &self.name)
            .field("token", &"<redacted>")
            .field("groups", &self.groups)
            .field("per_minute", &self.per_minute)
            .finish()
    }
}

/// A named, restricted capability set for an external Live consumer.
/// The unrestricted local scope cannot be constructed outside this crate.
///
/// ```compile_fail
/// let scope = mg_live::access::Scope::default();
/// ```
#[derive(Debug, Clone)]
pub struct Scope {
    pub(crate) name: String,
    pub(crate) delivery: DeliveryMode,
    groups: Option<Vec<ClientGroup>>,
}
impl Scope {
    pub(crate) fn default() -> Self {
        Self { name: "default".into(), groups: None, delivery: DeliveryMode::Auto }
    }
}
impl Scope {
    /// Creates a scope with exactly these groups; even an empty or complete
    /// group list never grants unrestricted local privileges.
    pub fn restricted(
        name: impl Into<String>,
        groups: Vec<ClientGroup>,
        delivery: DeliveryMode,
    ) -> Self {
        Self { name: name.into(), groups: Some(groups), delivery }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn unrestricted(&self) -> bool {
        self.groups.is_none()
    }
    pub fn has(&self, group: ClientGroup) -> bool {
        self.groups.as_ref().is_none_or(|g| g.contains(&group))
    }
    pub fn allows(&self, op: &str) -> bool {
        if op.strip_prefix("live_").unwrap_or(op) == "watch_areas" {
            return self.unrestricted() && self.name == "local";
        }
        self.unrestricted()
            || (self.has(ClientGroup::Speech)
                && matches!(
                    op.strip_prefix("live_").unwrap_or(op),
                    "npc_say" | "reply_to_player" | "pc_message" | "npc_memory_note"
                ))
            || (group(op).is_some_and(|g| self.has(g))
                && (op.strip_prefix("live_").unwrap_or(op) != "spawn_json"
                    || self.has(ClientGroup::Scripts)))
    }
}
pub(crate) fn group(op: &str) -> Option<ClientGroup> {
    use ClientGroup::*;
    Some(match op.strip_prefix("live_").unwrap_or(op) {
        "gm_card"
        | "npc_card"
        | "npc_memory"
        | "status"
        | "area_state"
        | "watch_areas"
        | "wait"
        | "events"
        | "find"
        | "get_variable"
        | "dump_locals"
        | "view_inventory"
        | "capture_object"
        | "get_faction_reputation" => Read,
        "set_variable" => Variables,
        "npc_memory_note" => Npc,
        "spawn" | "spawn_json" | "spawn_trigger" => Spawn,
        "run_script" => Scripts,
        "run_chunk" => Chunks,
        "reply_to_player" | "pc_message" | "pc_journal" | "pc_take_item" | "pc_boot" | "fade"
        | "play_sound" | "skill_check" | "give_level" | "give_alignment" | "set_stat"
        | "jump_all_players" | "pc_heal" | "pc_raise" | "pc_teleport" | "pc_give_gold"
        | "pc_give_xp" | "pc_give_item" => Players,
        "set_faction_reputation"
        | "teleport"
        | "set_tile"
        | "patrol"
        | "stop_patrol"
        | "replace_object_animation"
        | "set_static"
        | "set_lootable"
        | "set_appearance"
        | "apply_vfx"
        | "clear_vfx"
        | "disable_trap"
        | "spawn_trap_on_object"
        | "set_weather"
        | "set_fog"
        | "set_area_light"
        | "set_skybox"
        | "set_music"
        | "set_ambient_sound"
        | "sound_at"
        | "set_time"
        | "set_date"
        | "take_item"
        | "kill"
        | "give_item"
        | "equip_item"
        | "unequip_item"
        | "drop_item"
        | "use_item"
        | "transfer_item"
        | "play_creature_animation"
        | "take_gold"
        | "set_xp"
        | "take_xp"
        | "remove_level"
        | "heal"
        | "set_invulnerable"
        | "set_immortal"
        | "set_ai"
        | "force_rest"
        | "limbo"
        | "unlimbo"
        | "npc_move"
        | "npc_attack"
        | "npc_say"
        | "destroy"
        | "set_state"
        | "stop_combat"
        | "set_faction"
        | "freeze"
        | "unfreeze"
        | "set_facing"
        | "set_visual_transform"
        | "set_position"
        | "play_animation"
        | "set_useable"
        | "set_name" => Npc,
        _ => return None,
    })
}
fn valid_token(token: &str) -> bool {
    (32..=256).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_graphic())
}
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, Default)]
struct Window {
    start: Option<u64>,
    used: u32,
}
#[derive(Debug)]
pub(crate) struct Access {
    pub agents: crate::agents::Agents,
    clients: Vec<ClientToken>,
    scopes: Vec<Scope>,
    windows: Mutex<Vec<Window>>,
    pub audit: Arc<Audit>,
}
impl Access {
    pub(crate) fn new(default: String, clients: &[ClientToken]) -> Result<Self, String> {
        if !valid_token(&default) {
            return Err("Live token needs 32..256 printable ASCII characters without spaces".into());
        }
        if clients.len() > 64 {
            return Err("At most 64 named live clients are supported".into());
        }
        let mut names = HashSet::from(["default".to_string(), "local".to_string()]);
        let mut tokens = HashSet::from([default.clone()]);
        for c in clients {
            if c.name.is_empty()
                || c.name.chars().count() > 64
                || c.name.chars().any(char::is_control)
                || c.name.trim() != c.name
                || !names.insert(c.name.to_lowercase())
            {
                return Err("Client names must be unique, 1..64 non-control characters (default/local are reserved)".into());
            }
            if !valid_token(&c.token) || !tokens.insert(c.token.clone()) {
                return Err("Client tokens must be unique, 32..256 printable ASCII characters without spaces".into());
            }
            if !(1..=60_000).contains(&c.per_minute) {
                return Err("Client command limit must be 1..60000 per minute".into());
            }
        }
        if clients.iter().any(|c| tokens.iter().any(|t| c.name.contains(t))) {
            return Err("Client names must not contain credentials".into());
        }
        let mut all = vec![ClientToken {
            name: "default".into(),
            token: default,
            groups: vec![],
            per_minute: 0,
            delivery: DeliveryMode::Auto,
        }];
        all.extend_from_slice(clients);
        let scopes = std::iter::once(Scope::default())
            .chain(clients.iter().map(|c| Scope {
                name: c.name.clone(),
                groups: Some(c.groups.clone()),
                delivery: c.delivery,
            }))
            .collect();
        let windows = Mutex::new((0..all.len()).map(|_| Window::default()).collect());
        Ok(Self {
            clients: all,
            scopes,
            windows,
            audit: Arc::new(Audit::default()),
            agents: Default::default(),
        })
    }
    pub(crate) fn authenticate(&self, bearer: &str) -> Option<usize> {
        let mut found = None;
        for (i, c) in self.clients.iter().enumerate() {
            if same(bearer.as_bytes(), c.token.as_bytes()) {
                found = Some(i);
            }
        }
        found
    }
    pub(crate) fn scope_named(&self, name: &str) -> Option<Scope> {
        self.scopes.iter().find(|s| s.name == name).cloned()
    }
    pub(crate) fn scope(&self, who: usize) -> &Scope {
        &self.scopes[who]
    }
    pub(crate) fn command(&self, who: usize, now: u64) -> Result<(), u64> {
        if who == 0 {
            return Ok(());
        }
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        let w = &mut windows[who];
        if w.start.is_none_or(|start| now.saturating_sub(start) >= 60_000) {
            w.start = Some(now);
            w.used = 0;
        }
        if w.used >= self.clients[who].per_minute {
            return Err((60_000 - now.saturating_sub(w.start.unwrap())).div_ceil(1000));
        }
        w.used += 1;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct AuditState {
    path: Option<PathBuf>,
    pending: Vec<Value>,
    dropped: u64,
}
#[derive(Debug, Default)]
pub(crate) struct Audit {
    state: Mutex<AuditState>,
}
const AUDIT_BYTES: u64 = 8 * 1024 * 1024;

// Reopen under an OS lock so separate service instances never keep writing an
// inode which another instance has rotated. Keep one bounded previous segment.
fn audit_line(path: &Path, record: &Value, limit: u64) -> std::io::Result<()> {
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path.with_extension("lock"))?;
    guard.lock()?;
    let row = format!("{record}\n");
    if path.metadata().is_ok_and(|m| m.len() + row.len() as u64 > limit) {
        let previous = path.with_extension("1.jsonl");
        if previous.exists() {
            std::fs::remove_file(&previous)?;
        }
        std::fs::rename(path, previous)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(row.as_bytes())?;
    file.flush()
}
impl Audit {
    pub(crate) fn open(&self, data_dir: &Path) -> Result<(), String> {
        let dir = data_dir.join("live");
        std::fs::create_dir_all(&dir).map_err(|e| format!("Audit directory: {e}"))?;
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let path = dir.join("audit.jsonl");
        drop(options.open(&path).map_err(|e| format!("Audit file: {e}"))?);
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.path.is_some() {
            return Ok(());
        }
        for record in &s.pending {
            audit_line(&path, record, AUDIT_BYTES).map_err(|e| format!("Audit write: {e}"))?;
        }
        if s.dropped > 0 {
            audit_line(&path,&json!({"at_ms":crate::now_ms(),"who":"local","op":"audit","target":null,"result":"buffer_overflow","dropped":s.dropped}), AUDIT_BYTES).map_err(|e|e.to_string())?;
        }
        s.pending.clear();
        s.path = Some(path);
        Ok(())
    }
    pub(crate) fn record(
        &self,
        who: &str,
        op: &str,
        target: Option<&str>,
        result: &str,
        phase: &str,
    ) {
        let op = op.strip_prefix("live_").unwrap_or(op);
        let op = if group(op).is_some() || matches!(op, "describe" | "openapi") {
            op
        } else {
            "unknown"
        };
        let target = target
            .filter(|t| !t.is_empty() && t.len() <= 16 && t.bytes().all(|b| b.is_ascii_hexdigit()));
        let value = json!({"at_ms":crate::now_ms(),"who":who,"by":who,"op":op,"target":target,"result":result,"phase":phase});
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(path) = &s.path {
            if audit_line(path, &value, AUDIT_BYTES).is_err() {
                log::error!("Live audit write failed");
            }
        } else if s.pending.len() < 4096 {
            s.pending.push(value);
        } else {
            s.dropped += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_rotates_during_appends_and_keeps_complete_recent_rows() {
        let dir = std::env::temp_dir()
            .join(format!("mg-audit-rotate-{}", crate::service::generate_token().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("audit.jsonl");
        for n in 0..20 {
            audit_line(&path, &json!({"n":n,"text":"bounded"}), 100).unwrap();
        }
        let mut rows = Vec::new();
        for file in [path.with_extension("1.jsonl"), path] {
            assert!(file.metadata().unwrap().len() <= 100);
            rows.extend(
                std::fs::read_to_string(file).unwrap().lines().map(|line| {
                    serde_json::from_str::<Value>(line).unwrap()["n"].as_u64().unwrap()
                }),
            );
        }
        assert_eq!(rows.last(), Some(&19));
        assert!(rows.len() < 20);
        assert!(rows.windows(2).all(|pair| pair[1] == pair[0] + 1));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn only_local_scope_can_issue_raw_watch_commands() {
        let mut scope = Scope::default();
        assert!(!scope.allows("watch_areas"));
        assert!(!scope.allows("live_watch_areas"));
        scope.name = "local".into();
        assert!(scope.allows("watch_areas"));
    }
    const ROOT: &str = "root-0123456789abcdef0123456789abcdef";
    fn client() -> ClientToken {
        ClientToken {
            name: "LLM".into(),
            token: "named-0123456789abcdef0123456789abcdef".into(),
            ..Default::default()
        }
    }
    #[test]
    fn config_validation_redaction_and_default_compatibility() {
        let c = client();
        let access = Access::new(ROOT.into(), std::slice::from_ref(&c)).unwrap();
        assert_eq!(access.authenticate(ROOT), Some(0));
        assert_eq!(access.authenticate(&c.token), Some(1));
        assert_eq!(access.authenticate("wrong"), None);
        assert_eq!(
            serde_json::from_str::<ClientToken>(r#"{"name":"LLM","token":"x"}"#)
                .unwrap()
                .per_minute,
            60
        );
        assert!(!format!("{access:?}").contains(ROOT));
        assert!(!format!("{c:?}").contains(&c.token));
        assert!(Access::new(ROOT.into(), &[c.clone(), c.clone()]).is_err());
        for bad in [
            ClientToken { name: "default".into(), ..c.clone() },
            ClientToken { token: ROOT.into(), ..c.clone() },
            ClientToken { per_minute: 0, ..c.clone() },
            ClientToken { token: "short".into(), ..c.clone() },
            ClientToken { name: "bad\nname".into(), ..c.clone() },
        ] {
            assert!(Access::new(ROOT.into(), &[bad]).is_err());
        }
        assert!(serde_json::from_str::<ClientToken>(r#"{"groups":["admin"]}"#).is_err());
        assert!(crate::LiveConfig::default().clients.is_empty());
        assert!(serde_json::from_str::<crate::LiveConfig>("{}").unwrap().clients.is_empty());
    }
    #[test]
    fn quotas_are_per_client_atomic_and_default_is_unlimited() {
        let mut c = client();
        c.per_minute = 2;
        let mut d = c.clone();
        d.name = "DM".into();
        d.token.push('2');
        let a = Access::new(ROOT.into(), &[c, d]).unwrap();
        for _ in 0..100 {
            assert!(a.command(0, 0).is_ok());
        }
        assert!(a.command(1, 1000).is_ok());
        assert!(a.command(1, 1000).is_ok());
        assert_eq!(a.command(1, 1001), Err(60));
        assert_eq!(a.command(1, 60001), Err(1));
        assert!(a.command(2, 1001).is_ok());
        assert!(a.command(1, 61000).is_ok());
        let mut c = client();
        c.per_minute = 5;
        let a = Arc::new(Access::new(ROOT.into(), &[c]).unwrap());
        let children: Vec<_> = (0..20)
            .map(|_| {
                let a = a.clone();
                std::thread::spawn(move || a.command(1, 0).is_ok())
            })
            .collect();
        assert_eq!(children.into_iter().map(|c| usize::from(c.join().unwrap())).sum::<usize>(), 5);
    }
    #[test]
    fn every_operation_has_a_primary_group_and_script_intersection() {
        use ClientGroup::*;
        let all = crate::operations::describe(crate::store::Policy {
            scripts: true,
            chunks: true,
            players: true,
        });
        let ops = all["operations"].as_array().unwrap();
        for g in [Read, ReadChat, Npc, Spawn, Scripts, Chunks, Players] {
            let scope =
                Scope { name: "test".into(), groups: Some(vec![g]), delivery: DeliveryMode::Auto };
            for op in ops {
                let name = op["name"].as_str().unwrap();
                assert!(group(name).is_some(), "{name}");
                assert_eq!(
                    scope.allows(name),
                    group(name) == Some(g)
                        && !matches!(name, "live_spawn_json" | "live_watch_areas"),
                    "{g:?} {name}"
                );
            }
        }
    }
    #[test]
    fn session_faction_read_and_write_scopes() {
        assert_eq!(group("live_get_faction_reputation"), Some(ClientGroup::Read));
        assert_eq!(group("live_set_faction_reputation"), Some(ClientGroup::Npc));
    }
    #[test]
    fn conversation_profile_is_speech_only_and_defaults_to_drafts() {
        let c = ClientToken::conversation("Conversation", "c".repeat(32));
        assert_eq!(c.delivery, DeliveryMode::Drafts);
        let access = Access::new(ROOT.into(), &[c]).unwrap();
        let scope = access.scope(1);
        for op in [
            "live_npc_say",
            "live_reply_to_player",
            "live_pc_message",
            "live_npc_memory_note",
            "live_events",
            "live_gm_card",
        ] {
            assert!(scope.allows(op), "{op}");
        }
        for op in [
            "live_kill",
            "live_destroy",
            "live_give_item",
            "live_pc_give_gold",
            "live_run_script",
            "live_run_chunk",
            "live_npc_move",
            "live_spawn",
            "live_set_variable",
        ] {
            assert!(!scope.allows(op), "{op}");
        }
        assert!(!scope.has(ClientGroup::Players));
        assert!(!scope.has(ClientGroup::Npc));
    }
    #[test]
    fn audit_buffers_then_appends_complete_rows_across_instances() {
        let dir = std::env::temp_dir().join(format!(
            "mg-live-audit-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        assert!(!dir.exists());
        let children: Vec<_> = ["A", "B"]
            .into_iter()
            .map(|name| {
                let audit = Audit::default();
                audit.record(name, "live_freeze", Some("bad target"), "queued", "request");
                audit.open(&dir).unwrap();
                audit.open(&dir).unwrap(); // No duplicate buffered rows.
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        audit.record(name, "live_freeze", Some("7f01"), "ok", "completion");
                    }
                })
            })
            .collect();
        for child in children {
            child.join().unwrap();
        }
        let path = dir.join("live/audit.jsonl");
        let rows: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(rows.len(), 202);
        assert_eq!(
            rows.iter().filter(|r| r["phase"] == "request" && r["target"].is_null()).count(),
            2
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(dir.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
