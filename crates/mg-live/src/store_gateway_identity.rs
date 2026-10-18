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
    identity: AiIdentity,
    hash: [u8; 32],
}
#[derive(Debug, Default)]
pub(super) struct Identities {
    ais: BTreeMap<String, AiRecord>,
    owners: BTreeMap<String, Principal>,
    path: Option<PathBuf>,
    global: bool,
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
impl Identities {
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
        if !self.global || !ai.identity.llm_allowed || ai.identity.expires_ms <= now {
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
    /// Load bounded persisted AI digests before accepting clients. Global consent stays off.
    pub fn identity_file(&self, path: &Path) -> Result<(), String> {
        let mut s = self.store.lock();
        if !s.gateway.sessions.is_empty() || !s.gateway.identities.ais.is_empty() {
            return Err("Configure identities before sessions".into());
        }
        let ais: BTreeMap<String, AiRecord> = match std::fs::metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(_) => return Err("Cannot read AI registry".into()),
            Ok(meta) if meta.len() > 1024 * 1024 => return Err("AI registry exceeds limit".into()),
            Ok(_) => {
                serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read AI registry")?)
                    .map_err(|_| "Invalid AI registry")?
            }
        };
        if ais.len() > 256
            || ais.iter().any(|(id, a)| {
                id != &a.identity.id
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
        s.gateway.identities.ais = ais;
        s.gateway.identities.path = Some(path.to_owned());
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
        if s.gateway.identities.ais.len() >= 256
            || s.gateway.identities.ais.contains_key(&identity.id)
        {
            return Err("AI identity already exists or capacity reached".into());
        }
        let mut ais = s.gateway.identities.ais.clone();
        ais.insert(identity.id.clone(), AiRecord { identity, hash: digest(token) });
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
            return Ok(
                json!({"items":identities.ais.values().filter(|v|v.identity.owner==config.id || config.has("admin")).map(|v|&v.identity).collect::<Vec<_>>() }),
            );
        }
        let mut ais = identities.ais.clone();
        let mut revoked = None;
        let result = match op {
            "ai.create" => {
                if ais.len() >= 256 {
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
                ais.insert(id, AiRecord { identity: identity.clone(), hash: digest(&token) });
                json!({"ai":identity,"token":token})
            }
            "llm.allow" => {
                let on = a["on"].as_bool().ok_or_else(|| invalid("Expected boolean on"))?;
                for ai in ais.values_mut().filter(|ai| ai.identity.owner == config.id) {
                    ai.identity.llm_allowed = on;
                }
                json!({})
            }
            "ai.update" | "ai.revoke" => {
                let id = a["id"].as_str().ok_or_else(|| invalid("Missing AI id"))?;
                let ai = ais.get_mut(id).ok_or_else(forbidden)?;
                if ai.identity.owner != config.id && !config.has("admin") {
                    return Err(forbidden());
                }
                if op == "ai.revoke" {
                    ais.remove(id);
                    revoked = Some(id.to_owned());
                    json!({})
                } else {
                    if let Some(name) = a.get("name") {
                        ai.identity.name = name
                            .as_str()
                            .filter(|s| !s.trim().is_empty() && s.len() <= 256)
                            .ok_or_else(|| invalid("Invalid AI name"))?
                            .into();
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
                    json!({"ai":ai.identity})
                }
            }
            _ => return Err(invalid("Unknown identity operation")),
        };
        identities.save(&ais)?;
        identities.ais = ais;
        s.agent_epoch += 1;
        if let Some(id) = revoked {
            s.gateway.sessions.remove(&id);
            s.gateway.generation += 1;
            s.gateway.presence_rev += 1;
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

