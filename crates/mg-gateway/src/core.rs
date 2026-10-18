use crate::{Config, Principal};
use mg_live::{
    MIRROR_VERSION,
    bridge::{Bridge, RedisConfig},
    gateway::{Gateway, SessionConfig},
    gateway_frames::{Hello, Limits, PROTOCOL_VERSION, UserIdentity, Welcome},
    store::LiveStore,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

#[derive(Debug)]
struct Session {
    prefix: String,
    generation: u64,
}
#[derive(Debug)]
struct Core {
    gateway: Gateway,
    sessions: Mutex<HashMap<String, Session>>,
    bridge: Mutex<Option<Bridge>>,
}
/// Every closure runs on Tokio's blocking pool; no store guard crosses an await.
#[derive(Debug, Clone)]
pub struct CoreHandle(Arc<Core>);
impl CoreHandle {
    pub async fn new(store: Arc<LiveStore>) -> Result<Self, String> {
        tokio::task::spawn_blocking(move || {
            Ok(Self(Arc::new(Core {
                gateway: Gateway::new(store)?,
                sessions: Mutex::new(HashMap::new()),
                bridge: Mutex::new(None),
            })))
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn with_store<R: Send + 'static>(
        &self,
        f: impl FnOnce(&LiveStore) -> R + Send + 'static,
    ) -> Result<R, String> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || f(core.gateway.store()))
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn start_bridge(&self, config: RedisConfig) -> Result<(), String> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut bridge = core.bridge.lock().unwrap_or_else(|e| e.into_inner());
            if bridge.is_some() {
                return Err("Bridge already started".into());
            }
            *bridge = Some(Bridge::start(
                config,
                core.gateway.store().clone(),
                Arc::new(|| {}),
                Arc::new(AtomicBool::new(false)),
            )?);
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || {
            core.bridge.lock().unwrap_or_else(|e| e.into_inner()).take();
        })
        .await
        .map_err(|e| e.to_string())
    }
    pub(crate) async fn connect(
        &self,
        principal: Principal,
        hello: Hello,
        config: Config,
    ) -> Result<(u64, Welcome), String> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || {
            principal.validate()?;
            let mut sessions = core.sessions.lock().unwrap_or_else(|e| e.into_inner());
            if !sessions.contains_key(&principal.id) && sessions.len() >= 1024 {
                return Err("Session capacity reached".into());
            }
            let session = sessions.entry(principal.id.clone()).or_insert(Session {
                prefix: format!("{}.", &mg_live::service::generate_token()?[..12]),
                generation: 0,
            });
            session.generation += 1;
            let resumed = core.gateway.connect(
                SessionConfig {
                    id: principal.id.clone(),
                    id_prefix: session.prefix.clone(),
                    groups: principal.groups.clone(),
                    watch_limit: config.watch_limit,
                },
                hello.resume.as_ref(),
                &hello.watch,
                crate::now(),
            )?;
            let welcome = Welcome {
                proto: PROTOCOL_VERSION,
                mirror: MIRROR_VERSION,
                gw_epoch: core.gateway.epoch().into(),
                now_ms: crate::now(),
                server: config.server,
                user: UserIdentity {
                    kind: principal.id.split(':').next().unwrap_or("dm").into(),
                    id: principal.id,
                    name: principal.name,
                    avatar: None,
                },
                groups: principal.groups,
                policy: core.gateway.store().policy(),
                id_prefix: session.prefix.clone(),
                limits: Limits { watch: config.watch_limit, ..Limits::default() },
                features: vec![],
                nwsync: None,
                resumed,
            };
            Ok((session.generation, welcome))
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub(crate) async fn session<R: Send + 'static>(
        &self,
        id: String,
        generation: u64,
        f: impl FnOnce(&Gateway, &str) -> Result<R, String> + Send + 'static,
    ) -> Result<R, String> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let sessions = core.sessions.lock().unwrap_or_else(|e| e.into_inner());
            if sessions.get(&id).is_none_or(|s| s.generation != generation) {
                return Err("replaced".into());
            }
            f(&core.gateway, &id)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
impl Drop for Core {
    fn drop(&mut self) {
        if let Some(bridge) = self.bridge.get_mut().unwrap_or_else(|e| e.into_inner()).take() {
            // Joining Redis threads may block; even a forgotten shutdown cannot block Tokio.
            let _ =
                std::thread::Builder::new().name("gateway-stop".into()).spawn(move || drop(bridge));
        }
    }
}
