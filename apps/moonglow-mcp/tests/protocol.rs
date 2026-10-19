use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Opt-in reproducible process latency assay; no running game or real credentials.
#[test]
fn shell_call_latency() {
    let Ok(output) = std::env::var("MGLIVE_MCP_MEASURE") else { return };
    let scratch = Scratch::new();
    let agent = Agent::new();
    {
        let mut describe = agent.describe.lock().unwrap();
        for n in 2..82 {
            let mut op = describe["operations"][0].clone();
            op["name"] = json!(format!("invented_{n}"));
            describe["operations"].as_array_mut().unwrap().push(op);
        }
    }
    let gateway = agent.args(&agent.pin);
    let mg = std::env::var_os("MGLIVE_MCP_MEASURE_MG")
        .map(PathBuf::from)
        .unwrap_or_else(|| scratch.0.join("no-cli"));
    let mut samples = Vec::new();
    for _ in 0..11 {
        let start = std::time::Instant::now();
        let result = Command::new(env!("CARGO_BIN_EXE_moonglow-mcp"))
            .args(&gateway)
            .args(["--mg", mg.to_str().unwrap(), "call", "invented_read", "{}"])
            .current_dir(&scratch.0)
            .env("APPDATA", &scratch.0)
            .env("XDG_DATA_HOME", &scratch.0)
            .env("HOME", &scratch.0)
            .env("MOONGLOW_AI_TOKEN", "test-secret")
            .env_remove("MOONGLOW_MCP_ROOTS")
            .env_remove("MOONGLOW_MCP_HTTP_TOKEN")
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let mut warm = samples[1..].to_vec();
    warm.sort_by(f64::total_cmp);
    let hash = |p: &Path| {
        ring::digest::digest(&ring::digest::SHA256, &fs::read(p).unwrap())
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let receipt = json!({"fixture":"82 operations; gateway discovery; authenticated TLS; one shell process/call",
        "cold_ms":samples[0],"warm_median_ms":(warm[4]+warm[5])/2.,"warm_max_ms":warm[9],"samples_ms":samples,
        "adapter_sha256":hash(Path::new(env!("CARGO_BIN_EXE_moonglow-mcp"))),
        "cli_sha256":mg.is_file().then(||hash(&mg)),"describe_bytes":agent.describe.lock().unwrap().to_string().len()});
    fs::write(output, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    println!("{receipt}");
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "agent-protocol-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.0) {
            if std::thread::panicking() {
                eprintln!("Test scratch retained at {}: {e}", self.0.display());
            } else {
                panic!("Test scratch cleanup failed at {}: {e}", self.0.display());
            }
        }
    }
}
struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<std::process::ChildStdout>,
}
impl Client {
    fn new<S: AsRef<str>>(
        scratch: &Path,
        mg: &Path,
        description: Option<&Path>,
        args: &[S],
    ) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_moonglow-mcp"));
        command
            .args(["--mg", mg.to_str().unwrap(), "--roots", scratch.to_str().unwrap()])
            .args(args.iter().map(AsRef::as_ref))
            .env("APPDATA", scratch)
            .env("XDG_DATA_HOME", scratch)
            .env("HOME", scratch)
            .env("MOONGLOW_AI_TOKEN", "test-secret")
            .env_remove("MOONGLOW_MCP_WRITE")
            .env_remove("MOONGLOW_MCP_ROOTS")
            .env_remove("FAKE_CLI_DESCRIPTION")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(description) = description {
            command.env("FAKE_CLI_DESCRIPTION", description);
        }
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self { child, input: Some(input), output }
    }
    fn raw(&mut self, text: &str) -> Value {
        writeln!(self.input.as_mut().unwrap(), "{text}").unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
        self.read()
    }
    fn read(&mut self) -> Value {
        let mut text = String::new();
        assert!(self.output.read_line(&mut text).unwrap() > 0, "server exited");
        serde_json::from_str(&text).unwrap()
    }
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.raw(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string())
    }
    fn tool(&mut self, name: &str, args: Value) -> Value {
        self.call("tools/call", json!({"name":name,"arguments":args}))["result"].clone()
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.input.take();
        for _ in 0..100 {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Agent {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    url: String,
    pin: String,
    describe: Arc<Mutex<Value>>,
    calls: Arc<Mutex<Vec<Value>>>,
    pings: Arc<AtomicU64>,
    reject_ping: Arc<AtomicBool>,
    describes: Arc<AtomicU64>,
    connections: Arc<AtomicU64>,
    events: Arc<(Mutex<Vec<Value>>, std::sync::Condvar)>,
    drop_call: Arc<AtomicBool>,
    transport: Arc<AtomicU64>,
}
impl Agent {
    fn new() -> Self {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let der = cert.cert.der().clone();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
        let pin = ring::digest::digest(&ring::digest::SHA256, &der)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![der], key.into())
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let describe = Arc::new(Mutex::new(
            json!({"protocol":"moonglow-agent/1","moonglow":{"name":"Invented application","version":"99","variant":"test"},"operations":[
                {"name":"invented_read","description":"An invented read operation","readOnly":true,"destructive":false,"input":{"type":"object","properties":{},"additionalProperties":false}},
                {"name":"invented_put","description":"An invented write operation","readOnly":false,"destructive":true,"input":{"type":"object","required":["message"],"properties":{"message":{"type":"string","description":"Message to echo"}},"additionalProperties":false}}
            ]}),
        ));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let pings = Arc::new(AtomicU64::new(0));
        let reject_ping = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let describes = Arc::new(AtomicU64::new(0));
        let connections = Arc::new(AtomicU64::new(0));
        let events = Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new()));
        let drop_call = Arc::new(AtomicBool::new(false));
        let transport = Arc::new(AtomicU64::new(0));
        let shared = Arc::new(FakeState {
            describe: describe.clone(),
            calls: calls.clone(),
            pings: pings.clone(),
            reject: reject_ping.clone(),
            stop: stop.clone(),
            describes: describes.clone(),
            events: events.clone(),
            drop_call: drop_call.clone(),
            transport: transport.clone(),
        });
        let accepted = connections.clone();
        let config = Arc::new(config);
        let worker = thread::spawn(move || {
            let mut workers = Vec::new();
            while !shared.stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        accepted.fetch_add(1, Ordering::Relaxed);
                        let (state, config) = (shared.clone(), config.clone());
                        workers.push(thread::spawn(move || serve_agent(socket, config, state)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1))
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            stop,
            thread: Some(worker),
            url,
            pin,
            describe,
            calls,
            pings,
            reject_ping,
            describes,
            connections,
            events,
            drop_call,
            transport,
        }
    }
    /// The adapter's gateway arguments; its token is the environment's `MOONGLOW_AI_TOKEN`.
    fn args(&self, pin: &str) -> Vec<String> {
        vec!["--gateway".into(), self.url.clone(), "--gateway-pin".into(), pin.into()]
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}
struct FakeState {
    describe: Arc<Mutex<Value>>,
    calls: Arc<Mutex<Vec<Value>>>,
    pings: Arc<AtomicU64>,
    reject: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    describes: Arc<AtomicU64>,
    events: Arc<(Mutex<Vec<Value>>, std::sync::Condvar)>,
    drop_call: Arc<AtomicBool>,
    transport: Arc<AtomicU64>,
}
fn serve_agent(socket: TcpStream, config: Arc<rustls::ServerConfig>, state: Arc<FakeState>) {
    socket.set_nonblocking(false).unwrap();
    socket.set_nodelay(true).unwrap();
    socket.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
    let mut tls = rustls::StreamOwned::new(rustls::ServerConnection::new(config).unwrap(), socket);
    while !state.stop.load(Ordering::Relaxed) {
        let mut header = Vec::new();
        let mut byte = [0];
        while !header.ends_with(b"\r\n\r\n")
            && header.len() < 65536
            && !state.stop.load(Ordering::Relaxed)
        {
            match tls.read_exact(&mut byte) {
                Ok(()) => header.push(byte[0]),
                Err(e)
                    if header.is_empty()
                        && matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                {
                    continue;
                }
                Err(_) => return,
            }
        }
        if !header.ends_with(b"\r\n\r\n") {
            return;
        }
        let header = String::from_utf8(header).unwrap();
        assert!(header.contains("Authorization: Bearer test-secret"));
        let field = |name: &str| {
            header
                .lines()
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.trim())
        };
        let n = field("Content-Length").unwrap().parse::<usize>().unwrap();
        let mut bytes = vec![0; n];
        if tls.read_exact(&mut bytes).is_err() {
            return;
        }
        let path = header.split_whitespace().nth(1).unwrap();
        let path = path.strip_prefix("/agent").expect("the gateway serves /agent/v1");
        let describe = state.describe.lock().unwrap().clone();
        let etag = format!(
            "\"{}\"",
            ring::digest::digest(&ring::digest::SHA256, describe.to_string().as_bytes())
                .as_ref()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let mut status = "200 OK";
        let value = if path == "/v1/describe" {
            state.describes.fetch_add(1, Ordering::Relaxed);
            describe
        } else if path == "/v1/ping" {
            state.pings.fetch_add(1, Ordering::Relaxed);
            if state.reject.load(Ordering::Relaxed) {
                status = "403 Forbidden";
            }
            json!({"ok":true})
        } else if state.transport.load(Ordering::Relaxed) == 3 {
            status = "403 Forbidden";
            json!({"error":"consent revoked"})
        } else if state
            .transport
            .compare_exchange(2, 0, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            status = "404 Not Found";
            json!({"error":"refresh operation"})
        } else if field("If-Match").is_some_and(|e| e != etag) {
            status = "412 Precondition Failed";
            json!({"error":"description changed"})
        } else {
            let op = path.strip_prefix("/v1/call/").unwrap_or_default();
            if !describe["operations"].as_array().unwrap().iter().any(|v| v["name"] == op) {
                status = "404 Not Found";
                json!({"error":"unknown operation"})
            } else {
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                state.calls.lock().unwrap().push(body.clone());
                if state.drop_call.swap(false, Ordering::Relaxed) {
                    return;
                }
                if op == "invented_events" {
                    let since = body["since"].as_u64().unwrap_or(0);
                    let (lock, signal) = &*state.events;
                    let mut events = lock.lock().unwrap();
                    if events.last().is_none_or(|e| e["n"].as_u64().unwrap() <= since)
                        && body["wait_ms"].as_u64().unwrap_or(0) > 0
                    {
                        events = signal.wait_timeout(events, Duration::from_millis(200)).unwrap().0;
                    }
                    let rows: Vec<_> = events
                        .iter()
                        .filter(|e| {
                            e["n"].as_u64().unwrap() > since
                                && (body["from"] != "players" || e["data"]["pc"] == true)
                                && body["kinds"].as_array().is_none_or(|k| k.contains(&e["kind"]))
                                && body.get("area").is_none_or(|a| a == &e["area"])
                                && (body["to_dm"] != true || e["data"]["to_dm"] == true)
                        })
                        .cloned()
                        .collect();
                    json!({"ok":true,"result":{"events":rows,"last":events.last().map_or(0,|e|e["n"].as_u64().unwrap()),"boot":"fixture","lost":false,"reset":false}})
                } else {
                    json!({"ok":true,"result":{"echo":body}})
                }
            }
        };
        let body = value.to_string();
        let modern = state.transport.load(Ordering::Relaxed) != 1;
        let keep =
            modern && field("Connection").is_some_and(|v| v.eq_ignore_ascii_case("keep-alive"));
        let revision = if modern { format!("ETag: {etag}\r\n") } else { String::new() };
        let connection = if keep { "keep-alive" } else { "close" };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{revision}Connection: {connection}\r\n\r\n{body}",
            body.len()
        );
        if tls.write_all(response.as_bytes()).is_err() || tls.flush().is_err() {
            return;
        }
        if !keep {
            return;
        }
    }
}
fn description() -> Value {
    json!({"protocol":"moonglow-cli/1","version":"imaginary","commands":[{"name":"invented","path":["invented"],"description":"Invented CLI command","writes":true,"longRunning":false,"input":{"type":"object","additionalProperties":false,"required":["file"],"properties":{"file":{"type":"string","description":"Path"},"delay":{"type":"boolean","description":"Wait before returning"}}},"argv":[{"input":"file","flag":null,"position":1,"action":"value","path":true,"last":false},{"input":"delay","flag":"--delay","position":null,"action":"set_true","path":false}]}]})
}
fn tools(value: &Value) -> Vec<String> {
    value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_owned())
        .collect()
}

