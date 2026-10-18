//! Synchronous job-worker API. Four downloads at a time, no GUI or game-directory I/O.
use crate::{Gateway, SessionToken};
use mg_core::{
    ResType,
    sha1::{hex, sha1},
};
use mg_live::resource_files::ResourceGeneration;
use mg_module::{
    Module,
    nwsync_cache::{Limits, NwSyncCache},
};
use mg_resman::Container;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn check(&self) -> Result<(), String> {
        if self.is_cancelled() { Err("download cancelled".into()) } else { Ok(()) }
    }
}
#[derive(Debug, Clone)]
pub struct Progress {
    /// Network bytes received, including compressed files and manifests.
    pub bytes: u64,
    /// Completed network transfers; reused verified cache files require no transfer.
    pub files: u64,
    /// Repository-relative digest path, never a credential or remote URL.
    pub current: String,
}
/// Only constructed after both manifests, every blob and module folder validate.
#[derive(Debug)]
pub struct ReadyGeneration {
    generation: ResourceGeneration,
    module: Module,
}
impl ReadyGeneration {
    pub fn generation(&self) -> &ResourceGeneration {
        &self.generation
    }
    pub fn module(&self) -> &Module {
        &self.module
    }
}
#[derive(Debug, Default)]
pub struct Generations {
    state: Mutex<(Option<String>, Option<Arc<ReadyGeneration>>)>,
}
impl Generations {
    /// Call when welcome/files announces the desired generation. The old one stays visible.
    pub fn select(&self, generation: &ResourceGeneration) {
        self.state.lock().unwrap().0 = Some(generation.id());
    }
    /// A stale job cannot replace a newer selection. Clear renderer caches after true.
    pub fn activate(&self, ready: ReadyGeneration) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.0.as_deref() != Some(ready.generation.id().as_str()) {
            return false;
        }
        state.1 = Some(Arc::new(ready));
        true
    }
    /// Invalidate geometry immediately on a boot change/disconnect requiring reset.
    /// Verified disk blobs remain available for the next selected generation.
    pub fn clear(&self) {
        *self.state.lock().unwrap() = (None, None);
    }
    pub fn current(&self) -> Option<Arc<ReadyGeneration>> {
        self.state.lock().unwrap().1.clone()
    }
}
/// Use a dedicated caller-owned cache root, never the game's user directory.
#[derive(Debug)]
pub struct ResourceClient {
    gateway: Gateway,
    token: SessionToken,
    root: PathBuf,
    limits: Limits,
}
impl ResourceClient {
    pub fn new(gateway: Gateway, token: SessionToken, root: PathBuf, limits: Limits) -> Self {
        Self { gateway, token, root, limits }
    }
    /// Blocking: invoke from the GUI job worker. Cancel is checked before requests and
    /// on every read; socket inactivity is bounded to five seconds. No redirects.
    pub fn download(
        &self,
        generation: &ResourceGeneration,
        cancel: &Cancel,
        progress: &(impl Fn(Progress) + Sync),
    ) -> Result<ReadyGeneration, String> {
        generation.validate()?;
        cancel.check()?;
        let module_url =
            self.gateway.origin.join(&generation.module_url).map_err(|_| "invalid module URL")?;
        if module_url.origin() != self.gateway.origin.origin() || module_url.path() != "/files/v1/"
        {
            return Err("module URL must be the authenticated gateway files endpoint".into());
        }
        let haks_url = url::Url::parse(&generation.haks_url).map_err(|_| "invalid hak URL")?;
        for url in [&module_url, &haks_url] {
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !url.path().ends_with('/')
            {
                return Err("resource URL must be an HTTPS repository without credentials".into());
            }
        }
        let gateway_agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(Duration::from_secs(30))
            .timeout_read(Duration::from_secs(5))
            .tls_config(self.gateway.tls_config().map_err(|e| e.to_string())?)
            .build();
        let public_agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(Duration::from_secs(30))
            .timeout_read(Duration::from_secs(5))
            .build();
        let wire_limit = self
            .limits
            .total_bytes
            .saturating_add((self.limits.manifest_bytes as u64).saturating_mul(2));
        let bytes = AtomicU64::new(0);
        let files = AtomicU64::new(0);
        // Callback state is owned by each reader; the shared callback is invoked from workers.
        let counters = Arc::new((bytes, files));
        let fetch = |base: &url::Url,
                     authenticated: bool,
                     path: &str,
                     limit: usize|
         -> Result<Box<dyn Read>, String> {
            cancel.check()?;
            let url = base.join(path).map_err(|_| "invalid generated path")?;
            let agent = if authenticated || base.origin() == self.gateway.origin.origin() {
                &gateway_agent
            } else {
                &public_agent
            };
            let mut request = agent.get(url.as_str());
            if authenticated {
                request = request.set("Authorization", &format!("Bearer {}", self.token.expose()));
            }
            let response = request.call().map_err(|error| match error {
                ureq::Error::Status(status, _) => format!("resource HTTP {status}"),
                ureq::Error::Transport(error) => format!("resource transport {:?}", error.kind()),
            })?;
            if response.status() != 200 {
                return Err("resource request failed".into());
            }
            // Read bodies inside this callback so no borrowed observer escapes its worker.
            if response
                .header("Content-Length")
                .and_then(|n| n.parse::<u64>().ok())
                .is_some_and(|n| n > limit as u64)
            {
                return Err("resource download exceeds byte limit".into());
            }
            let mut reader = response.into_reader();
            let mut body = Vec::new();
            let mut buf = [0; 16384];
            loop {
                cancel.check()?;
                let room = (limit + 1).saturating_sub(body.len()).min(buf.len());
                if room == 0 {
                    return Err("resource download exceeds byte limit".into());
                }
                let n = reader.read(&mut buf[..room]).map_err(|_| "resource read failed")?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
                let received = counters.0.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
                if received > wire_limit {
                    return Err("generation wire byte limit exceeded".into());
                }
                progress(Progress {
                    bytes: received,
                    files: counters.1.load(Ordering::Relaxed),
                    current: path.into(),
                });
                if body.len() > limit {
                    return Err("resource download exceeds byte limit".into());
                }
            }
            let done = counters.1.fetch_add(1, Ordering::Relaxed) + 1;
            progress(Progress {
                bytes: counters.0.load(Ordering::Relaxed),
                files: done,
                current: path.into(),
            });
            Ok(Box::new(std::io::Cursor::new(body)))
        };
        let cache = self.root.join("cache");
        let haks = NwSyncCache::fetch_parallel(
            &cache,
            &generation.haks_manifest,
            self.limits,
            &|path, limit| fetch(&haks_url, false, path, limit),
        )?;
        let mut remaining = self.limits;
        remaining.total_bytes = remaining
            .total_bytes
            .checked_sub(haks.decoded_bytes())
            .ok_or("generation size limit")?;
        let module = NwSyncCache::fetch_parallel(
            &cache,
            &generation.module_manifest,
            remaining,
            &|path, limit| fetch(&module_url, true, path, limit),
        )?;
        let mut tlk = BTreeMap::new();
        for key in haks.keys().filter(|k| k.restype == ResType::TLK) {
            let data = haks.read(&key).map_err(|_| "invalid cached TLK")?;
            tlk.insert(key.resref.to_lowercase().to_string(), hex(&sha1(&data)));
        }
        if tlk != generation.tlk {
            return Err("generation TLK mismatch".into());
        }
        cancel.check()?;
        let generations = self.root.join("generations");
        std::fs::create_dir_all(&generations).map_err(|_| "cannot create generation directory")?;
        let opened = module.open_module(&generations.join(generation.id()), haks)?;
        if let Some(name) = opened.custom_tlk().map_err(|_| "invalid module TLK")? {
            let name = name.to_ascii_lowercase();
            if !generation.tlk.contains_key(name.strip_suffix(".tlk").unwrap_or(&name)) {
                return Err("custom TLK missing from generation".into());
            }
        }

        cancel.check()?;
        // Persist only the complete descriptor as a recovery hint; Generations owns activation.
        persist(&self.root.join("prepared.json"), generation)?;
        Ok(ReadyGeneration { generation: generation.clone(), module: opened })
    }
}
fn persist(path: &Path, generation: &ResourceGeneration) -> Result<(), String> {
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or("invalid cache root")?)
        .map_err(|_| "cannot stage generation")?;
    temp.write_all(&serde_json::to_vec(generation).map_err(|_| "invalid descriptor")?)
        .map_err(|_| "cannot write generation")?;
    temp.as_file().sync_all().map_err(|_| "cannot sync generation")?;
    temp.persist(path).map_err(|_| "cannot commit generation")?;
    Ok(())
}
