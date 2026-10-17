//! Configuration and one owner for the API and Redis worker lifetimes.
use crate::{
    api,
    bridge::{Bridge, RedisConfig},
    store::{LiveStore, Policy},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveConfig {
    pub enabled: bool,
    /// Explicit persisted user consent; environment overrides never enable this.
    pub allow_agents: bool,
    pub api_bind: String,
    pub api_port: u16,
    pub token: String,
    pub clients: Vec<crate::access::ClientToken>,
    pub redis_host: String,
    pub redis_port: u16,
    pub prefix: String,
    pub policy: Policy,
    /// The live events kept on disk ([`crate::logs`]).
    pub keep_logs: bool,
    /// And what players write among them.
    pub log_chat: bool,
    /// Windows: None automatically enables this for loopback Redis; Some overrides it.
    pub keep_wsl: Option<bool>,
}

// Options actions and settings are traced with Debug: credentials must never enter logs.
impl std::fmt::Debug for LiveConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveConfig")
            .field("enabled", &self.enabled)
            .field("allow_agents", &self.allow_agents)
            .field("api_bind", &self.api_bind)
            .field("api_port", &self.api_port)
            .field("token", &"<redacted>")
            .field("clients", &self.clients)
            .field("redis_host", &self.redis_host)
            .field("redis_port", &self.redis_port)
            .field("prefix", &self.prefix)
            .field("policy", &self.policy)
            .field("keep_logs", &self.keep_logs)
            .field("log_chat", &self.log_chat)
            .field("keep_wsl", &self.keep_wsl)
            .finish()
    }
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allow_agents: false,
            api_bind: "127.0.0.1".into(),
            api_port: 5199,
            token: String::new(),
            clients: Vec::new(),
            redis_host: "127.0.0.1".into(),
            redis_port: 6379,
            prefix: "mglive".into(),
            policy: Policy::default(),
            keep_logs: true,
            log_chat: false,
            keep_wsl: None,
        }
    }
}