fn shell(scratch: &Path, agent: &Agent) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_moonglow-mcp"));
    cmd.args(agent.args(&agent.pin))
        .args(["--mg", "missing-cli"])
        .current_dir(scratch)
        .env("APPDATA", scratch)
        .env("HOME", scratch)
        .env("XDG_DATA_HOME", scratch)
        .env("MOONGLOW_AI_TOKEN", "test-secret")
        .env_remove("MOONGLOW_MCP_ROOTS")
        .env_remove("MOONGLOW_MCP_WRITE");
    cmd
}

#[test]
fn shell_cache_validates_revision_credential_and_input_without_repeating_actions() {
    let scratch = Scratch::new();
    let agent = Agent::new();
    let call = |input: Value| {
        shell(&scratch.0, &agent)
            .args(["call", "invented_put", &input.to_string()])
            .output()
            .unwrap()
    };
    for text in ["cold", "warm"] {
        let result = call(json!({"message":text}));
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    }
    assert_eq!(
        agent.describes.load(Ordering::Relaxed),
        1,
        "warm shell call must not fetch describe"
    );
    assert_eq!(agent.connections.load(Ordering::Relaxed), 2, "one TLS socket per shell process");
    assert!(!call(json!({"message":42})).status.success());
    assert_eq!(agent.calls.lock().unwrap().len(), 2, "bad input is rejected before any command");
    assert_eq!(agent.describes.load(Ordering::Relaxed), 2, "local schema mismatch refreshes once");
    // Another credential is another cache identity: the description is fetched again.
    let other = shell(&scratch.0, &agent)
        .env("MOONGLOW_AI_TOKEN", "test-secret-2")
        .args(["call", "invented_put", &json!({"message":"new credential"}).to_string()])
        .output()
        .unwrap();
    assert!(other.status.success());
    assert_eq!(agent.describes.load(Ordering::Relaxed), 3);
    agent.describe.lock().unwrap()["operations"][1]["input"]["properties"]["count"] =
        json!({"type":"integer","description":"Optional count"});
    let changed = call(json!({"message":"revision changed"}));
    assert!(changed.status.success(), "{}", String::from_utf8_lossy(&changed.stderr));
    assert_eq!(
        agent.describes.load(Ordering::Relaxed),
        4,
        "conditional POST rejects before dispatch then refreshes"
    );
    assert_eq!(agent.calls.lock().unwrap().len(), 4, "412 never executes the action");
    agent.drop_call.store(true, Ordering::Relaxed);
    let failed = call(json!({"message":"one uncertain delivery"}));
    assert!(!failed.status.success());
    assert_eq!(agent.calls.lock().unwrap().len(), 5, "lost response is never resent");
    agent.describe.lock().unwrap()["operations"].as_array_mut().unwrap().pop();
    assert!(!call(json!({"message":"removed"})).status.success());
    assert_eq!(agent.calls.lock().unwrap().len(), 5);
    fn inspect(dir: &Path) {
        for file in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
            if file.path().is_dir() {
                inspect(&file.path());
            } else if file.path().parent().is_some_and(|p| p.ends_with("agent-cache")) {
                assert!(!fs::read_to_string(file.path()).unwrap().contains("test-secret"));
                fs::write(file.path(), b"broken cache").unwrap();
            }
        }
    }
    inspect(&scratch.0);
    let result = shell(&scratch.0, &agent).args(["call", "invented_read", "{}"]).output().unwrap();
    assert!(result.status.success(), "corrupt cache must be disposable");
}

