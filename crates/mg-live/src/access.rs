//! Operation scopes and groups for agent access, and the command quotas they carry.
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "measure"))]
use std::collections::HashSet;

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
    /// Allows scripted spawn_json objects, never arbitrary run_script.
    SpawnScripted,
    Scripts,
    Chunks,
    Players,
}
impl ClientGroup {
    pub const ALL: [ClientGroup; 10] = {
        use ClientGroup::*;
        [Read, ReadChat, Variables, Npc, Speech, Spawn, SpawnScripted, Scripts, Chunks, Players]
    };
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
    pub(crate) agent_epoch: Option<u64>,
    pub(crate) gateway_accept: Option<crate::store::gateway::Acceptance>,
    pub(crate) command_limit: u32,
    pub(crate) gateway_groups: Option<Vec<String>>,
}
impl Scope {
    pub(crate) fn default() -> Self {
        Self {
            name: "default".into(),
            groups: None,
            delivery: DeliveryMode::Auto,
            agent_epoch: None,
            gateway_accept: None,
            command_limit: 60,
            gateway_groups: None,
        }
    }
}
impl Scope {
    pub(crate) fn local() -> Self {
        Self { name: "local".into(), ..Self::default() }
    }

    /// Creates a scope with exactly these groups; even an empty or complete
    /// group list never grants unrestricted local privileges.
    pub fn restricted(
        name: impl Into<String>,
        groups: Vec<ClientGroup>,
        delivery: DeliveryMode,
    ) -> Self {
        Self {
            name: name.into(),
            groups: Some(groups),
            delivery,
            agent_epoch: None,
            gateway_accept: None,
            command_limit: 60,
            gateway_groups: None,
        }
    }
    /// Host-configured game-command attempts per owner per minute. The store's
    /// global budget still applies; recreating a scope never resets either budget.
    pub fn with_command_limit(mut self, per_minute: u32) -> Result<Self, String> {
        if !(1..=60_000).contains(&per_minute) {
            return Err("Client command limit must be 1..60000 per minute".into());
        }
        self.command_limit = per_minute;
        Ok(self)
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
                    || self.has(ClientGroup::Scripts)
                    || self.has(ClientGroup::SpawnScripted)))
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
/// A fixture of named clients' scopes, for the store's own tests and diagnostics: the
/// gateway builds scopes from its principals instead.
#[derive(Debug)]
pub(crate) struct Access {
    scopes: Vec<Scope>,
}
#[cfg(any(test, feature = "measure"))]
fn valid_token(token: &str) -> bool {
    (32..=256).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_graphic())
}
#[cfg(any(test, feature = "measure"))]
impl Access {
    pub(crate) fn new(default: String, clients: &[ClientToken]) -> Result<Self, String> {
        if !valid_token(&default) {
            return Err("Live token needs 32..256 printable ASCII characters without spaces".into());
        }
        if clients.len() > 64 {
            return Err("At most 64 named live clients are supported".into());
        }
        let mut names = HashSet::from(["default".to_string(), "local".to_string()]);
        let mut tokens = HashSet::from([default]);
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
        let scopes = std::iter::once(Scope::default())
            .chain(clients.iter().map(|c| Scope {
                name: c.name.clone(),
                groups: Some(c.groups.clone()),
                delivery: c.delivery,
                agent_epoch: None,
                gateway_accept: None,
                command_limit: c.per_minute,
                gateway_groups: None,
            }))
            .collect();
        Ok(Self { scopes })
    }
    #[cfg(test)]
    pub(crate) fn scope(&self, who: usize) -> &Scope {
        &self.scopes[who]
    }
}
impl Access {
    pub(crate) fn scope_named(&self, name: &str) -> Option<Scope> {
        self.scopes.iter().find(|s| s.name == name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(access.scope_named("LLM").is_some() && access.scope_named("default").is_some());
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
    }
    #[test]
    fn configured_command_limits_are_carried_by_scopes() {
        let c = ClientToken { per_minute: 2, ..client() };
        let access = Access::new(ROOT.into(), &[c]).unwrap();
        assert_eq!(access.scope_named("default").unwrap().command_limit, 60);
        assert_eq!(access.scope_named("LLM").unwrap().command_limit, 2);
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
            let scope = Scope {
                name: "test".into(),
                groups: Some(vec![g]),
                delivery: DeliveryMode::Auto,
                agent_epoch: None,
                gateway_accept: None,
                command_limit: 60,
                gateway_groups: None,
            };
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
        let scope = access.scope_named("Conversation").unwrap();
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
}
