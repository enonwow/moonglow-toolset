//! Managed gateway identities. Only token digests are retained.
use super::*;
use crate::gateway_frames::FrameError;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub id: String,
    pub name: String,
    pub groups: Vec<String>,
}
impl Principal {
    pub fn validate(&self) -> Result<(), String> {
        let (kind, id) = self.id.split_once(':').ok_or("Invalid principal")?;
        if !matches!(kind, "dm" | "ai")
            || id.is_empty()
            || id.len() > 128
            || matches!(id, "default" | "local")
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || self.name.len() > 256
            || self.groups.len() > 32
        {
            return Err("Invalid or reserved principal".into());
        }
        Ok(())
    }
    pub fn scope(&self, delivery: DeliveryMode) -> Result<Scope, String> {
        self.validate()?;
        let mut scope = SessionConfig {
            id: self.id.clone(),
            id_prefix: String::new(),
            groups: self.groups.clone(),
            watch_limit: 0,
        }
        .scope();
        scope.delivery = delivery;
        Ok(scope)
    }
}
/// Wire identity summary. Credential digests and internal capabilities never enter this DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSummary {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub avatar: Option<String>,
    pub llm: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityDisabled {
    Revoked,
    ConsentWithdrawn,
}
/// Called after durable registry mutation, with no store lock held.
pub trait IdentityObserver: std::fmt::Debug + Send + Sync + 'static {
    fn disabled(&self, principal: &str, reason: IdentityDisabled);
}
fn avatar(value: Option<&Value>) -> Result<Option<String>, FrameError> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let url = value.as_str().and_then(crate::avatar::avatar_url).ok_or_else(|| {
        invalid("Avatar must be a public HTTPS URL with a domain name, without credentials")
    })?;
    Ok(Some(url.to_string()))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiIdentity {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub groups: Vec<String>,
    pub expires_ms: u64,
    pub llm_allowed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiRecord {
    #[serde(default)]
    revoked: bool,
    #[serde(default)]
    retired_ms: u64,
    #[serde(default)]
    avatar: Option<String>,
    identity: AiIdentity,
    hash: [u8; 32],
}
impl AiRecord {
    fn summary(&self) -> AiSummary {
        AiSummary {
            id: self.identity.id.clone(),
            name: self.identity.name.clone(),
            owner: self.identity.owner.clone(),
            avatar: self.avatar.clone(),
            llm: self.identity.llm_allowed,
        }
    }
}
#[derive(Debug, Default)]
pub(super) struct Identities {
    ais: BTreeMap<String, AiRecord>,
    owners: BTreeMap<String, Principal>,
    path: Option<PathBuf>,
    global: bool,
    imports_sealed: bool,
}
fn digest(token: &str) -> [u8; 32] {
    ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
        .as_ref()
        .try_into()
        .expect("SHA256 length")
}
fn forbidden() -> FrameError {
    FrameError::new("forbidden", "Owner permission required")
}
fn invalid(msg: &str) -> FrameError {
    FrameError::new("invalid", msg)
}
fn require_dm(config: &SessionConfig, group: &str) -> Result<(), FrameError> {
    if !config.id.starts_with("dm:") || (!config.has(group) && !config.has("admin")) {
        return Err(forbidden());
    }
    Ok(())
}
fn groups(value: Option<&Value>) -> Result<Vec<String>, FrameError> {
    let groups: Vec<String> =
        serde_json::from_value(value.cloned().unwrap_or(json!(["read", "read_chat", "speech"])))
            .map_err(|_| invalid("Invalid AI groups"))?;
    // Non-speech writes need typed proposals before they can be delegated safely.
    if groups.len() > 16
        || groups.iter().any(|g| {
            !matches!(g.as_str(), "read" | "read_chat" | "read_tells" | "speech" | "focus")
        })
    {
        return Err(invalid("AI groups support reads, speech drafts and focus"));
    }
    Ok(groups.into_iter().collect::<BTreeSet<_>>().into_iter().collect())
}
const ACTIVE_AI_PER_OWNER: usize = 16;
const ACTIVE_AI_GLOBAL: usize = 256;
const RETIRED_AI: usize = 64;
const RETIRED_MS: u64 = 86_400_000;
fn prune(ais: &mut BTreeMap<String, AiRecord>, now: u64) {
    ais.retain(|_, a| {
        a.identity.expires_ms > now && (!a.revoked || now.saturating_sub(a.retired_ms) < RETIRED_MS)
    });
    let mut retired: Vec<_> =
        ais.iter().filter(|(_, a)| a.revoked).map(|(id, a)| (a.retired_ms, id.clone())).collect();
    retired.sort();
    let excess = retired.len().saturating_sub(RETIRED_AI);
    for (_, id) in retired.into_iter().take(excess) {
        ais.remove(&id);
    }
}
fn capacity(ais: &BTreeMap<String, AiRecord>, owner: &str) -> bool {
    ais.values().filter(|a| !a.revoked).count() < ACTIVE_AI_GLOBAL
        && ais.values().filter(|a| !a.revoked && a.identity.owner == owner).count()
            < ACTIVE_AI_PER_OWNER
}
impl Identities {
    pub(super) fn owned_by(&self, ai: &str, owner: &str) -> bool {
        self.ais.get(ai).is_some_and(|a| a.identity.owner == owner)
    }
    fn save(&self, ais: &BTreeMap<String, AiRecord>) -> Result<(), FrameError> {
        if let Some(path) = &self.path {
            let bytes =
                serde_json::to_vec(ais).map_err(|_| invalid("Cannot encode AI registry"))?;
            let parent = path.parent().ok_or_else(|| invalid("Invalid registry path"))?;
            std::fs::create_dir_all(parent)
                .map_err(|_| invalid("Cannot create AI registry directory"))?;
            let part = path.with_extension("json.part");
            let result = (|| -> std::io::Result<()> {
                use std::io::Write;
                let mut file = std::fs::File::create(&part)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                std::fs::rename(&part, path)
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(&part);
                return Err(invalid("Cannot save AI registry"));
            }
        }
        Ok(())
    }
    fn effective(&self, ai: &AiRecord, now: u64) -> Option<Principal> {
        let owner = self.owners.get(&ai.identity.owner)?;
        if ai.revoked || !self.global || !ai.identity.llm_allowed || ai.identity.expires_ms <= now {
            return None;
        }
        Some(Principal {
            id: ai.identity.id.clone(),
            name: ai.identity.name.clone(),
            groups: ai
                .identity
                .groups
                .iter()
                .filter(|g| owner.groups.contains(g))
                .cloned()
                .collect(),
        })
    }
}
impl Gateway {
    pub fn set_identity_observer(&self, observer: Arc<dyn IdentityObserver>) {
        *self.identity_observer.lock().unwrap_or_else(|e| e.into_inner()) = Some(observer);
    }
    pub fn ai_avatar(&self, id: &str) -> Option<String> {
        self.store.lock().gateway.identities.ais.get(id).and_then(|a| a.avatar.clone())
    }
    /// Bootstrap display metadata for a trusted imported identity.
    pub fn set_ai_avatar(&self, id: &str, value: Option<&str>) -> Result<(), String> {
        let value = avatar(Some(&json!(value))).map_err(|e| e.to_string())?;
        let mut s = self.store.lock();
        let mut ais = s.gateway.identities.ais.clone();
        let ai = ais.get_mut(id).filter(|a| !a.revoked).ok_or("Unknown AI")?;
        ai.avatar = value;
        s.gateway.identities.save(&ais).map_err(|e| e.to_string())?;
        s.gateway.identities.ais = ais;
        Ok(())
    }
    /// Trusted host quota. Kept separately from request data and checked at final acceptance.
    pub fn set_command_limit(&self, principal: &str, limit: u32) -> Result<(), String> {
        Scope::restricted(principal, vec![], DeliveryMode::Auto).with_command_limit(limit)?;
        let mut s = self.store.lock();
        let known: BTreeSet<_> = s
            .gateway
            .identities
            .ais
            .keys()
            .chain(s.gateway.identities.owners.keys())
            .chain(s.gateway.sessions.keys())
            .cloned()
            .collect();
        if !known.contains(principal) {
            return Err("Unknown principal".into());
        }
        s.gateway.command_limits.retain(|id, _| known.contains(id));
        s.gateway.command_limits.insert(principal.into(), limit);
        Ok(())
    }
    /// Resolve the AI whose command an approval would accept; host refreshes its quota first.
    pub fn draft_principal(&self, draft_id: &str, now: u64) -> Option<Principal> {
        let drafts = self.store.drafts();
        let client = &drafts.iter().find(|d| d.command.id == draft_id)?.client;
        let s = self.store.lock();
        s.gateway.identities.ais.get(client).and_then(|a| s.gateway.identities.effective(a, now))
    }
    /// Load bounded persisted AI digests before accepting clients. Global consent stays off.
    pub fn identity_file(&self, path: &Path) -> Result<(), String> {
        let mut s = self.store.lock();
        if !s.gateway.sessions.is_empty() || !s.gateway.identities.ais.is_empty() {
            return Err("Configure identities before sessions".into());
        }
        let exists = path.exists();
        let mut ais: BTreeMap<String, AiRecord> = match std::fs::metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(_) => return Err("Cannot read AI registry".into()),
            Ok(meta) if meta.len() > 1024 * 1024 => return Err("AI registry exceeds limit".into()),
            Ok(_) => {
                serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read AI registry")?)
                    .map_err(|_| "Invalid AI registry")?
            }
        };
        if ais.len() > ACTIVE_AI_GLOBAL + RETIRED_AI
            || ais.iter().any(|(id, a)| {
                id != &a.identity.id
                    || !id.starts_with("ai:")
                    || avatar(Some(&json!(a.avatar))).is_err()
                    || !a.identity.owner.starts_with("dm:")
                    || Principal {
                        id: id.clone(),
                        name: a.identity.name.clone(),
                        groups: a.identity.groups.clone(),
                    }
                    .validate()
                    .is_err()
            })
        {
            return Err("Invalid AI registry".into());
        }
        prune(&mut ais, crate::now_ms());
        s.gateway.identities.ais = ais;
        s.gateway.identities.path = Some(path.to_owned());
        s.gateway.identities.imports_sealed = exists;
        Ok(())
    }
    /// Host-only global switch, deliberately absent from request dispatch.
    pub fn set_llm_enabled(&self, on: bool) {
        let mut s = self.store.lock();
        if s.gateway.identities.global != on {
            s.gateway.identities.global = on;
            s.agent_epoch += 1;
        }
    }
    pub fn llm_enabled(&self) -> bool {
        self.store.lock().gateway.identities.global
    }
    /// Refresh a DM from the host's current authority, not client frame data.
    pub fn refresh_principal(&self, principal: Principal) -> Result<(), String> {
        principal.validate()?;
        if !principal.id.starts_with("dm:") {
            return Err("DM identity required".into());
        }
        let mut s = self.store.lock();
        if !s.gateway.identities.owners.contains_key(&principal.id)
            && s.gateway.identities.owners.len() >= 1024
        {
            return Err("Identity capacity reached".into());
        }
        s.gateway.identities.owners.insert(principal.id.clone(), principal);
        Ok(())
    }
    pub fn ai_owner(&self, id: &str) -> Option<String> {
        self.store.lock().gateway.identities.ais.get(id).map(|a| a.identity.owner.clone())
    }
    /// Trusted host import for existing configured AI credentials. No token is retained.
    pub fn install_ai(&self, identity: AiIdentity, token: &str) -> Result<(), String> {
        Principal {
            id: identity.id.clone(),
            name: identity.name.clone(),
            groups: identity.groups.clone(),
        }
        .validate()?;
        if !identity.id.starts_with("ai:")
            || !identity.owner.starts_with("dm:")
            || !(32..=256).contains(&token.len())
        {
            return Err("Invalid AI credential".into());
        }
        let mut s = self.store.lock();
        if s.gateway.identities.imports_sealed {
            return Err(
                "Static AI import is sealed; create credentials through the owner API".into()
            );
        }
        let mut ais = s.gateway.identities.ais.clone();
        prune(&mut ais, crate::now_ms());
        if !capacity(&ais, &identity.owner)
            || ais.contains_key(&identity.id)
            || ais.values().any(|a| a.hash == digest(token))
        {
            return Err("AI identity already exists or capacity reached".into());
        }
        ais.insert(
            identity.id.clone(),
            AiRecord { retired_ms: 0, avatar: None, revoked: false, identity, hash: digest(token) },
        );
        s.gateway.identities.save(&ais).map_err(|e| e.to_string())?;
        s.gateway.identities.ais = ais;
        Ok(())
    }
    /// Token lookup yields only the owner id. The host refreshes that owner's permissions
    /// before authentication; unknown or revoked tokens never fall back to host auth.
    pub fn token_owner(&self, token: &str) -> Option<String> {
        let hash = digest(token);
        self.store
            .lock()
            .gateway
            .identities
            .ais
            .values()
            .find(|a| a.hash == hash)
            .map(|a| a.identity.owner.clone())
    }
    pub fn authenticate_ai(
        &self,
        token: &str,
        owner: Option<Principal>,
        now: u64,
    ) -> Option<Principal> {
        let hash = digest(token);
        let mut s = self.store.lock();
        let ai = s.gateway.identities.ais.values().find(|a| a.hash == hash)?.clone();
        match owner.filter(|p| p.validate().is_ok() && p.id == ai.identity.owner) {
            Some(p) => {
                s.gateway.identities.owners.insert(p.id.clone(), p);
            }
            None => {
                s.gateway.identities.owners.remove(&ai.identity.owner);
                return None;
            }
        }
        s.gateway.identities.effective(&ai, now)
    }
    pub(super) fn identity_request(
        &self,
        config: &SessionConfig,
        op: &str,
        a: &Value,
        now: u64,
    ) -> Result<Value, FrameError> {
        require_dm(config, if op == "llm.allow" { "allow_llm" } else { "manage_ai" })?;
        let mut s = self.store.lock();
        let identities = &mut s.gateway.identities;
        if op == "ai.list" {
            return Ok(json!(
                identities
                    .ais
                    .values()
                    .filter(|v| !v.revoked
                        && v.identity.expires_ms > now
                        && (v.identity.owner == config.id || config.has("admin")))
                    .map(AiRecord::summary)
                    .collect::<Vec<_>>()
            ));
        }
        let mut ais = identities.ais.clone();
        prune(&mut ais, now);
        let mut disabled = Vec::new();
        let result = match op {
            "ai.create" => {
                if !capacity(&ais, &config.id) {
                    return Err(FrameError::new("rate_limited", "AI capacity reached"));
                }
                let name = a["name"]
                    .as_str()
                    .filter(|n| !n.trim().is_empty() && n.len() <= 256)
                    .ok_or_else(|| invalid("Invalid AI name"))?;
                let groups = groups(a.get("groups"))?;
                let expiry = match a.get("expires_ms") {
                    Some(v) => v.as_u64().ok_or_else(|| invalid("Invalid expiry"))?,
                    None => now.saturating_add(30 * 86_400_000),
                };
                if expiry <= now || expiry > now.saturating_add(365 * 86_400_000) {
                    return Err(invalid("Expiry must be within one year"));
                }
                let id = format!(
                    "ai:{}",
                    &crate::service::generate_token()
                        .map_err(|_| invalid("Random source unavailable"))?[..32]
                );
                let token = format!(
                    "mgai_{}",
                    crate::service::generate_token()
                        .map_err(|_| invalid("Random source unavailable"))?
                );
                let identity = AiIdentity {
                    id: id.clone(),
                    owner: config.id.clone(),
                    name: name.into(),
                    groups,
                    expires_ms: expiry,
                    llm_allowed: false,
                };
                ais.insert(
                    id,
                    AiRecord {
                        retired_ms: 0,
                        avatar: None,
                        revoked: false,
                        identity: identity.clone(),
                        hash: digest(&token),
                    },
                );
                let ai = ais.get_mut(&identity.id).expect("inserted identity");
                ai.avatar = avatar(a.get("avatar"))?;
                json!({"ai":ai.summary(),"token":token})
            }
            "llm.allow" => {
                let on = a["on"].as_bool().ok_or_else(|| invalid("Expected boolean on"))?;
                let selected = match a.get("id") {
                    None => None,
                    Some(v) => Some(
                        v.as_str()
                            .filter(|s| !s.is_empty())
                            .ok_or_else(|| invalid("Invalid AI id"))?,
                    ),
                };
                if let Some(id) = selected {
                    let ai = ais.get(id).filter(|a| !a.revoked).ok_or_else(forbidden)?;
                    if ai.identity.owner != config.id && !config.has("admin") {
                        return Err(forbidden());
                    }
                }
                let mut ids = Vec::new();
                for (id, ai) in ais.iter_mut().filter(|(id, ai)| {
                    !ai.revoked
                        && selected.map_or(ai.identity.owner == config.id, |selected| {
                            selected == id.as_str()
                        })
                }) {
                    ai.identity.llm_allowed = on;
                    ids.push(id.clone());
                    if !on {
                        disabled.push((id.clone(), IdentityDisabled::ConsentWithdrawn));
                    }
                }
                json!({"ids":ids})
            }
            "ai.update" | "ai.revoke" => {
                let id = a["id"].as_str().ok_or_else(|| invalid("Missing AI id"))?;
                let ai = ais.get_mut(id).ok_or_else(forbidden)?;
                if ai.identity.owner != config.id && !config.has("admin") {
                    return Err(forbidden());
                }
                if op == "ai.revoke" {
                    ai.revoked = true;
                    ai.retired_ms = now;
                    ai.identity.llm_allowed = false;
                    disabled.push((id.to_owned(), IdentityDisabled::Revoked));
                    json!({})
                } else {
                    if ai.revoked {
                        return Err(forbidden());
                    }
                    if let Some(name) = a.get("name") {
                        ai.identity.name = name
                            .as_str()
                            .filter(|s| !s.trim().is_empty() && s.len() <= 256)
                            .ok_or_else(|| invalid("Invalid AI name"))?
                            .into();
                    }
                    if a.get("avatar").is_some() {
                        ai.avatar = avatar(a.get("avatar"))?;
                    }
                    if a.get("groups").is_some() {
                        ai.identity.groups = groups(a.get("groups"))?;
                    }
                    if let Some(expiry) = a.get("expires_ms") {
                        let expiry = expiry
                            .as_u64()
                            .filter(|n| *n > now && *n <= now.saturating_add(365 * 86_400_000))
                            .ok_or_else(|| invalid("Invalid expiry"))?;
                        ai.identity.expires_ms = expiry;
                    }
                    json!({"ai":ai.summary()})
                }
            }
            _ => return Err(invalid("Unknown identity operation")),
        };
        prune(&mut ais, now);
        identities.save(&ais)?;
        identities.ais = ais;
        identities.imports_sealed = true;
        s.agent_epoch += 1;
        // Update online display metadata immediately, independently of focus changes.
        let display: Vec<_> = s.gateway.identities.ais.values().map(AiRecord::summary).collect();
        let mut presence_changed = false;
        for ai in display {
            if let Some(session) = s.gateway.sessions.get_mut(&ai.id)
                && (session.actor["name"] != ai.name || session.actor["avatar"] != json!(ai.avatar))
            {
                session.actor["name"] = json!(ai.name);
                session.actor["avatar"] = json!(ai.avatar);
                presence_changed = true;
            }
        }
        for (id, _) in &disabled {
            presence_changed |= s.gateway.sessions.remove(id).is_some();
            s.gateway.generation += 1;
        }
        if presence_changed {
            s.gateway.presence_rev += 1;
        }
        s.events.changed.notify_all();
        drop(s);
        let observer = self.identity_observer.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(observer) = observer {
            for (id, reason) in disabled {
                observer.disabled(&id, reason);
            }
        }
        Ok(result)
    }
}
// Checked again at final acceptance; preflight authentication is insufficient.
pub(in crate::store) fn validate_ai_scope(s: &State, scope: &Scope) -> Result<(), CommandError> {
    if !scope.name().starts_with("ai:") {
        return Ok(());
    }
    let identities = &s.gateway.identities;
    let current =
        identities.ais.get(scope.name()).and_then(|a| identities.effective(a, crate::now_ms()));
    let Some(current) = current else {
        return Err(CommandError::new(ErrorCode::Forbidden, "AI access is disabled or revoked"));
    };
    let current =
        current.scope(scope.delivery).map_err(|e| CommandError::new(ErrorCode::Forbidden, e))?;
    for group in [
        ClientGroup::Read,
        ClientGroup::ReadChat,
        ClientGroup::Speech,
        ClientGroup::Npc,
        ClientGroup::Variables,
        ClientGroup::Spawn,
        ClientGroup::SpawnScripted,
        ClientGroup::Scripts,
        ClientGroup::Chunks,
        ClientGroup::Players,
    ] {
        if scope.has(group) && !current.has(group) {
            return Err(CommandError::new(ErrorCode::Forbidden, "AI permissions changed"));
        }
    }
    Ok(())
}

pub(in crate::store) fn draft_scope(
    s: &State,
    caller: &str,
    client: &str,
    approving: bool,
) -> Result<Scope, CommandError> {
    let denied = || CommandError::new(ErrorCode::Forbidden, "Draft owner permission required");
    let config = &s.gateway.sessions.get(caller).ok_or_else(denied)?.config;
    require_dm(config, "approve_drafts").map_err(|_| denied())?;
    let ai = s.gateway.identities.ais.get(client);
    if !config.has("admin") && ai.is_none_or(|ai| ai.identity.owner != caller) {
        return Err(denied());
    }
    if !approving {
        return Ok(Scope::restricted(client, vec![], DeliveryMode::Auto));
    }
    let principal =
        ai.and_then(|ai| s.gateway.identities.effective(ai, crate::now_ms())).ok_or_else(denied)?;
    principal
        .scope(DeliveryMode::Auto)
        .and_then(|scope| {
            scope.with_command_limit(s.gateway.command_limits.get(client).copied().unwrap_or(60))
        })
        .map_err(|_| denied())
}

#[cfg(test)]
mod retention_audit_tests {
    use super::*;
    fn setup() -> (Gateway, SessionConfig) {
        let g = Gateway::new(Arc::new(LiveStore::default())).unwrap();
        let c = SessionConfig {
            id: "dm:owner".into(),
            id_prefix: "owner.".into(),
            groups: vec!["read".into(), "manage_ai".into()],
            watch_limit: 16,
        };
        (g, c)
    }
    #[test]
    fn revoked_identities_do_not_exhaust_capacity_or_reactivate_old_tokens() {
        let (g, c) = setup();
        let mut old = Vec::new();
        for n in 0..300 {
            let created =
                g.identity_request(&c, "ai.create", &json!({"name":"Writer"}), n).unwrap();
            old.push(created["token"].as_str().unwrap().to_owned());
            g.identity_request(&c, "ai.revoke", &json!({"id":created["ai"]["id"]}), n).unwrap();
        }
        assert!(g.store.lock().gateway.identities.ais.len() <= 64);
        for token in old {
            assert!(g.authenticate_ai(&token, None, 301).is_none());
            assert!(
                g.install_ai(
                    AiIdentity {
                        id: "ai:reimport".into(),
                        owner: c.id.clone(),
                        name: "Old".into(),
                        groups: vec!["read".into()],
                        expires_ms: u64::MAX,
                        llm_allowed: true
                    },
                    &token
                )
                .is_err()
            );
        }
    }
    #[test]
    fn active_ai_capacity_is_scoped_to_owner() {
        let (g, c) = setup();
        for _ in 0..16 {
            g.identity_request(&c, "ai.create", &json!({"name":"Writer"}), 0).unwrap();
        }
        assert_eq!(
            g.identity_request(&c, "ai.create", &json!({"name":"Writer"}), 0).unwrap_err().code,
            "rate_limited"
        );
        let other = SessionConfig { id: "dm:other".into(), ..c };
        assert!(g.identity_request(&other, "ai.create", &json!({"name":"Writer"}), 0).is_ok());
    }
}

#[cfg(test)]
mod consent_wire_tests {
    use super::*;
    #[test]
    fn per_ai_consent_avatar_and_list_wire_are_isolated_and_validated() {
        let g = Gateway::new(Arc::new(LiveStore::default())).unwrap();
        let c = SessionConfig {
            id: "dm:owner".into(),
            id_prefix: "owner.".into(),
            groups: vec!["read".into(), "manage_ai".into(), "allow_llm".into()],
            watch_limit: 16,
        };
        let a = g
            .identity_request(
                &c,
                "ai.create",
                &json!({"name":"A","avatar":"HTTPS://EXAMPLE.INVALID/a.png"}),
                0,
            )
            .unwrap();
        let b = g.identity_request(&c, "ai.create", &json!({"name":"B"}), 0).unwrap();
        let id = &a["ai"]["id"];
        assert_eq!(a["ai"]["avatar"], "https://example.invalid/a.png");
        assert_eq!(a["ai"]["llm"], false);
        assert_eq!(
            g.identity_request(&c, "llm.allow", &json!({"id":id,"on":true}), 0).unwrap(),
            json!({"ids":[id]})
        );
        let rows = g.identity_request(&c, "ai.list", &json!({}), 0).unwrap();
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.iter().find(|r| r["id"] == *id).unwrap()["llm"], true);
        assert_eq!(rows.iter().find(|r| r["id"] == b["ai"]["id"]).unwrap()["llm"], false);
        for bad in [
            json!("http://example.invalid/a.png"),
            json!("https://"),
            json!(7),
            json!("https://user:pass@example.invalid/a"),
        ] {
            assert_eq!(
                g.identity_request(&c, "ai.update", &json!({"id":id,"avatar":bad}), 0)
                    .unwrap_err()
                    .code,
                "invalid"
            );
        }
        let cleared =
            g.identity_request(&c, "ai.update", &json!({"id":id,"avatar":null}), 0).unwrap();
        assert!(cleared["ai"]["avatar"].is_null());
        let other = SessionConfig { id: "dm:other".into(), ..c.clone() };
        assert_eq!(
            g.identity_request(&other, "llm.allow", &json!({"id":id,"on":false}), 0)
                .unwrap_err()
                .code,
            "forbidden"
        );
        let admin = SessionConfig { groups: vec!["admin".into()], ..other };
        assert_eq!(
            g.identity_request(&admin, "llm.allow", &json!({"id":id,"on":false}), 0).unwrap(),
            json!({"ids":[id]})
        );
    }
}