/// Cryptographic credentials; never derive bearer tokens from command ids.
pub fn generate_token() -> Result<String, String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("Live token randomness: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

impl LiveConfig {
    pub fn from_env(self) -> Result<Self, String> {
        self.with_overrides(|name| std::env::var(name).ok())
    }

    fn with_overrides(mut self, get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        if let Some(value) = get("MOONGLOW_LIVE_TOKEN") {
            self.enabled = true;
            self.token = value;
        }
        if let Some(value) = get("MOONGLOW_LIVE_BIND") {
            self.enabled = true;
            let address: SocketAddr =
                value.parse().map_err(|_| "MOONGLOW_LIVE_BIND needs IP:port")?;
            self.api_bind = address.ip().to_string();
            self.api_port = address.port();
        }
        if let Some(value) = get("MOONGLOW_LIVE_REDIS") {
            self.enabled = true;
            if let Ok(address) = value.parse::<SocketAddr>() {
                self.redis_host = address.ip().to_string();
                self.redis_port = address.port();
            } else {
                let (host, port) =
                    value.rsplit_once(':').ok_or("MOONGLOW_LIVE_REDIS needs host:port")?;
                self.redis_host = host.into();
                self.redis_port = port.parse().map_err(|_| "Invalid Redis port")?;
            }
        }
        if let Some(value) = get("MOONGLOW_LIVE_PREFIX") {
            self.enabled = true;
            self.prefix = value;
        }
        if let Some(value) = get("MOONGLOW_LIVE_CHUNKS") {
            self.enabled = true;
            self.policy.chunks = value == "1";
        }
        Ok(self)
    }

    pub fn keep_wsl_enabled(&self) -> bool {
        cfg!(windows)
            && self.keep_wsl.unwrap_or_else(|| {
                self.redis_host.eq_ignore_ascii_case("localhost")
                    || self.redis_host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
            })
    }

    pub fn redis(&self) -> RedisConfig {
        RedisConfig {
            host: self.redis_host.clone(),
            port: self.redis_port,
            prefix: self.prefix.clone(),
        }
    }
}

#[derive(Debug)]
pub struct LiveService {
    config: LiveConfig,
    address: SocketAddr,
    certificate: Vec<u8>,
    stop: Arc<AtomicBool>,
    api: Option<JoinHandle<()>>,
    bridge: Bridge,
    descriptor: Option<crate::descriptor::Descriptor>,
    data_dir: Option<PathBuf>,
    store: Arc<LiveStore>,
    logs: Option<JoinHandle<()>>,
    logs_stop: Arc<AtomicBool>,
    wsl: crate::wsl::KeepWsl,
    access: Arc<crate::access::Access>,
}

impl LiveService {
    pub fn start(
        mut config: LiveConfig,
        store: Arc<LiveStore>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        if !config.enabled {
            return Err("Live mode is disabled".into());
        }
        config.redis().validate()?;
        if config.token.is_empty() {
            config.token = generate_token()?;
        }
        let ip: IpAddr =
            config.api_bind.parse().map_err(|_| "API address must be a loopback IP")?;
        let stop = Arc::new(AtomicBool::new(false));
        let access = Arc::new(crate::access::Access::new(config.token.clone(), &config.clients)?);
        access.agents.set_allowed(config.allow_agents);
        // Apply gates before opening the listener: a known client can connect immediately.
        store.set_policy(config.policy);
        store.set_audit(access.audit.clone());
        store.set_assistant_access(access.clone());
        store.require_command_identity();
        let wsl = crate::wsl::KeepWsl::start(config.keep_wsl_enabled())?;
        let listener = api::start_with_access(
            &SocketAddr::new(ip, config.api_port).to_string(),
            access.clone(),
            store.clone(),
            wake.clone(),
            stop.clone(),
        )?;
        match Bridge::start(config.redis(), store.clone(), wake, stop.clone()) {
            Ok(bridge) => {
                config.api_port = listener.address.port();
                Ok(Self {
                    config,
                    address: listener.address,
                    certificate: listener.certificate,
                    stop,
                    api: Some(listener.worker),
                    bridge,
                    descriptor: None,
                    data_dir: None,
                    store,
                    logs: None,
                    logs_stop: Arc::new(AtomicBool::new(false)),
                    wsl,
                    access,
                })
            }
            Err(error) => {
                stop.store(true, Ordering::Release);
                let _ = listener.worker.join();
                Err(error)
            }
        }
    }

    pub fn config(&self) -> &LiveConfig {
        &self.config
    }
    /// Desktop startup supplies its own data folder; headless tests need not
    /// write to a real profile. The record lives exactly as long as this service.
    pub fn publish_descriptor(&mut self, data_dir: &std::path::Path) -> Result<(), String> {
        if self.data_dir.as_ref().is_some_and(|old| old != data_dir) {
            return Err("Restart Live before changing its data directory".into());
        }
        let host = if self.config.redis_host.eq_ignore_ascii_case("localhost")
            || self.config.redis_host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
        {
            "loopback".to_string()
        } else {
            self.config.redis_host.to_ascii_lowercase()
        };
        let server = serde_json::to_string(&(host, self.config.redis_port, &self.config.prefix))
            .map_err(|e| e.to_string())?;
        self.store.set_command_identity(data_dir, &server)?;
        self.bridge.event_storage(data_dir, &server, &self.store)?;
        self.access.audit.open(data_dir)?;
        if self.data_dir.as_deref() != Some(data_dir) {
            self.remove_descriptor()?;
            self.data_dir = Some(data_dir.into());
        }
        self.ensure_descriptor()?;
        // (The events go to the same data folder, while the service runs.)
        if self.config.keep_logs && self.logs.is_none() {
            let (store, stop) = (self.store.clone(), self.logs_stop.clone());
            self.logs = Some(crate::logs::keep(store, data_dir, self.config.log_chat, stop));
        }
        Ok(())
    }
    fn ensure_descriptor(&mut self) -> Result<(), String> {
        if self.config.allow_agents
            && self.descriptor.is_none()
            && let Some(data_dir) = &self.data_dir
        {
            self.descriptor = Some(crate::descriptor::Descriptor::write(
                data_dir,
                &format!("https://{}", self.address),
                &self.config.token,
                &self.certificate,
            )?);
        }
        Ok(())
    }
    fn remove_descriptor(&mut self) -> Result<(), String> {
        if let Some(mut descriptor) = self.descriptor.take() {
            descriptor.remove()?;
        }
        Ok(())
    }
    /// Change only agent consent/discovery. The API listener, Redis and local UI stay alive.
    pub fn set_allow_agents(&mut self, allowed: bool) -> Result<(), String> {
        self.config.allow_agents = allowed;
        if !allowed {
            // Revoke access even if removing the discovery file fails.
            self.access.agents.set_allowed(false);
            return self.remove_descriptor();
        }
        if let Err(e) = self.ensure_descriptor() {
            self.config.allow_agents = false;
            self.access.agents.set_allowed(false);
            return Err(e);
        }
        self.access.agents.set_allowed(true);
        Ok(())
    }
    /// Authenticated clients seen within 60 seconds; no game-store lock or report clone.
    pub fn agents(&self, now_ms: u64) -> Vec<crate::AgentPresence> {
        self.access.agents.recent(now_ms)
    }
    /// Changes on consent/presence updates; expiry is time-based (last_ms + 60_000).
    pub fn agents_revision(&self) -> u64 {
        self.access.agents.revision()
    }
    pub fn certificate(&self) -> &[u8] {
        &self.certificate
    }
    pub fn status(&self) -> Value {
        let mut status = self.connection_status();
        status["allowAgents"] = json!(self.config.allow_agents);
        status["agents"] = json!(self.agents(crate::now_ms()));
        status
    }
    /// Connection diagnostics without cloning presence (for the Options panel).
    pub fn connection_status(&self) -> Value {
        json!({"api":format!("https://{}",self.address),"redis":self.bridge.status(),"keepWsl":self.config.keep_wsl_enabled(),"wslPid":self.wsl.pid(),"wslOwner":self.wsl.owner()})
    }
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.descriptor.take();
        self.stop.store(true, Ordering::Release);
        self.bridge.join();
        if let Some(worker) = self.api.take() {
            let _ = worker.join();
        }
        self.logs_stop.store(true, Ordering::Release);
        if let Some(worker) = self.logs.take() {
            let _ = worker.join();
        }
        self.wsl.stop();
    }
}

