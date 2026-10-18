//! The production gateway router, managed AI registry and MCP process over pinned TLS.
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use mg_gateway::{Authenticator, CoreHandle, Principal};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
use tower::ServiceExt;
#[derive(Debug)]
struct Auth;
impl Authenticator for Auth {
    fn authenticate(&self, _: &str) -> Option<Principal> {
        None
    }
    fn principal(&self, id: &str) -> Option<Principal> {
        (id == "dm:fixture").then(|| Principal {
            id: id.into(),
            name: "Fixture".into(),
            groups: vec!["read".into()],
        })
    }
}
struct Fixture {
    url: String,
    pin: String,
    token: String,
    core: CoreHandle,
    calls: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
    scratch: std::path::PathBuf,
}
impl Fixture {
    async fn new() -> Self {
        let core = CoreHandle::new(Arc::new(mg_live::store::LiveStore::default())).await.unwrap();
        let mut random = [0; 32];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random).unwrap();
        let token = format!("mgai_{}", hex(&random));
        let credential = token.clone();
        core.with_gateway(move |g| {
            g.install_ai(
                mg_live::gateway::AiIdentity {
                    id: "ai:fixture".into(),
                    owner: "dm:fixture".into(),
                    name: "Fixture".into(),
                    groups: vec!["read".into()],
                    expires_ms: u64::MAX,
                    llm_allowed: true,
                },
                &credential,
            )
            .unwrap();
            g.set_llm_enabled(true);
        })
        .await
        .unwrap();
        core.with_store(|s| { s.sync(br#"{"v":1,"boot":"fixture","seq":1,"areas":[{"area":"a","full":true,"cre":[{"id":"1","name":"Example"}]}]}"#, 1000).unwrap(); }).await.unwrap();
        let router = mg_gateway::router(
            core.clone(),
            Arc::new(Auth),
            Arc::new(mg_live::store::Policy::default()),
            mg_gateway::Config::default(),
        );
        let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let pin = hex(ring::digest::digest(&ring::digest::SHA256, generated.cert.der()).as_ref());
        let tls = Arc::new(
            rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![generated.cert.der().clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der())
                    .into(),
            )
            .unwrap(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let runtime = tokio::runtime::Handle::current();
        let server = thread::spawn(move || {
            let mut workers = Vec::new();
            while !stopping.load(Ordering::Relaxed) {
                let Ok((socket, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                socket.set_nonblocking(false).unwrap();
                socket.set_nodelay(true).unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                socket.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
                let conn = rustls::ServerConnection::new(tls.clone()).unwrap();
                let app = router.clone();
                let runtime = runtime.clone();
                let counted = counted.clone();
                workers.push(thread::spawn(move || {
                    let mut stream = BufReader::new(rustls::StreamOwned::new(conn, socket));
                    let mut first = String::new();
                    if stream.read_line(&mut first).is_err() || first.is_empty() { return }
                    let parts: Vec<_> = first.split_whitespace().collect();
                    let mut request = Request::builder().method(parts[0]).uri(parts[1]);
                    assert!(parts[1].starts_with("/agent/v1/"));
                    let mut len = 0;
                    loop {
                        let mut line = String::new(); stream.read_line(&mut line).unwrap();
                        if line == "\r\n" { break }
                        let (k, v) = line.trim().split_once(':').unwrap();
                        if k.eq_ignore_ascii_case("content-length") { len = v.trim().parse::<usize>().unwrap(); }
                        request = request.header(k, v.trim());
                    }
                    assert!(len < 65536);
                    let mut body = vec![0;len]; stream.read_exact(&mut body).unwrap();
                    counted.fetch_add(1, Ordering::Relaxed);
                    let (status, etag, bytes) = runtime.block_on(async move {
                        let response = app.oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
                        let status = response.status().as_u16();
                        let etag = response.headers().get("etag").map(|v|format!("ETag: {}\r\n",v.to_str().unwrap())).unwrap_or_default();
                        let bytes = to_bytes(response.into_body(), 16*1024*1024).await.unwrap();
                        (status, etag, bytes)
                    });
                    write!(stream.get_mut(), "HTTP/1.1 {status} Reply\r\nContent-Length: {}\r\nConnection: close\r\n{etag}\r\n", bytes.len()).unwrap();
                    stream.get_mut().write_all(&bytes).unwrap(); stream.get_mut().flush().unwrap();
                    stream.get_mut().conn.send_close_notify();
                    let _ = stream.get_mut().flush();
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        let scratch = std::env::temp_dir().join(format!("mcp-gateway-{}", hex(&random[..8])));
        std::fs::create_dir(&scratch).unwrap();
        Self { url, pin, token, core, calls, stop, server: Some(server), scratch }
    }
    fn command(&self, pin: Option<&str>) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_moonglow-mcp"));
        c.args([
            "--gateway",
            &self.url,
            "--session-id",
            "table-one",
            "--mg",
            "missing-cli-for-fixture",
        ])
        .current_dir(&self.scratch)
        .env("MOONGLOW_AI_TOKEN", &self.token)
        .env("APPDATA", &self.scratch)
        .env("XDG_DATA_HOME", &self.scratch)
        .env("HOME", &self.scratch)
        .env_remove("MOONGLOW_AGENT_DESCRIPTOR")
        .env_remove("MOONGLOW_MCP_SESSIONS")
        .env_remove("MOONGLOW_MCP_ROOTS");
        if let Some(pin) = pin {
            c.args(["--gateway-pin", pin]);
        }
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.server.take().unwrap().join().unwrap();
        std::fs::remove_dir_all(&self.scratch).unwrap();
    }
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_catalogue_calls_string_session_and_consent_use_real_agent_route() {
    let f = Fixture::new().await;
    // Exercise MCP, not a second copy of the operation table.
    let mut child = f
        .command(Some(&f.pin))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut call = |method: &str, params: Value| {
        writeln!(input, "{}", json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert!(!line.contains(&f.token));
        serde_json::from_str::<Value>(&line).unwrap()
    };
    let tools = call("tools/list", json!({}));
    let names: Vec<_> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["name"].as_str())
        .collect();
    let sessions = call("tools/call", json!({"name":"moonglow_sessions","arguments":{}}));
    assert!(names.contains(&"live_find"), "discovery: {sessions}");
    assert!(!names.contains(&"live_run_script"));
    assert_eq!(sessions["result"]["structuredContent"]["sessions"][0]["id"], "table-one");
    assert!(sessions["result"]["structuredContent"]["sessions"][0].get("pid").is_none());
    let found = call("tools/call", json!({"name":"live_find","arguments":{"text":"Example"}}));
    assert_ne!(found["result"]["isError"], true, "{found}");
    assert!(found.to_string().contains("Example"));
    f.core.with_gateway(|g| g.set_llm_enabled(false)).await.unwrap();
    let refused = call("tools/call", json!({"name":"live_find","arguments":{"text":"Example"}}));
    assert_eq!(refused["result"]["isError"], true);
    drop(input);
    let result = child.wait_with_output().unwrap();
    assert!(!String::from_utf8_lossy(&result.stderr).contains(&f.token));
    f.core.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_ip_requires_pin_and_wrong_pin_never_sends_bearer() {
    let f = Fixture::new().await;
    for pin in [None, Some("0000000000000000000000000000000000000000000000000000000000000000")] {
        let output = f.command(pin).args(["call", "live_find", "{}"]).output().unwrap();
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains(&f.token));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(&f.token));
    }
    let dns = f.url.replace("127.0.0.1", "localhost");
    let output =
        f.command(None).args(["--gateway", &dns, "call", "live_find", "{}"]).output().unwrap();
    assert!(!output.status.success(), "DNS gateway must use WebPKI, not trust a self-signed leaf");
    assert_eq!(f.calls.load(Ordering::Relaxed), 0);
    let output =
        f.command(Some(&f.pin)).env_remove("MOONGLOW_AI_TOKEN").arg("tools").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MOONGLOW_AI_TOKEN"));
    f.core.shutdown().await.unwrap();
}