#[test]
fn shell_handles_404_and_revocation_and_older_uncached_agents() {
    let scratch = Scratch::new();
    let agent = Agent::new();
    let call = |token: &str| {
        shell(&scratch.0, &agent)
            .env("MOONGLOW_AI_TOKEN", token)
            .args(["call", "invented_read", "{}"])
            .output()
            .unwrap()
    };
    assert!(call("test-secret").status.success());
    agent.transport.store(2, Ordering::Relaxed);
    assert!(call("test-secret").status.success());
    assert_eq!(agent.describes.load(Ordering::Relaxed), 2);
    assert_eq!(
        agent.calls.lock().unwrap().len(),
        2,
        "404 is pre-dispatch, then exactly one action"
    );
    agent.transport.store(3, Ordering::Relaxed);
    assert!(!call("test-secret").status.success());
    assert_eq!(agent.describes.load(Ordering::Relaxed), 2, "403 never triggers a refresh or retry");
    assert_eq!(agent.calls.lock().unwrap().len(), 2);
    // A different credential and an older server: no ETag and no keep-alive opt-in.
    agent.transport.store(1, Ordering::Relaxed);
    assert!(call("test-secret-2").status.success());
    assert!(call("test-secret-2").status.success());
    assert_eq!(agent.describes.load(Ordering::Relaxed), 4, "legacy describes are not cached");
}

