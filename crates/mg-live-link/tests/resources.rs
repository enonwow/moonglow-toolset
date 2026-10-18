use mg_core::{
    ResType,
    sha1::{hex, sha1},
};
use mg_live::resource_files::ResourceGeneration;
use mg_live_link::{
    Gateway, SessionToken,
    resources::{Cancel, Generations, ResourceClient},
};
use mg_module::{
    Module,
    nwsync::{self, Options, Source},
    nwsync_cache::Limits,
};
use mg_resman::ResKey;
use std::{
    collections::{BTreeMap, HashMap},
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

struct Server {
    url: String,
    pin: String,
    rows: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    seen: Arc<Mutex<Vec<String>>>,
    peak: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(rows: HashMap<String, Vec<u8>>) -> Self {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let pin = mg_live_link::fingerprint(cert.cert.der().as_ref());
        let tls = Arc::new(
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
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        let rows = Arc::new(Mutex::new(rows));
        let seen = Arc::new(Mutex::new(vec![]));
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicUsize::new(0));
        let (r, s, c, p) = (rows.clone(), seen.clone(), stop.clone(), peak.clone());
        let thread = std::thread::spawn(move || {
            let active = Arc::new(AtomicUsize::new(0));
            let mut workers = vec![];
            while !c.load(Ordering::Acquire) {
                let Ok((socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                };
                let (r, s, tls, active, p) =
                    (r.clone(), s.clone(), tls.clone(), active.clone(), p.clone());
                workers.push(std::thread::spawn(move || {
                    socket.set_nonblocking(false).unwrap();
                    socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                    socket.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
                    let mut stream = rustls::StreamOwned::new(
                        rustls::ServerConnection::new(tls).unwrap(),
                        socket,
                    );
                    let mut headers = vec![];
                    while !headers.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        if stream.read_exact(&mut byte).is_err() {
                            return;
                        }
                        headers.push(byte[0]);
                        if headers.len() > 8192 {
                            return;
                        }
                    }
                    let text = String::from_utf8(headers).unwrap();
                    let path = text.split_whitespace().nth(1).unwrap().to_owned();
                    if path.starts_with("/files/") {
                        assert!(
                            text.to_ascii_lowercase()
                                .contains("authorization: bearer fixture-session")
                        );
                    } else {
                        assert!(!text.to_ascii_lowercase().contains("authorization:"));
                    }
                    s.lock().unwrap().push(path.clone());
                    let n = active.fetch_add(1, Ordering::SeqCst) + 1;
                    p.fetch_max(n, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(25));
                    let body = r.lock().unwrap().get(&path).cloned();
                    active.fetch_sub(1, Ordering::SeqCst);
                    let status = if body.is_some() { 200 } else { 404 };
                    let body = body.unwrap_or_default();
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                    stream.conn.send_close_notify();
                    let _ = stream.flush();
                }));
            }
            for w in workers {
                w.join().unwrap();
            }
        });
        Self { url, pin, rows, seen, peak, stop, thread: Some(thread) }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    server: Server,
    spec: ResourceGeneration,
    cache: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let mut module = Module::new();
        module.set_info(&mg_gff::Gff::new(*b"IFO ")).unwrap();
        let opts = Options::default();
        let m = nwsync::write(
            &repo,
            &nwsync::module_only_contents(&module, true),
            &opts,
            &mut |_, _| {},
        )
        .unwrap();
        let mut contents: Vec<_> = (0..8)
            .map(|i| {
                (
                    ResKey::parse(&format!("file{i}"), ResType::TXT).unwrap(),
                    Source::Bytes(vec![i; 4096].into()),
                )
            })
            .collect();
        // Authored empty TLK V3.0 (language 0, no entries, string section at 20).
        let mut tlk_bytes = b"TLK V3.0".to_vec();
        tlk_bytes.extend_from_slice(&0u32.to_le_bytes());
        tlk_bytes.extend_from_slice(&0u32.to_le_bytes());
        tlk_bytes.extend_from_slice(&20u32.to_le_bytes());
        let tlk_hash = hex(&sha1(&tlk_bytes));
        contents.push((
            ResKey::parse("custom", ResType::TLK).unwrap(),
            Source::Bytes(tlk_bytes.into()),
        ));
        let h = nwsync::write(&repo, &contents, &opts, &mut |_, _| {}).unwrap();
        let mut rows = HashMap::new();
        fn scan(
            dir: &std::path::Path,
            root: &std::path::Path,
            rows: &mut HashMap<String, Vec<u8>>,
        ) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    scan(&p, root, rows);
                } else {
                    let path = p.strip_prefix(root).unwrap().to_str().unwrap().replace('\\', "/");
                    let b = std::fs::read(&p).unwrap();
                    rows.insert(format!("/assets/{path}"), b.clone());
                    rows.insert(format!("/files/v1/{path}"), b);
                }
            }
        }
        scan(&repo, &repo, &mut rows);
        let server = Server::new(rows);
        let spec = ResourceGeneration {
            boot: "boot-1".into(),
            module_manifest: m.sha1,
            haks_manifest: h.sha1,
            tlk: BTreeMap::from([("custom".into(), tlk_hash)]),
            module_url: "/files/v1/".into(),
            haks_url: format!("{}/assets/", server.url),
        };
        let cache = temp.path().join("cache");
        Self { _temp: temp, server, spec, cache }
    }
    fn client(&self, limits: Limits) -> ResourceClient {
        ResourceClient::new(
            Gateway::new(&self.server.url, Some(&self.server.pin)).unwrap(),
            SessionToken::new("fixture-session".into()).unwrap(),
            self.cache.clone(),
            limits,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Windows requires removing the read-only flag from this fixture's own module files.
        #[cfg(windows)]
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "Windows-only cleanup of fixture-owned files"
        )]
        fn writable(p: &std::path::Path) {
            if let Ok(entries) = std::fs::read_dir(p) {
                for e in entries {
                    let p = e.unwrap().path();
                    if p.is_dir() {
                        writable(&p);
                    } else {
                        let mut perms = std::fs::metadata(&p).unwrap().permissions();
                        perms.set_readonly(false);
                        std::fs::set_permissions(p, perms).unwrap();
                    }
                }
            }
        }
        #[cfg(windows)]
        writable(&self.cache);
    }
}
#[test]
fn https_full_parallel_resume_cancel_and_atomic_generation_switch() {
    let f = Fixture::new();
    let client = f.client(Limits::default());
    let slot = Generations::default();
    slot.select(&f.spec);
    let cancel = Cancel::default();
    let error = client
        .download(&f.spec, &cancel, &|p| {
            if p.files >= 5 {
                cancel.cancel();
            }
        })
        .unwrap_err();
    assert!(error.contains("cancelled"), "{error}: {:?}", f.server.seen.lock().unwrap());
    assert!(slot.current().is_none());
    assert!(!f.cache.join("prepared.json").exists());
    let cached = std::fs::read_dir(f.cache.join("cache/blobs")).unwrap().count();
    assert!(cached > 0);
    f.server.seen.lock().unwrap().clear();
    let ready = client.download(&f.spec, &Cancel::default(), &|_| {}).unwrap();
    assert!(slot.current().is_none());
    assert!(slot.activate(ready));
    assert!(f.server.peak.load(Ordering::SeqCst) > 1 && f.server.peak.load(Ordering::SeqCst) <= 4);
    let transfers = f.server.seen.lock().unwrap().iter().filter(|p| p.contains("/data/")).count();
    assert_eq!(transfers, 10 - cached);
    assert_eq!(slot.current().unwrap().generation(), &f.spec);
    let mut next = f.spec.clone();
    next.boot = "boot-2".into();
    slot.select(&next);
    let cancelled = Cancel::default();
    cancelled.cancel();
    assert!(client.download(&next, &cancelled, &|_| {}).is_err());
    assert_eq!(slot.current().unwrap().generation().boot, "boot-1");
    let stale = client.download(&f.spec, &Cancel::default(), &|_| {}).unwrap();
    assert!(!slot.activate(stale));
    assert!(slot.activate(client.download(&next, &Cancel::default(), &|_| {}).unwrap()));
    assert_eq!(slot.current().unwrap().generation().boot, "boot-2");
    slot.clear();
    assert!(slot.current().is_none());
    assert!(!slot.activate(client.download(&next, &Cancel::default(), &|_| {}).unwrap()));
}
#[test]
fn https_tampered_and_oversized_blobs_never_publish_generation() {
    for oversized in [false, true] {
        let f = Fixture::new();
        let hash = hex(&sha1(&vec![0; 4096]));
        let path = format!("/assets/data/sha1/{}/{}/{hash}", &hash[..2], &hash[2..4]);
        let bytes = if oversized {
            vec![0; 2 * 1024 * 1024]
        } else {
            mg_erf::compressedbuf::compress(b"tampered", mg_erf::compressedbuf::MAGIC_NSYC)
        };
        f.server.rows.lock().unwrap().insert(path, bytes);
        let limits = Limits { resource_bytes: 8192, ..Limits::default() };
        let error = f.client(limits).download(&f.spec, &Cancel::default(), &|_| {}).unwrap_err();
        assert!(error.contains(if oversized { "byte limit" } else { "SHA-1/size" }), "{error}");
        assert!(!f.cache.join("cache/blobs").join(hash).exists());
        assert!(!f.cache.join("prepared.json").exists());
    }
}
#[test]
fn resource_urls_cannot_exfiltrate_tokens_or_downgrade_tls() {
    let f = Fixture::new();
    let client = f.client(Limits::default());
    for url in [
        "http://assets.example.invalid/",
        "https://user:secret@assets.example.invalid/",
        "https://assets.example.invalid/?q=x",
    ] {
        let mut spec = f.spec.clone();
        spec.haks_url = url.into();
        assert!(client.download(&spec, &Cancel::default(), &|_| {}).is_err());
    }
    let mut spec = f.spec.clone();
    spec.module_url = "https://other.example.invalid/files/v1/".into();
    assert!(client.download(&spec, &Cancel::default(), &|_| {}).is_err());
    assert!(f.server.seen.lock().unwrap().is_empty());
}

#[test]
fn tlk_mismatch_and_cached_tampering_cannot_activate() {
    let f = Fixture::new();
    let client = f.client(Limits::default());
    let mut bad = f.spec.clone();
    bad.tlk.insert("custom".into(), "a".repeat(40));
    let error = client.download(&bad, &Cancel::default(), &|_| {}).unwrap_err();
    assert!(error.contains("TLK mismatch"), "{error}");
    assert!(!f.cache.join("prepared.json").exists());
    let hash = hex(&sha1(&vec![0; 4096]));
    std::fs::write(f.cache.join("cache/blobs").join(hash), vec![1; 4096]).unwrap();
    let error = client.download(&f.spec, &Cancel::default(), &|_| {}).unwrap_err();
    assert!(error.contains("SHA-1 mismatch"), "{error}");
    assert!(!f.cache.join("prepared.json").exists());
}
