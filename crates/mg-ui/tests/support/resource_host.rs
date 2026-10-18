//! Authored NWSync fixtures following the link resource and gateway files tests.
#![allow(dead_code)]
use axum::{
    extract::{Path as UrlPath, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use mg_core::ResType;
use mg_gateway::{Authenticator, Config, CoreHandle, Principal, files::Publisher};
use mg_live::store::{LiveStore, Policy};
use mg_live_link::{
    SessionToken,
    credentials::{Backend, StoreError},
};
use mg_module::{
    Module, ModuleLocation,
    nwsync::{self, Source},
};
use mg_resman::ResKey;
use mg_ui::{Moonglow, NoDialogs, OptionsDraft};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};

pub(crate) struct Credentials;
impl Backend for Credentials {
    fn get(&self, _: &str) -> Result<Option<SessionToken>, StoreError> {
        Ok(Some(SessionToken::new("fixture-dm".into()).unwrap()))
    }
    fn set(&self, _: &str, _: &SessionToken) -> Result<(), StoreError> {
        Ok(())
    }
    fn delete(&self, _: &str) -> Result<(), StoreError> {
        Ok(())
    }
}
#[derive(Debug)]
struct Auth;
impl Authenticator for Auth {
    fn authenticate(&self, token: &str) -> Option<Principal> {
        (token == "fixture-dm").then(|| Principal {
            id: "dm:test".into(),
            name: "Guide".into(),
            groups: vec!["read".into()],
        })
    }
}
#[derive(Clone)]
struct Controls {
    rows: Arc<HashMap<String, Vec<u8>>>,
    delay: Arc<AtomicU64>,
    corrupt: Arc<AtomicBool>,
    interrupt: Arc<AtomicBool>,
    seen: Arc<Mutex<Vec<String>>>,
}
async fn assets(State(c): State<Controls>, UrlPath(path): UrlPath<String>) -> Response {
    c.rows
        .get(&path)
        .cloned()
        .map_or_else(|| StatusCode::NOT_FOUND.into_response(), IntoResponse::into_response)
}
async fn fault(State(c): State<Controls>, req: axum::extract::Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    if path.starts_with("/assets/") {
        assert!(!req.headers().contains_key("authorization"));
    }
    if path.starts_with("/files/v1/") {
        assert_eq!(req.headers().get("authorization").unwrap(), "Bearer fixture-dm");
    }
    if path.starts_with("/assets/") || path.starts_with("/files/v1/") {
        c.seen.lock().unwrap().push(path.clone());
        if path.contains("/data/sha1/") && c.interrupt.swap(false, Ordering::Relaxed) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        tokio::time::sleep(Duration::from_millis(c.delay.load(Ordering::Relaxed))).await;
        if path.contains("/data/sha1/") && c.corrupt.load(Ordering::Relaxed) {
            return b"tampered fixture".to_vec().into_response();
        }
    }
    next.run(req).await
}
pub(crate) struct Host {
    pub(crate) origin: String,
    pub(crate) pin: String,
    pub(crate) core: CoreHandle,
    pub(crate) publisher: Arc<Publisher>,
    pub(crate) delay: Arc<AtomicU64>,
    pub(crate) corrupt: Arc<AtomicBool>,
    pub(crate) interrupt: Arc<AtomicBool>,
    pub(crate) seen: Arc<Mutex<Vec<String>>>,
    module: PathBuf,
    hash: String,
    manifest: Vec<u8>,
    tasks: Vec<JoinHandle<()>>,
}
pub(crate) fn module(path: &Path, area: &str) {
    let mut module = Module::new();
    let mut info = mg_gff::Gff::new(*b"IFO ");
    let mut entry = mg_gff::Struct::new(6);
    entry.set("Area_Name", mg_gff::Value::ResRef(area.as_bytes().to_vec()));
    info.root.set("Mod_Area_list", mg_gff::Value::List(vec![entry]));
    module.set_info(&info).unwrap();
    let mut are = mg_gff::Gff::new(*b"ARE ");
    are.root.set("Width", mg_gff::Value::Int(2));
    are.root.set("Height", mg_gff::Value::Int(2));
    module.set(ResKey::parse(area, ResType::ARE).unwrap(), are.to_bytes().unwrap());
    module.set(
        ResKey::parse(area, ResType::GIT).unwrap(),
        mg_gff::Gff::new(*b"GIT ").to_bytes().unwrap(),
    );
    module.save_as(&ModuleLocation::Archive(path.into())).unwrap();
}
impl Host {
    pub(crate) async fn new(root: &Path) -> Self {
        let repo = root.join("repo");
        // The same authored small blobs used by the link HTTPS tests, no game assets.
        let entries: Vec<_> = (0..8)
            .map(|i| {
                (
                    ResKey::parse(&format!("file{i}"), ResType::TXT).unwrap(),
                    Source::Bytes(vec![i; 4096].into()),
                )
            })
            .collect();
        let manifest = nwsync::write(&repo, &entries, &Default::default(), &mut |_, _| {}).unwrap();
        let hash = manifest.sha1;
        let manifest = std::fs::read(repo.join("manifests").join(&hash)).unwrap();
        fn scan(root: &Path, path: &Path, rows: &mut HashMap<String, Vec<u8>>) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    scan(root, &path, rows);
                } else {
                    rows.insert(
                        path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"),
                        std::fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut rows = HashMap::new();
        scan(&repo, &repo, &mut rows);
        let controls = Controls {
            rows: Arc::new(rows),
            delay: Arc::new(AtomicU64::new(80)),
            corrupt: Arc::new(AtomicBool::new(false)),
            interrupt: Arc::new(AtomicBool::new(false)),
            seen: Arc::new(Mutex::new(vec![])),
        };
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let pin = mg_live_link::fingerprint(cert.cert.der().as_ref());
        let tls = tokio_rustls::TlsAcceptor::from(Arc::new(
            rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.cert.der().clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
            )
            .unwrap(),
        ));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("https://{}", listener.local_addr().unwrap());
        let module_path = root.join("fixture.mod");
        module(&module_path, "square");
        let publisher = Arc::new(Publisher::new(root.join("published")));
        publisher
            .refresh(&module_path, "boot", &format!("{origin}/assets/"), &hash, &manifest)
            .unwrap();
        let store = Arc::new(LiveStore::default());
        store.sync(br#"{"v":1,"boot":"boot","areas":[]}"#, mg_live::now_ms()).unwrap();
        let core = CoreHandle::new(store).await.unwrap();
        let router = mg_gateway::router(
            core.clone(),
            Arc::new(Auth),
            Arc::new(Policy::default()),
            Config { publisher: Some(publisher.clone()), ..Default::default() },
        )
        .merge(
            axum::Router::new()
                .route("/assets/{*path}", axum::routing::get(assets))
                .with_state(controls.clone()),
        )
        .layer(middleware::from_fn_with_state(controls.clone(), fault));
        let plain = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = plain.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(plain, router).await.unwrap();
        });
        let proxy = tokio::spawn(async move {
            let mut workers = tokio::task::JoinSet::new();
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let tls = tls.clone();
                workers.spawn(async move {
                    if let Ok(mut socket) = tls.accept(socket).await
                        && let Ok(mut target) = tokio::net::TcpStream::connect(address).await
                    {
                        let _ = tokio::io::copy_bidirectional(&mut socket, &mut target).await;
                    }
                });
            }
        });
        Self {
            origin,
            pin,
            core,
            publisher,
            delay: controls.delay,
            corrupt: controls.corrupt,
            interrupt: controls.interrupt,
            seen: controls.seen,
            module: module_path,
            hash,
            manifest,
            tasks: vec![server, proxy],
        }
    }
    pub(crate) fn change_area(&self, area: &str) {
        module(&self.module, area);
        self.publisher
            .refresh(
                &self.module,
                "boot",
                &format!("{}/assets/", self.origin),
                &self.hash,
                &self.manifest,
            )
            .unwrap();
    }
    pub(crate) fn app(&self) -> Moonglow {
        let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
        app.recovery_dir = None;
        app.live_assistant.data_dir = None;
        app.game = Some(Arc::new(mg_rules::GameData::new(
            mg_resman::ResMan::new(),
            mg_tlk::Tlk::new(mg_core::Language::ENGLISH),
        )));
        app.install = Some(mg_resman::GameInstall::new("absent-game", None, "en"));
        let id = app.settings.live_servers.add();
        let server = app.settings.live_servers.servers.iter_mut().find(|s| s.id == id).unwrap();
        server.name = "Campaign server".into();
        server.url = self.origin.clone();
        server.cert_pin = Some(self.pin.clone());
        app.settings.live.enabled = true;
        app
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
pub(crate) fn draw(data: PathBuf) -> impl FnMut(&mut egui::Ui, &mut Moonglow) {
    let mut started = false;
    move |ui, app| {
        if !started {
            OptionsDraft::set_live_credential_backend(
                ui.ctx(),
                Arc::new(Credentials),
                data.clone(),
            );
            app.live_focus.data_dir = Some(data.clone());
            app.apply_live(ui.ctx());
            started = true;
        }
        app.ui(ui);
    }
}
pub(crate) async fn pump(
    h: &mut egui_kittest::Harness<'_, Moonglow>,
    mut done: impl FnMut(&Moonglow) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            h.run_steps(2);
            if done(h.state()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("resource UI deadline: {:?}", h.state().log.entries));
}

pub(crate) struct Scratch(tempfile::TempDir);
impl Scratch {
    pub(crate) fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    pub(crate) fn path(&self) -> &Path {
        self.0.path()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        #[cfg(windows)]
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "Windows fixture-owned cache cleanup"
        )]
        fn writable(path: &Path) {
            let meta = std::fs::symlink_metadata(path).unwrap();
            assert!(!meta.file_type().is_symlink());
            if meta.is_dir() {
                for e in std::fs::read_dir(path).unwrap() {
                    writable(&e.unwrap().path());
                }
            } else {
                let mut permissions = meta.permissions();
                permissions.set_readonly(false);
                std::fs::set_permissions(path, permissions).unwrap();
            }
        }
        #[cfg(windows)]
        writable(self.path());
    }
}
pub(crate) fn blob_count(root: &Path) -> usize {
    if !root.exists() {
        return 0;
    }
    std::fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == "blobs") {
                std::fs::read_dir(p).unwrap().count()
            } else if p.is_dir() {
                blob_count(&p)
            } else {
                0
            }
        })
        .sum()
}