#[cfg(test)]
mod observer_tests {
    use super::*;
    #[derive(Debug)]
    struct Observer {
        store: Arc<LiveStore>,
        rows: Arc<Mutex<Vec<(String, IdentityDisabled)>>>,
    }
    impl IdentityObserver for Observer {
        fn disabled(&self, id: &str, reason: IdentityDisabled) {
            // Re-enter the store: observers must never run under its lock.
            let _ = self.store.boot();
            self.rows.lock().unwrap().push((id.into(), reason));
        }
    }
    #[test]
    fn consent_and_revoke_notify_host_after_unlock_and_presence_copies_avatar() {
        let store = Arc::new(LiveStore::default());
        let g = Gateway::new(store.clone()).unwrap();
        let c = SessionConfig {
            id: "dm:owner".into(),
            id_prefix: "owner.".into(),
            groups: vec!["read".into(), "manage_ai".into(), "allow_llm".into()],
            watch_limit: 16,
        };
        g.refresh_principal(Principal {
            id: c.id.clone(),
            name: "Owner".into(),
            groups: c.groups.clone(),
        })
        .unwrap();
        g.connect(c.clone(), None, &[], 0).unwrap();
        let rows = Arc::new(Mutex::new(Vec::new()));
        g.set_identity_observer(Arc::new(Observer { store, rows: rows.clone() }));
        let now = crate::now_ms();
        let created = g
            .identity_request(
                &c,
                "ai.create",
                &json!({"name":"A","avatar":"https://example.invalid/a.png"}),
                now,
            )
            .unwrap();
        let id = created["ai"]["id"].as_str().unwrap();
        g.identity_request(&c, "llm.allow", &json!({"id":id,"on":true}), now).unwrap();
        g.set_llm_enabled(true);
        g.connect(
            SessionConfig {
                id: id.into(),
                id_prefix: "ai.".into(),
                groups: vec!["read".into()],
                watch_limit: 16,
            },
            None,
            &[],
            now,
        )
        .unwrap();
        g.set_identity(id, "A", g.ai_avatar(id).as_deref(), Some(&c.id)).unwrap();
        g.identity_request(
            &c,
            "ai.update",
            &json!({"id":id,"avatar":"https://example.invalid/b.png"}),
            now,
        )
        .unwrap();
        assert!(g.pushes(&c.id,now).unwrap().iter().any(|p|matches!(p,Push::Presence{actors,..} if actors.iter().any(|a|a["id"]==id && a["avatar"]=="https://example.invalid/b.png"))));
        g.identity_request(&c, "llm.allow", &json!({"id":id,"on":false}), now).unwrap();
        assert!(g.pushes(id, now).is_err());
        g.identity_request(&c, "ai.revoke", &json!({"id":id}), now).unwrap();
        assert_eq!(
            *rows.lock().unwrap(),
            vec![
                (id.into(), IdentityDisabled::ConsentWithdrawn),
                (id.into(), IdentityDisabled::Revoked)
            ]
        );
    }
}