impl Drop for LiveService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_overrides_are_explicit() {
        let defaults = LiveConfig::default();
        assert!(!defaults.enabled && defaults.policy.scripts && !defaults.policy.chunks);
        assert!(!defaults.allow_agents);
        assert!(
            !serde_json::from_value::<LiveConfig>(json!({"enabled":true})).unwrap().allow_agents
        );
        let consent = LiveConfig { allow_agents: true, ..defaults.clone() };
        assert!(
            serde_json::from_value::<LiveConfig>(serde_json::to_value(&consent).unwrap())
                .unwrap()
                .allow_agents
        );
        assert!(consent.with_overrides(|_| None).unwrap().allow_agents);
        assert_eq!(defaults.clone().with_overrides(|_| None).unwrap(), defaults);
        let changed = defaults
            .with_overrides(|key| match key {
                "MOONGLOW_LIVE_BIND" => Some("[::1]:0".into()),
                "MOONGLOW_LIVE_REDIS" => Some("localhost:1234".into()),
                "MOONGLOW_LIVE_CHUNKS" => Some("1".into()),
                "MOONGLOW_LIVE_ALLOW_AGENTS" => Some("1".into()),
                _ => None,
            })
            .unwrap();
        assert!(changed.enabled && changed.policy.chunks);
        assert!(!changed.allow_agents, "environment cannot grant user consent");
        assert_eq!(changed.api_bind, "::1");
        assert_eq!(changed.redis_port, 1234);
        assert!(LiveConfig::default().with_overrides(|_| Some("invalid".into())).is_err());
    }

    #[test]
    fn token_and_shutdown_with_disconnected_redis() {
        let config = LiveConfig {
            enabled: true,
            api_port: 0,
            redis_port: 1,
            keep_wsl: Some(false),
            ..LiveConfig::default()
        };
        let store = Arc::new(LiveStore::default());
        let service = LiveService::start(config, store, Arc::new(|| {})).unwrap();
        assert_eq!(service.config().token.len(), 64);
        assert!(service.config().token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(service.config().token, generate_token().unwrap());
        let address = service.address;
        let before = std::time::Instant::now();
        service.stop();
        assert!(before.elapsed() < std::time::Duration::from_secs(2));
        assert!(std::net::TcpListener::bind(address).is_ok());
    }
    #[test]
    fn keep_wsl_defaults_follow_platform_and_redis_host() {
        let mut cfg = LiveConfig::default();
        assert_eq!(cfg.keep_wsl_enabled(), cfg!(windows));
        cfg.redis_host = "192.0.2.1".into();
        assert!(!cfg.keep_wsl_enabled());
        cfg.keep_wsl = Some(true);
        assert_eq!(cfg.keep_wsl_enabled(), cfg!(windows));
        cfg.redis_host = "::1".into();
        cfg.keep_wsl = None;
        assert_eq!(cfg.keep_wsl_enabled(), cfg!(windows));
        cfg.keep_wsl = Some(false);
        assert!(!cfg.keep_wsl_enabled());
    }
    #[test]
    #[cfg(windows)]
    #[ignore = "Starts one private owned WSL keepalive for a service lifecycle proof"]
    fn keep_wsl_service_lifecycle() {
        let service = LiveService::start(
            LiveConfig {
                enabled: true,
                api_port: 0,
                redis_port: 6392,
                keep_wsl: Some(true),
                ..Default::default()
            },
            Arc::new(LiveStore::default()),
            Arc::new(|| {}),
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let pid = service.status()["wslPid"].as_u64().expect("WSL child exited early");
        assert!(service.status()["keepWsl"].as_bool().unwrap());
        let owner = service.wsl.owner().to_owned();
        let linux_children = || {
            let output = std::process::Command::new("wsl.exe")
                .args(["-e", "python3", "-c", r#"import pathlib,sys,json
found=[]
for p in pathlib.Path('/proc').iterdir():
 if not p.name.isdigit(): continue
 try:
  if ('MOONGLOW_KEEP_OWNER='+sys.argv[1]).encode() in (p/'environ').read_bytes().split(b'\0') and (p/'cmdline').read_bytes().split(b'\0')[:2]==[b'sleep',b'infinity']: found.append(int(p.name))
 except (FileNotFoundError,PermissionError,ProcessLookupError): pass
print(json.dumps(found))"#, &owner])
                .output().unwrap();
            assert!(output.status.success());
            serde_json::from_slice::<Vec<u32>>(&output.stdout).unwrap()
        };
        let linux = linux_children();
        assert_eq!(linux.len(), 1, "one tagged Linux child must be alive");
        service.stop();
        for _ in 0..20 {
            if linux_children().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(linux_children().is_empty(), "owned Linux sleep leaked");
        println!(
            "Service stopped and reaped Windows child {pid}, Linux child {linux:?}; owner={owner}"
        );
    }
}
