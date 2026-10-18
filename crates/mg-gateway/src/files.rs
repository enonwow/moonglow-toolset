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
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Mutex,
};

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
}
impl Publisher {
    pub fn new(root: PathBuf) -> Self {
        Self { root, state: Mutex::new(vec![]), build: Mutex::new(()) }
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
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
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
        std::fs::read(full).map_err(|_| "missing resource".into())
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
    match tokio::task::spawn_blocking(move || publisher.read(&path)).await {
        Ok(Ok(bytes)) => (
            [("cache-control", "private, no-store"), ("content-type", "application/octet-stream")],
            bytes,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
