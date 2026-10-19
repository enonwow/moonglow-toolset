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
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug)]
struct Session {
    prefix: String,
    generation: u64,
    touched: std::time::Instant,
}
#[derive(Debug)]
struct Core {
    gateway: Gateway,
    sessions: Mutex<HashMap<String, Session>>,
    bridge: Mutex<Option<Bridge>>,
    stopped: AtomicBool,
    /// Bumped when the Redis bridge applied a report, event or result, so session pumps deliver
    /// it at once instead of at their next poll.
    wake: Arc<tokio::sync::watch::Sender<u64>>,
}
/// Every closure runs on Tokio's blocking pool; no store guard crosses an await.
#[derive(Debug, Clone)]
pub struct CoreHandle(Arc<Core>);
impl CoreHandle {
    pub async fn new(store: Arc<LiveStore>) -> Result<Self, String> {
        let core = tokio::task::spawn_blocking(move || -> Result<Self, String> {
            Ok(Self(Arc::new(Core {
                gateway: Gateway::new(store)?,
                sessions: Mutex::new(HashMap::new()),
                bridge: Mutex::new(None),
                stopped: AtomicBool::new(false),
                wake: Arc::new(tokio::sync::watch::channel(0).0),
            })))
        })
        .await
        .map_err(|e| e.to_string())??;
        let weak = Arc::downgrade(&core.0);
        tokio::spawn(async move {
            let mut last_reap = std::time::Instant::now();
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let Some(core) = weak.upgrade() else {
                    break;
                };
                if core.stopped.load(Ordering::Acquire) {
                    break;
                }
                let reap = last_reap.elapsed() >= std::time::Duration::from_secs(1);
                if reap {
                    last_reap = std::time::Instant::now();
                }
                let _ = tokio::task::spawn_blocking(move || {
                    core.gateway.tick(crate::now())?;
                    if reap {
                        core.reap(std::time::Instant::now())?;
                    }
                    Ok::<(), String>(())
                })
                .await;
            }
        });
        Ok(core)
    }
    /// Trusted host configuration and core requests, always on a blocking worker.
    pub async fn with_gateway<R: Send + 'static>(
        &self,
        f: impl FnOnce(&Gateway) -> R + Send + 'static,
    ) -> Result<R, String> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || f(&core.gateway)).await.map_err(|e| e.to_string())
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
    pub async fn start_bridge(
        &self,
        config: RedisConfig,
        data: std::path::PathBuf,
    ) -> Result<(), String> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut bridge = core.bridge.lock().unwrap_or_else(|e| e.into_inner());
            if bridge.is_some() {
                return Err("Bridge already started".into());
            }
            let wake = core.wake.clone();
            *bridge = Some(Bridge::start_persistent(
                config,
                core.gateway.store().clone(),
                Arc::new(move || {
                    wake.send_modify(|n| *n = n.wrapping_add(1));
                }),
                Arc::new(AtomicBool::new(false)),
                &data,
            )?);
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    /// Publish an auxiliary push after the host has authorized its audience.
    pub async fn publish(
        &self,
        principal: String,
        push: mg_live::gateway::Push,
    ) -> Result<(), String> {
        let core = self.0.clone();
        tokio::task::spawn_blocking(move || core.gateway.publish_session(&principal, push))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Resolves whenever the bridge applied new game data (coalesced); pumps also poll.
    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.0.wake.subscribe()
    }
    pub fn request_shutdown(&self) {
        self.0.stopped.store(true, Ordering::Release);
    }
    pub fn is_shutdown(&self) -> bool {
        self.0.stopped.load(Ordering::Acquire)
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        self.request_shutdown();
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
            core.reap(std::time::Instant::now())?;
            let mut sessions = core.sessions.lock().unwrap_or_else(|e| e.into_inner());
            if !sessions.contains_key(&principal.id) && sessions.len() >= 1024 {
                return Err("Session capacity reached".into());
            }
            let session = sessions.entry(principal.id.clone()).or_insert(Session {
                prefix: format!("{}.", &mg_live::service::generate_token()?[..12]),
                generation: 0,
                touched: std::time::Instant::now(),
            });
            session.generation += 1;
            session.touched = std::time::Instant::now();
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
            let owner = core.gateway.ai_owner(&principal.id);
            let avatar = core.gateway.ai_avatar(&principal.id);
            core.gateway.set_identity(
                &principal.id,
                &principal.name,
                avatar.as_deref(),
                owner.as_deref(),
            )?;
            let resources = if crate::files::allowed(&principal) {
                config.publisher.as_ref().and_then(|p| p.current(&core.gateway.store().boot()))
            } else {
                None
            };
            let nwsync = resources
                .map(|g| mg_live::gateway_frames::Nwsync {
                    haks_url: g.haks_url.clone(),
                    haks_manifest: g.haks_manifest.clone(),
                    module_manifest: g.module_manifest.clone(),
                    generation: Some(g),
                })
                .or_else(|| if crate::files::allowed(&principal) { config.nwsync } else { None });
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
                    avatar,
                },
                groups: principal.groups,
                policy: core.gateway.store().policy(),
                id_prefix: session.prefix.clone(),
                limits: Limits { watch: config.watch_limit, ..Limits::default() },
                features: config.features,
                nwsync,
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
            let mut sessions = core.sessions.lock().unwrap_or_else(|e| e.into_inner());
            if sessions.get(&id).is_none_or(|s| s.generation != generation) {
                return Err("replaced".into());
            }
            sessions.get_mut(&id).expect("checked session").touched = std::time::Instant::now();
            f(&core.gateway, &id)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