#[test]
fn stdio_calls_reuse_description_and_connection_until_tools_list() {
    let scratch = Scratch::new();
    let agent = Agent::new();
    let mut client =
        Client::new(&scratch.0, &scratch.0.join("missing-cli"), None, &agent.args(&agent.pin));
    client.call("initialize", json!({"protocolVersion":"2025-06-18"}));
    for _ in 0..4 {
        assert_eq!(client.tool("invented_read", json!({}))["isError"], false);
    }
    assert_eq!(agent.describes.load(Ordering::Relaxed), 1);
    assert_eq!(agent.connections.load(Ordering::Relaxed), 1);
    client.call("tools/list", json!({}));
    assert_eq!(agent.describes.load(Ordering::Relaxed), 2);
    assert_eq!(agent.connections.load(Ordering::Relaxed), 2);
}

fn event_stream(agent: &Agent) {
    let mut describe = agent.describe.lock().unwrap();
    describe["capabilities"] = json!({"eventStream":{"operation":"invented_events"}});
    describe["operations"].as_array_mut().unwrap().push(json!({
        "name":"invented_events","description":"Invented event stream","readOnly":true,"destructive":false,
        "input":{"type":"object","additionalProperties":false,"properties":{
            "since":{"type":"integer","description":"Cursor"},
            "wait_ms":{"type":"integer","maximum":30000,"description":"Wait"},
            "limit":{"type":"integer","description":"Limit"},
            "boot":{"type":"string","description":"Boot identity"},
            "from":{"type":"string","enum":["players"],"description":"Source"},
            "to_dm":{"type":"boolean","description":"DM chat"},
            "area":{"type":"string","description":"Area"},
            "kinds":{"type":"array","items":{"type":"string"},"description":"Kinds"}
        }}
    }));
}
struct Watching {
    child: Child,
    lines: std::sync::mpsc::Receiver<String>,
}
impl Watching {
    fn start(scratch: &Path, agent: &Agent, args: &[&str]) -> Self {
        let mut child = shell(scratch, agent)
            .arg("watch")
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let out = child.stdout.take().unwrap();
        let (send, lines) = std::sync::mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                if send.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }
    fn read(&self) -> Value {
        serde_json::from_str(
            &self.lines.recv_timeout(Duration::from_secs(5)).expect("watch timed out"),
        )
        .unwrap()
    }
}
impl Drop for Watching {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
fn watch_tails_new_matching_events_over_one_connection_and_can_replay() {
    let scratch = Scratch::new();
    let agent = Agent::new();
    event_stream(&agent);
    let event =
        |n, pc, kind, area| json!({"n":n,"kind":kind,"area":area,"data":{"pc":pc,"to_dm":true}});
    agent.events.0.lock().unwrap().push(event(1, true, "chat", "room"));
    let mut watch = Watching::start(
        &scratch.0,
        &agent,
        &["--from", "players", "--kinds", "chat", "--area", "room", "--to-dm", "--count", "2"],
    );
    let ready = std::time::Instant::now();
    while agent.calls.lock().unwrap().len() < 2 {
        assert!(ready.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(5));
    }
    assert!(watch.lines.try_recv().is_err(), "default watch skips historical event");
    let start = std::time::Instant::now();
    {
        let mut events = agent.events.0.lock().unwrap();
        events.extend([
            event(2, false, "chat", "room"),
            event(3, true, "damage", "room"),
            event(4, true, "chat", "elsewhere"),
            event(5, true, "chat", "room"),
        ]);
        agent.events.1.notify_all();
    }
    assert_eq!(watch.read()["n"], 5);
    let wake_ms = start.elapsed().as_secs_f64() * 1000.;
    agent.events.0.lock().unwrap().push(event(6, true, "chat", "room"));
    agent.events.1.notify_all();
    assert_eq!(watch.read()["n"], 6);
    assert!(watch.child.wait().unwrap().success());
    assert_eq!(agent.describes.load(Ordering::Relaxed), 1);
    assert_eq!(agent.connections.load(Ordering::Relaxed), 1);
    if let Ok(output) = std::env::var("MGLIVE_MCP_WATCH_MEASURE") {
        fs::write(
            output,
            json!({"notify_to_json_line_ms":wake_ms,"describes":1,"connections":1}).to_string(),
        )
        .unwrap();
    }
    let mut replay =
        Watching::start(&scratch.0, &agent, &["--since", "4", "--kinds", "chat", "--count", "2"]);
    assert_eq!(replay.read()["n"], 5);
    assert_eq!(replay.read()["n"], 6);
    assert!(replay.child.wait().unwrap().success());
    assert_eq!(
        agent.describes.load(Ordering::Relaxed),
        1,
        "watch also consumes the description cache"
    );
    assert_eq!(agent.connections.load(Ordering::Relaxed), 2);
    let bad = shell(&scratch.0, &agent).args(["watch", "--from", "robots"]).output().unwrap();
    assert!(!bad.status.success());
}

#[test]
fn pipes_discover_unknown_operations_cli_sessions_guards_and_bad_input() {
    let scratch = Scratch::new();
    let mg = scratch.0.join(if cfg!(windows) { "invented.exe" } else { "invented" });
    assert!(
        Command::new("rustup")
            .args(["run", "1.98.0", "rustc", "--edition=2024"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cli.rs"))
            .arg("-o")
            .arg(&mg)
            .status()
            .unwrap()
            .success()
    );
    let cli = scratch.0.join("cli.json");
    fs::write(&cli, description().to_string()).unwrap();
    let agent = Agent::new();
    let gateway = agent.args(&agent.pin);
    let mut client = Client::new(&scratch.0, &mg, Some(&cli), &gateway);
    for version in ["2024-11-05", "2025-03-26", "2025-06-18", "future"] {
        let value=client.call("initialize",json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"pipe-test","version":"1"}}));
        assert_eq!(
            value["result"]["protocolVersion"],
            if version == "future" { "2025-06-18" } else { version }
        );
    }
    writeln!(
        client.input.as_mut().unwrap(),
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    client.input.as_mut().unwrap().flush().unwrap();
    let listing = client.call("tools/list", json!({}));
    let names = tools(&listing);
    assert!(
        names.contains(&"invented_read".into())
            && names.contains(&"invented_put".into())
            && names.contains(&"mg_invented".into()),
        "{listing}; {}",
        client.tool("moonglow_sessions", json!({}))
    );
    assert_eq!(
        client.tool("invented_put", json!({"message":"hello"}))["structuredContent"]["echo"]["message"],
        "hello"
    );
    assert_eq!(agent.calls.lock().unwrap().len(), 1);
    assert_eq!(client.tool("invented_put", json!({"unknown":true}))["isError"], true);
    assert!(!client.tool("moonglow_sessions", json!({})).to_string().contains("test-secret"));
    assert_eq!(client.tool("mg_invented", json!({"file":scratch.0}))["isError"], true);
    agent.describe.lock().unwrap()["operations"].as_array_mut().unwrap().pop();
    assert!(!tools(&client.call("tools/list", json!({}))).contains(&"invented_put".into()));
    assert_eq!(client.read()["method"], "notifications/tools/list_changed");
    let line = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    for i in 0..line.len() {
        assert_eq!(client.raw(&line[..i])["error"]["code"], -32700);
    }
    for bad in ["garbage", "{", "\"x\"", "[]", "null"] {
        let response = client.raw(bad);
        assert!(response["error"]["code"] == -32700 || response["error"]["code"] == -32600);
    }
    assert_eq!(client.call("missing", json!({}))["error"]["code"], -32601);
    assert_eq!(client.call("tools/call", json!({}))["error"]["code"], -32602);
    drop(client);
    let mut writer = Client::new(&scratch.0, &mg, Some(&cli), &["--allow-write"]);
    writer.call("initialize", json!({"protocolVersion":"2025-06-18"}));
    assert_eq!(
        writer.tool("mg_invented", json!({"file":scratch.0.join("new.txt")}))["structuredContent"]
            ["stdout"]["invented"],
        true
    );
    assert_eq!(
        writer.tool("mg_invented", json!({"file":scratch.0.join("../outside.txt")}))["isError"],
        true
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(scratch.0.parent().unwrap(), scratch.0.join("escape")).unwrap();
        assert_eq!(
            writer.tool("mg_invented", json!({"file":scratch.0.join("escape/outside.txt")}))["isError"],
            true
        );
    }
    #[cfg(windows)]
    {
        let junction = scratch.0.join("escape");
        assert!(
            Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(&junction)
                .arg(scratch.0.parent().unwrap())
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            writer.tool("mg_invented", json!({"file":junction.join("outside.txt")}))["isError"],
            true
        );
        fs::remove_dir(junction).unwrap();
    }
    let timeout =
        writer.tool("mg_invented", json!({"file":scratch.0,"delay":true,"timeout_ms":100}));
    assert_eq!(timeout["structuredContent"]["timedOut"], true);
    let job = writer.tool("mg_invented", json!({"file":scratch.0,"delay":true,"background":true}))
        ["structuredContent"]["job"]
        .clone();
    assert_eq!(
        writer.tool("moonglow_jobs", json!({"action":"stop","job":job}))["structuredContent"]["stopped"],
        true
    );
    assert_eq!(
        writer.tool("moonglow_jobs", json!({"action":"stop","job":"foreign"}))["isError"],
        true
    );
    let mut long = description();
    long["commands"][0]["longRunning"] = json!(true);
    fs::write(&cli, long.to_string()).unwrap();
    writer.call("tools/list", json!({}));
    assert_eq!(writer.tool("mg_invented", json!({"file":scratch.0}))["isError"], true);
    let timed_job = writer.tool(
        "mg_invented",
        json!({"file":scratch.0,"delay":true,"background":true,"timeout_ms":100}),
    )["structuredContent"]["job"]
        .clone();
    let started = std::time::Instant::now();
    let timed_result = loop {
        let result = writer.tool("moonglow_jobs", json!({"action":"status","job":timed_job}));
        if result["structuredContent"]["running"] == false {
            break result;
        }
        assert!(started.elapsed() < Duration::from_secs(3), "job exceeded its timeout: {result}");
        thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(timed_result["structuredContent"]["timedOut"], true);
    drop(writer);
    let mut old = Client::new(&scratch.0, &mg, None, &[] as &[&str]);
    let init = old.call("initialize", json!({"protocolVersion":"2025-06-18"}));
    assert!(init["result"]["instructions"].as_str().unwrap().contains("no mg_*"));
    assert!(!tools(&old.call("tools/list", json!({}))).iter().any(|n| n.starts_with("mg_")));
    drop(old);
    // The wrong certificate fingerprint offers no agent tools, and no bearer is sent.
    let calls = agent.calls.lock().unwrap().len();
    let mut wrong = Client::new(&scratch.0, &mg, None, &agent.args(&"0".repeat(64)));
    assert!(!tools(&wrong.call("tools/list", json!({}))).contains(&"invented_read".into()));
    drop(wrong);
    assert_eq!(agent.calls.lock().unwrap().len(), calls);
    // No gateway: only the CLI tools, and nothing is discovered on this machine.
    let mut none = Client::new(&scratch.0, &mg, Some(&cli), &[] as &[&str]);
    let listed = tools(&none.call("tools/list", json!({})));
    assert!(!listed.contains(&"invented_read".into()) && listed.contains(&"mg_invented".into()));
}

#[test]
fn http_requires_token_and_handles_initialize() {
    let scratch = Scratch::new();
    let mg = scratch.0.join("missing");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let address = format!("127.0.0.1:{port}");
    let _server =
        Client::new(&scratch.0, &mg, None, &["--http", &address, "--http-token", "test-http"]);
    let request = |token: &str, body: &str| {
        let mut socket = (0..100)
            .find_map(|_| match TcpStream::connect(&address) {
                Ok(s) => Some(s),
                Err(_) => {
                    thread::sleep(Duration::from_millis(20));
                    None
                }
            })
            .unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        write!(socket,"POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        response
    };
    assert!(request("wrong", "{}").starts_with("HTTP/1.1 401"));
    assert!(request("", "{}").starts_with("HTTP/1.1 401"));
    let body=json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}).to_string();
    let response = request("test-http", &body);
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("2025-06-18"));
    assert!(
        request("test-http", r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .starts_with("HTTP/1.1 202")
    );
}

#[test]
fn idle_stdio_and_http_send_heartbeats_to_the_gateway() {
    let scratch = Scratch::new();
    let agent = Agent::new();
    let args = agent.args(&agent.pin);
    let mg = scratch.0.join("no-mg");
    let mut pipes = Client::new(&scratch.0, &mg, None, &args);
    pipes.call("initialize", json!({"protocolVersion":"2025-06-18"}));
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let address = format!("127.0.0.1:{port}");
    let mut http_args: Vec<String> = args.clone();
    http_args.extend(["--http", &address, "--http-token", "test-http"].map(String::from));
    let http = Client::new(&scratch.0, &mg, None, &http_args);
    let mut socket = (0..100)
        .find_map(|_| match TcpStream::connect(&address) {
            Ok(s) => Some(s),
            Err(_) => {
                thread::sleep(Duration::from_millis(20));
                None
            }
        })
        .unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}).to_string();
    write!(socket,"POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer test-http\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    // No MCP messages for a full production 20-second interval, on either transport.
    let deadline = std::time::Instant::now() + Duration::from_secs(25);
    while agent.pings.load(Ordering::Relaxed) < 2 && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(agent.pings.load(Ordering::Relaxed), 2, "one heartbeat per transport");
    // A rejected heartbeat is never surfaced.
    agent.reject_ping.store(true, Ordering::Relaxed);
    assert_eq!(
        pipes.call("ping", json!({}))["result"],
        json!({}),
        "failed heartbeat must not write unsolicited protocol output"
    );
    // Both children are owned and reaped; no heartbeat child/helper process exists.
    drop(pipes);
    drop(http);
}

