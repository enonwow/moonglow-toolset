//! Host-configured module publishing. No client-supplied filesystem paths.
use crate::{App, Principal};
use axum::{
    Json,
    extract::{Path as UrlPath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use mg_live::resource_files::{ResourceGeneration, digest};
use mg_module::{Module, nwsync};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::io::AsyncReadExt;

/// Bounds outstanding response bodies, not just the time spent opening a file.
#[derive(Debug, Clone, Copy)]
pub struct DownloadLimits {
    pub global: usize,
    pub per_principal: usize,
    pub bytes: u64,
}
impl Default for DownloadLimits {
    fn default() -> Self {
        Self { global: 16, per_principal: 4, bytes: 64 * 1024 * 1024 }
    }
}
#[derive(Debug, Default)]
struct DownloadUsage {
    principals: BTreeMap<String, usize>,
    active: usize,
    bytes: u64,
}
#[derive(Debug)]
struct DownloadPermit {
    usage: Arc<Mutex<DownloadUsage>>,
    principal: String,
    bytes: u64,
    limit: u64,
}
impl DownloadPermit {
    fn reserve_bytes(&mut self, bytes: u64) -> bool {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        if bytes > self.limit.saturating_sub(usage.bytes) {
            return false;
        }
        usage.bytes += bytes;
        self.bytes = bytes;
        true
    }
}
impl Drop for DownloadPermit {
    fn drop(&mut self) {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        usage.active -= 1;
        usage.bytes -= self.bytes;
        if let Some(count) = usage.principals.get_mut(&self.principal) {
            *count -= 1;
            if *count == 0 {
                usage.principals.remove(&self.principal);
            }
        }
    }
}
#[derive(Debug)]
struct Published {
    generation: ResourceGeneration,
    paths: BTreeSet<String>,
}
#[derive(Debug)]
pub struct Publisher {
    root: PathBuf,
    state: Mutex<Vec<Published>>,
    build: Mutex<()>,
    limits: DownloadLimits,
    downloads: Arc<Mutex<DownloadUsage>>,
}
impl Publisher {
    pub fn new(root: PathBuf) -> Self {
        Self::with_download_limits(root, DownloadLimits::default()).expect("valid defaults")
    }
    pub fn with_download_limits(root: PathBuf, limits: DownloadLimits) -> Result<Self, String> {
        if limits.global == 0 || limits.per_principal == 0 || limits.bytes == 0 {
            return Err("Download limits must be positive".into());
        }
        Ok(Self {
            root,
            state: Mutex::new(vec![]),
            build: Mutex::new(()),
            limits,
            downloads: Arc::default(),
        })
    }
    fn acquire(&self, principal: &str) -> Option<DownloadPermit> {
        let mut usage = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
        if usage.active >= self.limits.global
            || usage.principals.get(principal).copied().unwrap_or(0) >= self.limits.per_principal
        {
            return None;
        }
        usage.active += 1;
        *usage.principals.entry(principal.into()).or_default() += 1;
        Some(DownloadPermit {
            usage: self.downloads.clone(),
            principal: principal.into(),
            bytes: 0,
            limit: self.limits.bytes,
        })
    }
    /// Blocking host job; archive/folder is read afresh. Content hashes detect changes,
    /// including same-size edits. All payloads are written before the descriptor changes.
    /// The supplied hak manifest is verified here, including its TLK identities.
    pub fn refresh(
        &self,
        module_path: &Path,
        boot: &str,
        haks_url: &str,
        haks_manifest: &str,
        hak_bytes: &[u8],
    ) -> Result<ResourceGeneration, String> {
        let _build = self.build.lock().map_err(|_| "publisher unavailable")?;
        if !digest(haks_manifest)
            || mg_core::sha1::hex(&mg_core::sha1::sha1(hak_bytes)) != haks_manifest
            || hak_bytes.len() > 16 * 1024 * 1024
        {
            return Err("invalid hak manifest".into());
        }
        let mut tlk = std::collections::BTreeMap::new();
        for (key, hash, _) in nwsync::read_manifest(hak_bytes)? {
            if key.restype == mg_core::ResType::TLK {
                tlk.insert(key.resref.to_lowercase().to_string(), mg_core::sha1::hex(&hash));
            }
        }
        let module = Module::open(module_path).map_err(|_| "cannot open publishing module")?;
        module.info().map_err(|_| "invalid publishing module")?;
        if let Some(name) = module.custom_tlk().map_err(|_| "invalid module TLK")? {
            let name = name.to_ascii_lowercase();
            let name = name.strip_suffix(".tlk").unwrap_or(&name);
            if !tlk.contains_key(name) {
                return Err("custom TLK missing from hak manifest".into());
            }
        }

        let contents = nwsync::module_only_contents(&module, true);
        let written = nwsync::write(
            &self.root,
            &contents,
            &nwsync::Options { limit: Some(nwsync::FILE_LIMIT), ..Default::default() },
            &mut |_, _| {},
        )
        .map_err(|_| "cannot publish module")?;
        let generation = ResourceGeneration {
            boot: boot.into(),
            module_manifest: written.sha1,
            haks_manifest: haks_manifest.into(),
            tlk,
            module_url: "/files/v1/".into(),
            haks_url: haks_url.into(),
        };
        generation.validate()?;
        let manifest_path = format!("manifests/{}", generation.module_manifest);
        let raw = std::fs::read(self.root.join(&manifest_path))
            .map_err(|_| "cannot read published manifest")?;
        let mut paths = BTreeSet::from([manifest_path]);
        for (_, hash, _) in nwsync::read_manifest(&raw)? {
            let hash = mg_core::sha1::hex(&hash);
            paths.insert(format!("data/sha1/{}/{}/{hash}", &hash[..2], &hash[2..4]));
        }
        let mut state = self.state.lock().map_err(|_| "publisher unavailable")?;
        if state.last().is_none_or(|p| p.generation != generation) {
            state.push(Published { generation: generation.clone(), paths });
            // Old in-flight generations may finish, without an unbounded authorization index.
            if state.len() > 4 {
                state.remove(0);
            }
        }
        Ok(generation)
    }
    pub fn current(&self, boot: &str) -> Option<ResourceGeneration> {
        self.state
            .lock()
            .ok()?
            .last()
            .filter(|p| p.generation.boot == boot)
            .map(|p| p.generation.clone())
    }
    pub fn disable(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.clear();
        }
    }
    fn open(&self, path: &str) -> Result<(std::fs::File, u64), String> {
        if !self
            .state
            .lock()
            .map_err(|_| "publisher unavailable")?
            .iter()
            .any(|p| p.paths.contains(path))
        {
            return Err("unknown resource".into());
        }
        let full = self.root.join(path);
        let metadata = std::fs::symlink_metadata(&full).map_err(|_| "missing resource")?;
        if !metadata.is_file() || metadata.len() > 17 * 1024 * 1024 {
            return Err("invalid resource".into());
        }
        let file = std::fs::File::open(full).map_err(|_| "missing resource")?;
        let meta = file.metadata().map_err(|_| "missing resource")?;
        if !meta.is_file() || meta.len() > 17 * 1024 * 1024 {
            return Err("invalid resource".into());
        }
        Ok((file, meta.len()))
    }
}
pub(crate) fn allowed(principal: &Principal) -> bool {
    principal.id.starts_with("dm:") && principal.groups.iter().any(|g| g == "read")
}
pub(crate) async fn handle(
    State(app): State<App>,
    UrlPath(path): UrlPath<String>,
    headers: HeaderMap,
) -> Response {
    let (_, principal) = match crate::authenticate(&app, &headers).await {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !allowed(&principal) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(publisher) = app.config.publisher else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if path == "generation" {
        let Ok(boot) = app.core.with_store(|s| s.boot()).await else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        return match publisher.current(&boot) {
            Some(g) => ([("cache-control", "no-store")], Json(g)).into_response(),
            None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
    }
    let Some(permit) = publisher.acquire(&principal.id) else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    // The worker owns its permit even if the HTTP task is cancelled during open.
    let opened = tokio::task::spawn_blocking(move || {
        publisher.open(&path).map(|(file, len)| (file, len, permit))
    })
    .await;
    let Ok(Ok((file, len, mut permit))) = opened else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !permit.reserve_bytes(len) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let stream = futures_util::stream::try_unfold(
        (tokio::fs::File::from_std(file).take(len), len, permit),
        |(mut file, remaining, permit)| async move {
            if remaining == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            let mut buf = vec![0u8; remaining.min(32 * 1024) as usize];
            let n = file.read(&mut buf).await?;
            if n == 0 {
                return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
            }
            buf.truncate(n);
            Ok(Some((buf, (file, remaining - n as u64, permit))))
        },
    );
    (
        [
            ("cache-control", "private, no-store"),
            ("content-type", "application/octet-stream"),
            ("content-length", &len.to_string()),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn global_and_byte_budgets_release_on_drop_without_retaining_principals() {
        let p = Publisher::with_download_limits(
            PathBuf::new(),
            DownloadLimits { global: 2, per_principal: 2, bytes: 10 },
        )
        .unwrap();
        let mut a = p.acquire("dm:a").unwrap();
        let mut b = p.acquire("dm:b").unwrap();
        assert!(p.acquire("dm:c").is_none());
        assert!(a.reserve_bytes(7));
        assert!(!b.reserve_bytes(4));
        assert!(b.reserve_bytes(3));
        drop(a);
        drop(b);
        let mut c = p.acquire("dm:c").unwrap();
        assert!(c.reserve_bytes(10));
        drop(c);
        let usage = p.downloads.lock().unwrap();
        assert!(usage.principals.is_empty());
        assert_eq!(usage.bytes, 0);
    }
}
