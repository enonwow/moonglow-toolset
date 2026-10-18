//! Loopback-only host for local integration, demos and CI.
use clap::Parser;
use mg_gateway::{Authenticator, CoreHandle, Principal};
use mg_live::{LiveConfig, store::LiveStore};
use serde::Deserialize;
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

#[derive(Debug)]
struct CancelUnpublished(Arc<LiveStore>);
impl mg_gateway::IdentityObserver for CancelUnpublished {
    fn disabled(&self, id: &str, reason: mg_gateway::IdentityDisabled) {
        let reason = match reason {
            mg_gateway::IdentityDisabled::Revoked => "AI revoked",
            mg_gateway::IdentityDisabled::ConsentWithdrawn => "AI consent withdrawn",
        };
        self.0.revoke_owner(id, reason);
    }
}
#[derive(Debug, Parser)]
#[command(
    about = "Loopback-only Live gateway; credentials come from a private JSON file or MOONGLOW_GATEWAY_TOKEN"
)]
struct Args {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    port: Option<u16>,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    port: u16,
    token: String,
    groups: Vec<String>,
    tokens: Vec<Credential>,
    data_dir: PathBuf,
    llm_enabled: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            port: 8080,
            token: String::new(),
            groups: default_groups(),
            tokens: vec![],
            data_dir: PathBuf::from("private/gateway-dev-data"),
            llm_enabled: false,
        }
    }
}
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .field("groups", &self.groups)
            .field("credential_count", &self.tokens.len())
            .finish()
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credential {
    token: String,
    principal: Principal,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    llm_allowed: bool,
    #[serde(default)]
    avatar: Option<String>,
}
#[derive(Default)]
struct StaticAuth {
    tokens: Vec<Credential>,
}
impl std::fmt::Debug for StaticAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaticAuth").field("credentials", &self.tokens.len()).finish()
    }
}
impl Authenticator for StaticAuth {
    fn principal(&self, id: &str) -> Option<Principal> {
        self.tokens
            .iter()
            .find(|c| c.principal.id == id && id.starts_with("dm:"))
            .map(|c| c.principal.clone())
    }
    fn authenticate(&self, bearer: &str) -> Option<Principal> {
        let mut found = None;
        for credential in &self.tokens {
            let same = credential.token.len() == bearer.len()
                && credential
                    .token
                    .bytes()
                    .zip(bearer.bytes())
                    .fold(0, |diff, (a, b)| diff | (a ^ b))
                    == 0;
            if same {
                found = Some(credential.principal.clone());
            }
        }
        found
    }
}
fn default_groups() -> Vec<String> {
    [
        "read",
        "read_chat",
        "read_tells",
        "variables",
        "npc",
        "speech",
        "spawn",
        "spawn_scripted",
        "players",
        "focus",
        "approve_drafts",
        "allow_llm",
        "manage_ai",
        "admin",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
impl Config {
    fn load(text: Option<&str>, token: Option<String>) -> Result<Self, String> {
        // Do not reflect parser errors: a misspelled field could contain a credential.
        let mut config: Self = match text {
            Some(text) => {
                serde_json::from_str(text).map_err(|_| "Invalid gateway configuration")?
            }
            None => Self::default(),
        };
        if let Some(token) = token {
            config.token = token;
        }
        Ok(config)
    }
    fn auth(&mut self) -> Result<StaticAuth, String> {
        let mut tokens = std::mem::take(&mut self.tokens);
        if !self.token.is_empty() {
            tokens.push(Credential {
                token: std::mem::take(&mut self.token),
                owner: None,
                llm_allowed: false,
                avatar: None,
                principal: Principal {
                    id: "dm:dev".into(),
                    name: "Developer".into(),
                    groups: self.groups.clone(),
                },
            });
        }
        if tokens.is_empty() {
            return Err(
                "Set MOONGLOW_GATEWAY_TOKEN or provide a private token configuration".into()
            );
        }
        for (i, c) in tokens.iter().enumerate() {
            if !(32..=256).contains(&c.token.len())
                || !c.token.bytes().all(|b| b.is_ascii_graphic())
            {
                return Err(
                    "Credentials need 32..256 printable ASCII characters without spaces".into()
                );
            }
            c.principal.validate()?;
            if c.principal.id.starts_with("ai:")
                && !c.owner.as_ref().is_some_and(|owner| {
                    tokens.iter().any(|p| p.principal.id == *owner && owner.starts_with("dm:"))
                })
            {
                return Err("Configured AI requires an existing DM owner".into());
            }
            if tokens[..i].iter().any(|old| old.token == c.token) {
                return Err("Duplicate credential".into());
            }
        }
        Ok(StaticAuth { tokens })
    }
    fn address(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }
}
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        println!("Gateway: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), String> {
    let args = Args::parse();
    let text = args
        .config
        .map(std::fs::read_to_string)
        .transpose()
        .map_err(|_| "Cannot read gateway configuration")?;
    let mut config = Config::load(text.as_deref(), std::env::var("MOONGLOW_GATEWAY_TOKEN").ok())?;
    if let Some(port) = args.port {
        config.port = port;
    }
    let auth = Arc::new(config.auth()?);
    let live = LiveConfig::default().from_env()?;
    let core = CoreHandle::new(Arc::new(LiveStore::default())).await?;
    let data = config.data_dir.clone();
    let identities = auth.clone();
    let enabled = config.llm_enabled;
    core.with_gateway(move |g| -> Result<(), String> {
        g.identity_file(&data.join("gateway-ai.json"))?;
        g.store().assistant_context(data.clone(), data.join("gateway.mod"), "gateway".into())?;
        for credential in &identities.tokens {
            if let Some(owner) = &credential.owner
                && g.token_owner(&credential.token).as_ref() != Some(owner)
            {
                g.install_ai(
                    mg_live::gateway::AiIdentity {
                        id: credential.principal.id.clone(),
                        owner: owner.clone(),
                        name: credential.principal.name.clone(),
                        groups: credential.principal.groups.clone(),
                        expires_ms: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64
                            + 30 * 86_400_000,
                        llm_allowed: credential.llm_allowed,
                    },
                    &credential.token,
                )?;
                g.set_ai_avatar(&credential.principal.id, credential.avatar.as_deref())?;
            }
        }
        g.set_identity_observer(Arc::new(CancelUnpublished(g.store().clone())));
        g.set_llm_enabled(enabled);
        Ok(())
    })
    .await??;
    core.start_bridge(live.redis(), config.data_dir.clone()).await?;
    let app = mg_gateway::router(
        core.clone(),
        auth,
        Arc::new(live.policy),
        mg_gateway::Config::default(),
    );
    let listener = tokio::net::TcpListener::bind(config.address())
        .await
        .map_err(|_| "Cannot bind loopback gateway")?;
    println!(
        "Development gateway listening on {}",
        listener.local_addr().map_err(|_| "Cannot read listener address")?
    );
    let stopping = core.clone();
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            stopping.request_shutdown();
        })
        .await
        .map_err(|_| "Gateway listener failed".to_owned());
    core.shutdown().await?;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_loopback_and_tokens_never_appear_in_debug() {
        let token = "x".repeat(40);
        let mut config = Config::load(None, Some(token.clone())).unwrap();
        assert_eq!(config.address().ip(), Ipv4Addr::LOCALHOST);
        assert!(!config.llm_enabled);
        assert!(!config.groups.iter().any(|g| g == "scripts" || g == "chunks"));
        assert!(!format!("{config:?}").contains(&token));
        let auth = config.auth().unwrap();
        assert!(!format!("{auth:?}").contains(&token));
        let p = auth.authenticate(&token).unwrap();
        assert_eq!(p.id, "dm:dev");
        assert!(auth.authenticate("wrong").is_none());
        assert!(!p.scope(mg_live::access::DeliveryMode::Auto).unwrap().unrestricted());
    }
    #[test]
    fn private_configuration_can_explicitly_enable_scripts_and_add_ai() {
        let text=serde_json::json!({"token":"a".repeat(32),"groups":["read","scripts","chunks"],"tokens":[{"token":"b".repeat(32),"owner":"dm:dev","principal":{"id":"ai:test","name":"Test","groups":["read"]}}]}).to_string();
        let mut config = Config::load(Some(&text), Some("c".repeat(32))).unwrap();
        let auth = config.auth().unwrap();
        assert!(auth.authenticate(&"a".repeat(32)).is_none());
        assert!(auth.authenticate(&"c".repeat(32)).unwrap().groups.contains(&"scripts".into()));
        assert_eq!(auth.authenticate(&"b".repeat(32)).unwrap().id, "ai:test");
    }
    #[test]
    fn malformed_config_missing_credentials_and_reserved_names_fail_closed() {
        assert!(Config::load(Some(r#"{"bind":"0.0.0.0"}"#), None).is_err());
        assert!(Config::default().auth().is_err());
        let text=serde_json::json!({"tokens":[{"token":"x".repeat(32),"principal":{"id":"local","name":"Test","groups":["read"]}}]}).to_string();
        assert!(Config::load(Some(&text), None).unwrap().auth().is_err());
    }
    #[test]
    fn configured_ai_requires_an_owner_and_explicit_consent() {
        let value = serde_json::json!({"token":"d".repeat(32),"tokens":[{"token":"a".repeat(32),"principal":{"id":"ai:helper","name":"Helper","groups":["read"]}}]});
        assert!(Config::load(Some(&value.to_string()), None).unwrap().auth().is_err());
        let mut value = value;
        value["tokens"][0]["owner"] = serde_json::json!("dm:dev");
        let mut config = Config::load(Some(&value.to_string()), None).unwrap();
        let auth = config.auth().unwrap();
        assert!(!config.llm_enabled);
        assert!(!auth.tokens[0].llm_allowed);
        assert_eq!(auth.principal("dm:dev").unwrap().id, "dm:dev");
        assert!(auth.principal("dm:absent").is_none());
    }
}