#[test]
fn a_shell_lists_and_calls_the_same_tools() {
    let scratch = Scratch::new();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_moonglow-mcp"))
            .args(args)
            .env("APPDATA", &scratch.0)
            .env("XDG_DATA_HOME", &scratch.0)
            .env("HOME", &scratch.0)
            .env("MOONGLOW_MG", scratch.0.join("no-mg"))
            .env("MOONGLOW_AI_TOKEN", "test-secret")
            .current_dir(&scratch.0)
            .output()
            .unwrap()
    };
    let listed = run(&["tools"]);
    assert!(listed.status.success(), "{}", String::from_utf8_lossy(&listed.stderr));
    assert!(String::from_utf8_lossy(&listed.stdout).contains("moonglow_sessions"));
    let called = run(&["call", "moonglow_sessions", "{}"]);
    assert!(called.status.success(), "{}", String::from_utf8_lossy(&called.stderr));
    let value: Value = serde_json::from_slice(&called.stdout).unwrap();
    assert!(value.is_object(), "{value}");
    let bad = run(&["call", "moonglow_sessions", "{not json"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("Invalid JSON input"));
    assert!(!run(&["call"]).status.success(), "usage");

    // A gateway's operations are listed beside them.
    let agent = Agent::new();
    let gateway = agent.args(&agent.pin);
    let stdout = |o: std::process::Output| String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(!stdout(run(&["tools"])).contains("invented_read"));
    let mut with_gateway = vec!["tools"];
    with_gateway.extend(gateway.iter().map(String::as_str));
    assert!(stdout(run(&with_gateway)).contains("invented_read"));
}