impl Core {
    fn reap(&self, now: std::time::Instant) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let expired: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| {
                now.saturating_duration_since(s.touched)
                    >= std::time::Duration::from_millis(mg_live::gateway::RESULT_RETENTION_MS)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            self.gateway.remove_session(&id, crate::now())?;
            sessions.remove(&id);
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn abandoned_sessions_release_watch_capacity_and_cannot_resume() {
        let core = CoreHandle::new(Arc::new(LiveStore::default())).await.unwrap();
        let principal =
            Principal { id: "dm:test".into(), name: "Test".into(), groups: vec!["read".into()] };
        let hello = Hello {
            proto: 1,
            mirror: MIRROR_VERSION,
            client: "test".into(),
            resume: None,
            watch: vec!["square".into()],
        };
        let (_, welcome) =
            core.connect(principal.clone(), hello.clone(), Config::default()).await.unwrap();
        let inner = core.0.clone();
        tokio::task::spawn_blocking(move || {
            let touched = inner.sessions.lock().unwrap()["dm:test"].touched;
            inner
                .reap(
                    touched
                        + std::time::Duration::from_millis(
                            mg_live::gateway::RESULT_RETENTION_MS - 1,
                        ),
                )
                .unwrap();
            assert!(inner.sessions.lock().unwrap().contains_key("dm:test"));
            let areas: Vec<_> = (0..64).map(|i| format!("zone{i}")).collect();
            assert!(
                inner
                    .gateway
                    .store()
                    .set_watched_areas_owned("probe", &areas, crate::now())
                    .is_err()
            );
            inner
                .reap(
                    touched
                        + std::time::Duration::from_millis(mg_live::gateway::RESULT_RETENTION_MS),
                )
                .unwrap();
            assert!(inner.sessions.lock().unwrap().is_empty());
            assert!(inner.gateway.pushes("dm:test", crate::now()).is_err());
            let areas: Vec<_> = (0..64).map(|i| format!("zone{i}")).collect();
            inner.gateway.store().set_watched_areas_owned("probe", &areas, crate::now()).unwrap();
            inner.gateway.store().set_watched_areas_owned("probe", &[], crate::now()).unwrap();
        })
        .await
        .unwrap();
        let mut hello = hello;
        hello.resume =
            Some(mg_live::gateway::Resume { gw_epoch: welcome.gw_epoch, ..Default::default() });
        let (_, next) = core.connect(principal, hello, Config::default()).await.unwrap();
        assert!(!next.resumed);
        assert_ne!(welcome.id_prefix, next.id_prefix);
        core.shutdown().await.unwrap();
    }
}
