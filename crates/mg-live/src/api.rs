//! Authenticated HTTPS for local clients. Game reports arrive only over Redis.
use crate::{
    access::{Access, ClientGroup},
    store::LiveStore,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[derive(Debug)]
pub(crate) struct Listener {
    pub address: SocketAddr,
    pub certificate: Vec<u8>,
    pub worker: std::thread::JoinHandle<()>,
}

/// Start a stoppable API. Dropping a service signals stop and joins this thread.
pub fn serve(
    bind: &str,
    token: String,
    store: Arc<LiveStore>,
    wake: Arc<dyn Fn() + Send + Sync>,
    stop: Arc<AtomicBool>,
) -> Result<std::thread::JoinHandle<()>, String> {
    Ok(start(bind, token, store, wake, stop)?.worker)
}

pub(crate) fn start(
    bind: &str,
    token: String,
    store: Arc<LiveStore>,
    wake: Arc<dyn Fn() + Send + Sync>,
    stop: Arc<AtomicBool>,
) -> Result<Listener, String> {
    start_with_access(bind, Arc::new(Access::new(token, &[])?), store, wake, stop)
}
pub(crate) fn start_with_access(
    bind: &str,
    access: Arc<Access>,
    store: Arc<LiveStore>,
    wake: Arc<dyn Fn() + Send + Sync>,
    stop: Arc<AtomicBool>,
) -> Result<Listener, String> {
    let address: SocketAddr =
        bind.parse().map_err(|_| "API bind must be an IP address and port")?;
    if !address.ip().is_loopback() {
        return Err("Live API bind must be loopback".into());
    }
    let listener = TcpListener::bind(address).map_err(|e| format!("{bind}: {e}"))?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let (config, certificate) = tls_config()?;
    let config = Arc::new(config);
    let worker = std::thread::spawn(move || {
        let mut clients: Vec<(std::thread::JoinHandle<()>, std::net::TcpStream)> = Vec::new();
        while !stop.load(Ordering::Acquire) {
            let mut i = 0;
            while i < clients.len() {
                if clients[i].0.is_finished() {
                    let (worker, _) = clients.swap_remove(i);
                    let _ = worker.join();
                } else {
                    i += 1;
                }
            }
            match listener.accept() {
                Ok((stream, _)) if clients.len() < 16 => {
                    // Windows accepts inherit the listener's non-blocking mode.
                    // TLS workers use bounded blocking reads; only accept polls.
                    if stream.set_nonblocking(false).is_err() {
                        continue;
                    }
                    let _ = stream.set_nodelay(true);
                    let Ok(cancel) = stream.try_clone() else {
                        continue;
                    };
                    let (config, access, store, wake) =
                        (config.clone(), access.clone(), store.clone(), wake.clone());
                    let cancelled = stop.clone();
                    let worker = std::thread::spawn(move || {
                        // shutdown() on a duplicated Windows socket need not
                        // interrupt an already-blocked recv. Bound that wait too.
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                        if let Ok(tls) = rustls::ServerConnection::new(config) {
                            let mut s = rustls::StreamOwned::new(tls, stream);
                            loop {
                                let mut request = IdleRequest {
                                    stream: &mut s,
                                    stop: &cancelled,
                                    deadline: std::time::Instant::now() + Duration::from_secs(32),
                                    started: false,
                                };
                                let drafts = store.draft_revision();
                                let result = handle(&mut request, &access, &store, 0, &cancelled);
                                let presence_wake = access.agents.take_wake();
                                if presence_wake || store.draft_revision() != drafts {
                                    wake();
                                }
                                match result {
                                    Ok(true) if !cancelled.load(Ordering::Acquire) => {}
                                    Ok(_) => {
                                        finish(&mut s);
                                        break;
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                    });
                    clients.push((worker, cancel));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(e) => {
                    log::error!("Live API accept: {e}");
                    break;
                }
            }
        }
        drop(listener);
        store.wake_event_waiters();
        for (_, stream) in &clients {
            let _ = stream.shutdown(Shutdown::Both);
        }
        for (worker, _) in clients {
            let _ = worker.join();
        }
    });
    Ok(Listener { address, certificate, worker })
}

/// Wait for the next sequential request without turning a 500ms cancellation
/// bound into a 500ms keep-alive lifetime. Partial requests remain bounded.
struct IdleRequest<'a, S> {
    stream: &'a mut S,
    stop: &'a AtomicBool,
    deadline: std::time::Instant,
    started: bool,
}
impl<S: Read> Read for IdleRequest<'_, S> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.stop.load(Ordering::Acquire) {
                return Ok(0);
            }
            match self.stream.read(bytes) {
                Err(e)
                    if !self.started
                        && matches!(
                            e.kind(),
                            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                        )
                        && std::time::Instant::now() < self.deadline => {}
                Ok(n) => {
                    self.started |= n > 0;
                    return Ok(n);
                }
                other => return other,
            }
        }
    }
}
impl<S: Write> Write for IdleRequest<'_, S> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

#[derive(Default)]
struct ResponseMeta {
    keep_alive: bool,
    etag: Option<String>,
}
fn header<'a>(head: &'a str, wanted: &str) -> Option<&'a str> {
    head.lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find_map(|(key, value)| key.eq_ignore_ascii_case(wanted).then_some(value.trim()))
}
fn catalogue(live: &LiveStore, scope: &crate::access::Scope, openapi: bool) -> Value {
    let mut policy = live.policy();
    policy.scripts &= scope.has(ClientGroup::Scripts);
    policy.chunks &= scope.has(ClientGroup::Chunks);
    policy.players &= scope.has(ClientGroup::Players);
    let mut value = if openapi {
        crate::operations::openapi(policy)
    } else {
        crate::operations::describe(policy)
    };
    if let Some(ops) = value.get_mut("operations").and_then(Value::as_array_mut) {
        ops.retain(|o| scope.allows(o["name"].as_str().unwrap_or("")));
    }
    if let Some(paths) = value.get_mut("paths").and_then(Value::as_object_mut) {
        paths.retain(|p, _| scope.allows(p.trim_start_matches("/v1/call/")));
    }
    value
}
fn catalogue_tag(value: &Value) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, value.to_string().as_bytes());
    format!("\"{}\"", digest.as_ref().iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Notify TLS clients and let them close TCP first. Actively closing every
/// response leaves the Windows listener's port in TIME_WAIT and prevents a
/// quick GUI restart. Bound the drain; uncooperative peers cannot hold a slot.
fn finish(s: &mut rustls::StreamOwned<rustls::ServerConnection, std::net::TcpStream>) {
    s.conn.send_close_notify();
    if s.flush().is_err() {
        return;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    let mut bytes = [0; 1024];
    for _ in 0..16 {
        let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else {
            break;
        };
        let _ = s.sock.set_read_timeout(Some(left));
        match s.sock.read(&mut bytes) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

fn tls_config() -> Result<(rustls::ServerConfig, Vec<u8>), String> {
    let rcgen::CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec!["moonglow-live".into()])
            .map_err(|e| format!("TLS certificate: {e}"))?;
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());
    let certificate = cert.der().to_vec();
    rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .and_then(|b| b.with_no_client_auth().with_single_cert(vec![cert.der().clone()], key))
        .map(|config| (config, certificate))
        .map_err(|e| format!("TLS: {e}"))
}

/// The request's head and body, or the status to refuse it with.
fn read_request(
    stream: &mut impl Read,
    access: &Access,
) -> Result<(String, Vec<u8>, usize), (u16, &'static str)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            if i + 4 > MAX_HEAD {
                return Err((431, "Request head too large"));
            }
            break i;
        }
        if buf.len() > MAX_HEAD {
            return Err((431, "Request head too large"));
        }
        let n = stream.read(&mut chunk).map_err(|_| (400, "Unreadable request"))?;
        if n == 0 {
            return Err((400, "Truncated request head"));
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = std::str::from_utf8(&buf[..head_end])
        .map_err(|_| (400, "Request head is not UTF-8"))?
        .to_owned();
    // Authenticate the bounded head before reading a body or routing a request.
    let mut auth = head.lines().skip(1).filter_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.eq_ignore_ascii_case("authorization").then_some(value.trim())
    });
    let who =
        auth.next().and_then(|a| a.strip_prefix("Bearer ")).and_then(|b| access.authenticate(b));
    if who.is_none() || auth.next().is_some() {
        return Err((401, "Unauthorized"));
    }
    let who = who.unwrap();
    // Refuse authenticated clients before reading/parsing their command body.
    if !access.agents.allowed() {
        return Err((403, crate::agents::DENIED));
    }
    let mut line = head.lines().next().unwrap_or_default().split_whitespace();
    let method = line.next().unwrap_or_default();
    if line.next().is_none() || line.next() != Some("HTTP/1.1") || line.next().is_some() {
        return Err((400, "Bad request line"));
    }
    let mut length = None;
    for field in head.lines().skip(1) {
        let (name, value) = field.split_once(':').ok_or((400, "Bad header"))?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err((400, "Transfer-Encoding is not supported"));
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err((400, "Duplicate Content-Length"));
            }
            length = Some(value.trim().parse::<usize>().map_err(|_| (400, "Bad Content-Length"))?);
        }
    }
    let length = match length {
        Some(n) => n,
        None if method == "POST" => return Err((411, "Content-Length required")),
        None => 0,
    };
    if length > MAX_BODY {
        return Err((413, "Body too large"));
    }
    let mut body = buf[head_end + 4..].to_vec();
    if body.len() > length {
        return Err((400, "Body longer than Content-Length"));
    }
    let start = body.len();
    body.resize(length, 0);
    stream.read_exact(&mut body[start..]).map_err(|_| (400, "Truncated body"))?;
    Ok((head, body, who))
}

fn answer(
    stream: &mut impl Read,
    access: &Access,
    live: &LiveStore,
    now_ms: u64,
    stop: &AtomicBool,
    meta: &mut ResponseMeta,
) -> (u16, Value) {
    let (head, body, who) = match read_request(stream, access) {
        Ok(r) => r,
        Err((403, why)) => return (403, json!({"err":why})),
        Err((status, why)) => return (status, json!({"ok":false,"error":why})),
    };
    meta.keep_alive =
        header(&head, "connection").is_some_and(|v| v.eq_ignore_ascii_case("keep-alive"));
    let now_ms = if now_ms == 0 { now() } else { now_ms };
    let mut line = head.lines().next().unwrap_or("").split(' ');
    let (method, path) = (line.next().unwrap_or(""), line.next().unwrap_or(""));
    let scope = access.scope(who);
    let input: Result<Value, _> = serde_json::from_slice(&body);
    let op = match path {
        "/v1/ping" => "ping",
        "/v1/describe" => "describe",
        "/v1/openapi.json" => "openapi",
        "/live/v1/status" => "live_status",
        "/live/v1/command" => {
            input.as_ref().ok().and_then(|v| v["op"].as_str()).unwrap_or("unknown")
        }
        p if p.starts_with("/live/v1/area/") => "live_area_state",
        p if p.starts_with("/v1/call/") => p.trim_start_matches("/v1/call/"),
        _ => "unknown",
    };
    // Never retain arbitrary path/body strings (or secrets hidden in them).
    let presence_op =
        if matches!(op, "ping" | "describe" | "openapi") || crate::access::group(op).is_some() {
            op
        } else {
            "unknown"
        };
    // Recheck after reading the body: consent may have changed during a slow request.
    if !access.agents.request(&scope.name, presence_op, now_ms) {
        return (403, json!({"err":crate::agents::DENIED}));
    }
    if path == "/v1/ping" {
        return if method == "GET" {
            (200, json!({"ok":true}))
        } else {
            (405, json!({"ok":false,"error":"Use GET"}))
        };
    }
    let target =
        input.as_ref().ok().and_then(|v| v["target"].as_str().or_else(|| v["player"].as_str()));
    let response = (|| {
        let expected = match path {
            "/v1/describe" | "/v1/openapi.json" | "/live/v1/status" => "GET",
            p if p.starts_with("/live/v1/area/") => "GET",
            "/live/v1/command" => "POST",
            p if p.starts_with("/v1/call/") => "POST",
            _ => return (404, json!({"ok":false,"error":"Not found"})),
        };
        if method != expected {
            return (405, json!({"ok":false,"error":format!("Use {expected}")}));
        }
        if matches!(op, "describe" | "openapi") {
            let value = catalogue(live, scope, op == "openapi");
            meta.etag = Some(catalogue_tag(&value));
            return (200, value);
        }
        if path.starts_with("/v1/call/")
            && let Some(expected) = header(&head, "if-match")
        {
            let current = catalogue_tag(&catalogue(live, scope, false));
            meta.etag = Some(current.clone());
            if expected != current {
                return (
                    412,
                    json!({"ok":false,"code":"catalogue_changed","error":"Refresh /v1/describe before dispatch"}),
                );
            }
        }
        if !scope.allows(op) {
            return (
                403,
                json!({"ok":false,"code":"forbidden","error":"Client scope does not allow this operation"}),
            );
        }
        if method == "POST"
            && (crate::access::group(op) != Some(ClientGroup::Read)
                || crate::operations::executes_in_game(op))
            && let Err(seconds) = access.command(who, now_ms)
        {
            return (
                429,
                json!({"ok":false,"code":"rate_limited","error":"Client command limit exceeded","retryAfterSeconds":seconds}),
            );
        }
        if path == "/live/v1/status" {
            let mut value = live.status_scoped(now_ms, scope);
            value["agents"] = json!(access.agents.recent(now_ms));
            return (200, value);
        }
        if let Some(area) = path.strip_prefix("/live/v1/area/") {
            return live
                .area_json(area, now_ms)
                .map(|v| (200, v))
                .unwrap_or((404, json!({"error":"Not found"})));
        }
        let input = match &input {
            Ok(v) => v.clone(),
            Err(_) => return (400, json!({"ok":false,"code":"invalid","error":"Bad JSON"})),
        };
        let legacy = path == "/live/v1/command";
        let result = if legacy {
            crate::operations::command_scoped(input, live, now_ms, scope)
        } else {
            crate::operations::call_scoped(op, input, live, stop, scope)
        };
        match result {
            Ok(mut v) => {
                if op == "live_status" {
                    v["agents"] = json!(access.agents.recent(now_ms));
                }
                (200, if legacy { v } else { json!({"ok":true,"result":v}) })
            }
            Err(e) => (
                if e.code == crate::store::ErrorCode::Forbidden { 403 } else { 400 },
                json!({"ok":false,"error":e.error,"code":e.code}),
            ),
        }
    })();
    // Never include request bodies, credentials, script errors or returned values.
    let (status, value) = &response;
    let result = if *status == 200 {
        if value.get("queued").is_some() || value["result"].get("queued").is_some() {
            "queued"
        } else {
            "ok"
        }
    } else {
        value["code"].as_str().unwrap_or(match status {
            404 => "not_found",
            405 => "method_not_allowed",
            _ => "invalid",
        })
    };
    access.audit.record(&scope.name, op, target, result, "request");
    response
}

/// Answers one request. Only explicit keep-alive permits another request.
fn handle(
    stream: &mut (impl Read + Write),
    access: &Access,
    live: &LiveStore,
    now_ms: u64,
    stop: &AtomicBool,
) -> std::io::Result<bool> {
    let mut meta = ResponseMeta::default();
    let (status, body) = answer(stream, access, live, now_ms, stop, &mut meta);
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        429 => "Too Many Requests",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Bad Request",
    };
    let retry = if status == 429 {
        format!("Retry-After: {}\r\n", body["retryAfterSeconds"].as_u64().unwrap_or(60))
    } else {
        String::new()
    };
    let body = body.to_string();
    let connection = if meta.keep_alive { "keep-alive" } else { "close" };
    let etag = meta.etag.map(|tag| format!("ETag: {tag}\r\n")).unwrap_or_default();
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: {connection}\r\n{retry}{etag}\r\n{body}",
        body.len()
    )?;
    stream.flush()?;
    Ok(meta.keep_alive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const PATH: &str = "/live/v1/command";
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn allowed_access(clients: &[crate::access::ClientToken]) -> Access {
        let access = Access::new(TOKEN.into(), clients).unwrap();
        access.agents.set_allowed(true);
        access
    }

    struct Duplex {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl Read for Duplex {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }
    impl Write for Duplex {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn exchange(live: &LiveStore, raw: &[u8]) -> (u16, Value) {
        let mut d = Duplex { input: Cursor::new(raw.to_vec()), output: Vec::new() };
        handle(&mut d, &allowed_access(&[]), live, 1000, &AtomicBool::new(false)).unwrap();
        let text = String::from_utf8(d.output).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        assert!(head.contains(&format!("Content-Length: {}", body.len())));
        (status, serde_json::from_str(body).unwrap())
    }

    fn post(path: &str, token: &str, body: &str) -> Vec<u8> {
        format!(
            "POST {path} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    const SYNC: &str = r#"{"v":1,"boot":"b","seq":1,"areas":[{"area":"a","cre":[{"id":"7f01"}]}]}"#;

    #[test]
    fn consent_gates_all_tokens_and_routes_but_not_local_use() {
        let live = LiveStore::default();
        live.sync(SYNC.as_bytes(), 1000).unwrap();
        let client = crate::access::ClientToken {
            name: "campaign".into(),
            token: "z".repeat(32),
            ..Default::default()
        };
        let access = Access::new(TOKEN.into(), std::slice::from_ref(&client)).unwrap();
        for token in [TOKEN, client.token.as_str()] {
            for path in [
                "/v1/describe",
                "/v1/ping",
                "/v1/openapi.json",
                "/v1/call/live_status",
                "/v1/call/live_destroy",
                "/live/v1/status",
                "/live/v1/area/a",
                "/live/v1/command",
            ] {
                let raw = if path.contains("/call/") || path.ends_with("command") {
                    post(path, token, r#"{"op":"destroy","area":"a","target":"7f01"}"#)
                } else {
                    format!("GET {path} HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n")
                        .into_bytes()
                };
                let (status, body, _) = scoped_exchange(&live, &access, &raw, 1000);
                assert_eq!((status, body), (403, json!({"err":crate::agents::DENIED})), "{path}");
            }
        }
        assert!(access.agents.recent(1000).is_empty());
        assert_eq!(
            crate::operations::call(
                "live_area_state",
                json!({"area":"a"}),
                &live,
                &AtomicBool::new(false)
            )
            .unwrap()["area"],
            "a"
        );
        access.agents.set_allowed(true);
        let get =
            format!("GET /v1/ping HTTP/1.1\r\nAuthorization: Bearer {}\r\n\r\n", client.token);
        assert_eq!(scoped_exchange(&live, &access, get.as_bytes(), 1100).0, 200);
        assert_eq!(
            access.agents.recent(1100),
            vec![crate::AgentPresence {
                client: "campaign".into(),
                last_ms: 1100,
                op: "ping".into()
            }]
        );
        let (_, status, _) =
            scoped_exchange(&live, &access, &post("/v1/call/live_status", TOKEN, "{}"), 1200);
        assert_eq!(status["result"]["agents"].as_array().unwrap().len(), 2);
        assert_eq!(access.agents.recent(61100).len(), 1);
        assert!(access.agents.recent(61200).is_empty());
        assert_eq!(
            scoped_exchange(&live, &access, &post("/v1/call/live_status", "wrong", "{}"), 62000).0,
            401
        );
        assert!(access.agents.recent(62000).is_empty());
    }

    #[test]
    fn command_refusals_have_the_same_code_on_both_routes() {
        let live = LiveStore::default();
        live.sync(
            br#"{"v":1,"boot":"b","areas":[{"area":"a","cre":[{"id":"1","dead":true}]}]}"#,
            1000,
        )
        .unwrap();
        for path in ["/live/v1/command", "/v1/call/live_npc_move"] {
            let mut body = json!({"area":"a","target":"1","x":1,"y":2,"z":0});
            if path == "/live/v1/command" {
                body["op"] = json!("npc_move");
            }
            let (status, value) = exchange(&live, &post(path, TOKEN, &body.to_string()));
            assert_eq!(status, 400);
            assert_eq!(value["code"], "dead");
            assert_eq!(value["error"], "Target is dead");
        }
    }

    #[test]
    fn game_http_route_is_removed() {
        let live = LiveStore::default();
        assert_eq!(exchange(&live, &post("/live/v1/sync", TOKEN, SYNC)).0, 404);
        assert!(live.area("a").is_none());
    }

    #[test]
    fn command_schema_limits_match_on_both_routes() {
        let live = LiveStore::default();
        live.sync(SYNC.as_bytes(), 1000).unwrap();
        for path in ["/live/v1/command", "/v1/call/live_npc_say"] {
            for length in [200, 201] {
                let mut body = json!({"area":"a","target":"7f01","text":"x".repeat(length)});
                if path == "/live/v1/command" {
                    body["op"] = json!("npc_say");
                }
                let (status, value) = exchange(&live, &post(path, TOKEN, &body.to_string()));
                assert_eq!(status, if length == 200 { 200 } else { 400 }, "{path}: {value}");
                if length == 201 {
                    assert_eq!(value["code"], "invalid");
                    assert_eq!(value["error"], "text: String length is out of range");
                }
            }
        }
    }

    #[test]
    fn refusals() {
        let live = LiveStore::default();
        assert_eq!(exchange(&live, &post(PATH, "wrong", SYNC)).0, 401);
        assert_eq!(exchange(&live, &post("/other", TOKEN, SYNC)).0, 404);
        let get = String::from_utf8(post(PATH, TOKEN, SYNC)).unwrap().replacen("POST", "GET", 1);
        assert_eq!(exchange(&live, get.as_bytes()).0, 405);
        assert_eq!(exchange(&live, &post(PATH, TOKEN, "{}")).0, 400);
        let huge = format!(
            "POST {PATH} HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert_eq!(exchange(&live, huge.as_bytes()).0, 413);
        let no_length = format!("POST {PATH} HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\n\r\n");
        assert_eq!(exchange(&live, no_length.as_bytes()).0, 411);
        let endless = vec![b'a'; MAX_HEAD * 2];
        assert_eq!(exchange(&live, &endless).0, 431);
        assert!(live.area("a").is_none(), "nothing refused was stored");
    }

    #[test]
    fn truncated_requests_are_refused() {
        let live = LiveStore::default();
        let full = post(PATH, TOKEN, SYNC);
        for cut in 0..full.len() {
            assert_eq!(exchange(&live, &full[..cut]).0 / 100, 4, "cut at {cut}");
        }
    }

    fn get(path: &str) -> Vec<u8> {
        format!("GET {path} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\n\r\n")
            .into_bytes()
    }

    #[test]
    fn clients_read_reports_and_queue_idempotent_commands() {
        let live = LiveStore::default();
        let cmd = r#"{"id":"retry","area":"a","op":"destroy","target":"7f01"}"#;
        assert_eq!(exchange(&live, &get("/live/v1/status")).1["connected"], false);
        assert_eq!(exchange(&live, &post("/live/v1/command", TOKEN, cmd)).0, 400);
        assert_eq!(exchange(&live, &get("/live/v1/area/a")).0, 404);
        live.sync(SYNC.as_bytes(), 1000).unwrap();
        assert_eq!(exchange(&live, &get("/live/v1/area/a")).1["creatures"][0]["id"], "7f01");
        for _ in 0..2 {
            assert_eq!(
                exchange(&live, &post("/live/v1/command", TOKEN, cmd)),
                (200, json!({"queued":"retry"}))
            );
        }
        assert_eq!(
            exchange(&live, &get("/live/v1/status")).1["queued"].as_array().unwrap().len(),
            1
        );
        assert_eq!(exchange(&live, &get("/live/v1/command")).0, 405);
    }

    #[test]
    fn authentication_precedes_body_parsing_and_heads_are_bounded() {
        let live = LiveStore::default();
        assert_eq!(
            exchange(&live, b"POST /other HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n").0,
            401
        );
        let duplicate = format!(
            "POST {PATH} HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n"
        );
        assert_eq!(exchange(&live, duplicate.as_bytes()).0, 400);
        let huge = format!(
            "GET /live/v1/status HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\nX: {}\r\n\r\n",
            "x".repeat(MAX_HEAD)
        );
        assert_eq!(exchange(&live, huge.as_bytes()).0, 431);
    }

    /// Real TCP/TLS client commands; game state is injected through the store.
    #[test]
    fn real_tls_status_command_and_stop() {
        let live = Arc::new(LiveStore::default());
        live.sync(SYNC.as_bytes(), now()).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let listener = start_with_access(
            "127.0.0.1:0",
            Arc::new(allowed_access(&[])),
            live.clone(),
            Arc::new(|| {}),
            stop.clone(),
        )
        .unwrap();
        let address = listener.address;
        let request = |path: &str, body: Option<&str>| {
            let mut curl =
                std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
            curl.args([
                "--silent",
                "--show-error",
                "--fail",
                "--insecure",
                "--noproxy",
                "*",
                "--max-time",
                "5",
                "-H",
                &format!("Authorization: Bearer {TOKEN}"),
            ]);
            if let Some(body) = body {
                curl.args(["-H", "Content-Type: application/json", "--data-binary", body]);
            }
            let out = curl.arg(format!("https://{address}{path}")).output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            serde_json::from_slice::<Value>(&out.stdout).unwrap()
        };
        assert_eq!(request("/live/v1/status", None)["areas"][0]["area"], "a");
        assert_eq!(
            request(PATH, Some(r#"{"id":"tls-test","area":"a","op":"destroy","target":"7f01"}"#))["queued"],
            "tls-test"
        );
        assert_eq!(live.commands(now())["commands"][0]["id"], "tls-test");
        live.sync(br#"{"v":1,"boot":"b","seq":2,"ack":["tls-test"]}"#, now()).unwrap();
        assert_eq!(
            request("/live/v1/status", None)["results"][0]["result"]["err"],
            "unknown: acknowledged; result not received"
        );
        let stalled = std::net::TcpStream::connect(address).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let before = std::time::Instant::now();
        stop.store(true, Ordering::Release);
        listener.worker.join().unwrap();
        assert!(before.elapsed() < Duration::from_secs(2));
        drop(stalled);
    }

    #[test]
    fn loopback_only_and_exclusive_listener() {
        let live = Arc::new(LiveStore::default());
        let stop = Arc::new(AtomicBool::new(false));
        assert!(
            start("0.0.0.0:0", TOKEN.into(), live.clone(), Arc::new(|| {}), stop.clone()).is_err()
        );
        let listener =
            start("127.0.0.1:0", TOKEN.into(), live, Arc::new(|| {}), stop.clone()).unwrap();
        assert!(TcpListener::bind(listener.address).is_err());
        stop.store(true, Ordering::Release);
        listener.worker.join().unwrap();
        let rebound = TcpListener::bind(listener.address).unwrap();
        drop(rebound);
    }

    #[test]
    #[ignore = "manual real TLS client smoke"]
    fn real_tls_status() {
        real_tls_status_command_and_stop();
    }
    fn scoped_exchange(
        live: &LiveStore,
        access: &Access,
        raw: &[u8],
        time: u64,
    ) -> (u16, Value, String) {
        let mut d = Duplex { input: Cursor::new(raw.to_vec()), output: vec![] };
        handle(&mut d, access, live, time, &AtomicBool::new(false)).unwrap();
        let text = String::from_utf8(d.output).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        (
            head.split(' ').nth(1).unwrap().parse().unwrap(),
            serde_json::from_str(body).unwrap(),
            head.into(),
        )
    }
    fn named(groups: Vec<ClientGroup>, per_minute: u32) -> crate::access::ClientToken {
        crate::access::ClientToken {
            name: "LLM".into(),
            token: "scoped-0123456789abcdef0123456789abcdef".into(),
            groups,
            per_minute,
            delivery: Default::default(),
        }
    }
    #[test]
    fn catalogue_precondition_precedes_dispatch_and_tracks_policy_and_scope() {
        let live = LiveStore::default();
        live.sync(SYNC.as_bytes(), 1000).unwrap();
        let c = named(vec![ClientGroup::Read], 1);
        let access = allowed_access(std::slice::from_ref(&c));
        let (_, _, head) = scoped_exchange(&live, &access, &get("/v1/describe"), 1000);
        let tag = header(&head, "etag").unwrap();
        assert!(tag.starts_with('"') && tag.ends_with('"'));
        let scoped = String::from_utf8(get("/v1/describe")).unwrap().replace(TOKEN, &c.token);
        let (_, _, other) = scoped_exchange(&live, &access, scoped.as_bytes(), 1000);
        assert_ne!(header(&other, "etag"), Some(tag));
        let body = r#"{"id":"conditional","area":"a","target":"7f01"}"#;
        let raw = String::from_utf8(post("/v1/call/live_destroy", TOKEN, body)).unwrap();
        let stale = raw.replace("Host: x", "Host: x\r\nIf-Match: \"stale\"");
        let (status, _, h) = scoped_exchange(&live, &access, stale.as_bytes(), 1000);
        assert_eq!(status, 412);
        assert_eq!(header(&h, "etag"), Some(tag));
        assert!(live.commands(1000)["commands"].as_array().unwrap().is_empty());
        let valid = raw.replace("Host: x", &format!("Host: x\r\nIf-Match: {tag}"));
        assert_eq!(scoped_exchange(&live, &access, valid.as_bytes(), 1000).0, 200);
        live.set_policy(crate::store::Policy { players: true, ..live.policy() });
        assert_eq!(scoped_exchange(&live, &access, valid.as_bytes(), 1000).0, 412);
    }

    #[test]
    fn real_tls_keep_alive_reauthenticates_and_stops_an_idle_connection() {
        let live = Arc::new(LiveStore::default());
        let access = Arc::new(allowed_access(&[]));
        let stop = Arc::new(AtomicBool::new(false));
        let listener =
            start_with_access("127.0.0.1:0", access.clone(), live, Arc::new(|| {}), stop.clone())
                .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(listener.certificate.clone().into()).unwrap();
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let conn =
            rustls::ClientConnection::new(Arc::new(config), "moonglow-live".try_into().unwrap())
                .unwrap();
        let tcp = std::net::TcpStream::connect(listener.address).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        tcp.set_nodelay(true).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let request = |tls: &mut rustls::StreamOwned<
            rustls::ClientConnection,
            std::net::TcpStream,
        >,
                       token: &str| {
            write!(tls,"GET /v1/ping HTTP/1.1\r\nAuthorization: Bearer {token}\r\nConnection: keep-alive\r\n\r\n").unwrap();
            tls.flush().unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                tls.read_exact(&mut b).unwrap();
                head.push(b[0]);
            }
            let head = String::from_utf8(head).unwrap();
            let mut body = vec![0; header(&head, "content-length").unwrap().parse().unwrap()];
            tls.read_exact(&mut body).unwrap();
            (head, serde_json::from_slice::<Value>(&body).unwrap())
        };
        assert!(request(&mut tls, TOKEN).0.contains("Connection: keep-alive"));
        std::thread::sleep(Duration::from_millis(650));
        assert_eq!(request(&mut tls, TOKEN).1["ok"], true);
        access.agents.set_allowed(false);
        let (head, body) = request(&mut tls, TOKEN);
        assert!(head.starts_with("HTTP/1.1 403"));
        assert_eq!(body["err"], crate::agents::DENIED);
        drop(tls);
        let idle = std::net::TcpStream::connect(listener.address).unwrap();
        std::thread::sleep(Duration::from_millis(60));
        let start = std::time::Instant::now();
        stop.store(true, Ordering::Release);
        listener.worker.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(idle);
    }
    #[test]
    fn locals_read_and_variables_scopes_are_independent_on_both_routes() {
        let live = LiveStore::default();
        let time = now();
        live.sync(br#"{"v":1,"boot":"b","seq":1,"areas":[{"area":"a","cre":[{"id":"1"}]}]}"#, time)
            .unwrap();
        for groups in [vec![ClientGroup::Read], vec![ClientGroup::Variables]] {
            let c = named(groups.clone(), 100);
            let access = allowed_access(std::slice::from_ref(&c));
            for (op, read) in
                [("get_variable", true), ("dump_locals", true), ("set_variable", false)]
            {
                for legacy in [false, true] {
                    let mut input = json!({"area":"a","target":"module"});
                    if op != "dump_locals" {
                        input["type"] = json!("int");
                        input["name"] = json!("quest");
                    }
                    if op == "set_variable" {
                        input["value"] = json!(42);
                    }
                    let path = if legacy {
                        input["op"] = json!(op);
                        PATH.to_string()
                    } else {
                        format!("/v1/call/live_{op}")
                    };
                    let (status, _, _) = scoped_exchange(
                        &live,
                        &access,
                        &post(&path, &c.token, &input.to_string()),
                        time,
                    );
                    assert_eq!(
                        status,
                        if read == groups.contains(&ClientGroup::Read) { 200 } else { 403 },
                        "{op}"
                    );
                }
            }
        }
    }
    #[test]
    fn scopes_enforce_both_routes_discovery_and_players_flag() {
        let live = LiveStore::default();
        live.set_policy(crate::store::Policy { scripts: true, chunks: true, players: true });
        let time = now();
        live.sync(br#"{"v":1,"boot":"b","seq":1,"areas":[{"area":"a","cre":[{"id":"1"},{"id":"2","pc":true}]}]}"#,time).unwrap();
        let c = named(vec![ClientGroup::Read, ClientGroup::Npc], 100);
        let access = allowed_access(std::slice::from_ref(&c));
        let send = |path: &str, body: Value| {
            scoped_exchange(&live, &access, &post(path, &c.token, &body.to_string()), time)
        };
        for (op, input) in [
            ("spawn", json!({})),
            ("run_script", json!({})),
            ("run_chunk", json!({})),
            ("pc_give_gold", json!({"area":"a","target":"2","amount":1})),
        ] {
            let mut legacy = input.clone();
            legacy["op"] = json!(op);
            assert_eq!(send(PATH, legacy).0, 403);
            assert_eq!(send(&format!("/v1/call/live_{op}"), input).0, 403);
        }
        for path in [PATH, "/v1/call/live_freeze"] {
            let mut input = json!({"area":"a","target":"2","seconds":5});
            if path == PATH {
                input["op"] = json!("freeze");
            }
            let (status, v, _) = send(path, input);
            assert_eq!(status, 400);
            assert_eq!(v["code"], "protected");
        }
        let (status, _, _) = send(PATH, json!({"area":"a","op":"freeze","target":"1","seconds":5}));
        assert_eq!(status, 200);
        assert!(live.commands(time)["commands"][0].get("players").is_none());
        assert_eq!(
            send(PATH, json!({"area":"a","op":"freeze","target":"1","seconds":5,"players":true})).0,
            400
        );
        for path in ["/v1/describe", "/v1/openapi.json"] {
            let raw = String::from_utf8(get(path)).unwrap().replace(TOKEN, &c.token);
            let (status, v, _) = scoped_exchange(&live, &access, raw.as_bytes(), time);
            assert_eq!(status, 200);
            assert!(v.get(if path == "/v1/describe" { "paths" } else { "operations" }).is_none());
            assert!(!v.to_string().contains("live_run_chunk"));
            assert!(!v.to_string().contains("\"live_spawn\""));
            assert!(v.to_string().contains("live_freeze"));
        }
        let c = named(vec![ClientGroup::Players, ClientGroup::Npc], 100);
        let access = allowed_access(std::slice::from_ref(&c));
        let cmd = json!({"area":"a","target":"2","seconds":5});
        assert_eq!(
            scoped_exchange(
                &live,
                &access,
                &post("/v1/call/live_freeze", &c.token, &cmd.to_string()),
                time
            )
            .0,
            200
        );
        assert_eq!(live.commands(time)["commands"][1]["players"], true);
        live.set_policy(crate::store::Policy::default());
        let cmd = json!({"area":"a","target":"2","amount":1});
        let (_, v, _) = scoped_exchange(
            &live,
            &access,
            &post("/v1/call/live_pc_give_gold", &c.token, &cmd.to_string()),
            time,
        );
        assert_eq!(v["code"], "disabled");
    }
    #[test]
    fn quota_has_retry_after_and_reads_do_not_spend_it() {
        let live = LiveStore::default();
        let time = now();
        live.sync(SYNC.as_bytes(), time).unwrap();
        let c = named(vec![ClientGroup::Read, ClientGroup::Npc], 1);
        let access = allowed_access(std::slice::from_ref(&c));
        let raw =
            post(PATH, &c.token, r#"{"id":"quota","area":"a","op":"destroy","target":"7f01"}"#);
        assert_eq!(scoped_exchange(&live, &access, &raw, time).0, 200);
        let (status, v, head) = scoped_exchange(&live, &access, &raw, time + 1);
        assert_eq!(status, 429);
        assert_eq!(v["code"], "rate_limited");
        assert!(head.lines().any(|line| line == "Retry-After: 60"));
        assert_eq!(
            scoped_exchange(
                &live,
                &access,
                &post("/v1/call/live_status", &c.token, "{}"),
                time + 1
            )
            .0,
            200
        );
        assert_eq!(scoped_exchange(&live, &access, &raw, time + 60000).0, 200);
    }

    #[test]
    fn game_reads_spend_the_client_quota_on_both_http_routes() {
        for op in ["get_variable", "dump_locals", "capture_object"] {
            for legacy in [false, true] {
                let live = LiveStore::default();
                let time = now();
                live.sync(SYNC.as_bytes(), time).unwrap();
                let client = named(vec![ClientGroup::Read], 1);
                let access = allowed_access(std::slice::from_ref(&client));
                let mut input = json!({"id":"read","area":"a","target":"7f01"});
                if op == "get_variable" {
                    input["type"] = json!("int");
                    input["name"] = json!("example");
                }
                let path = if legacy {
                    input["op"] = json!(op);
                    PATH.into()
                } else {
                    format!("/v1/call/live_{op}")
                };
                let raw = post(&path, &client.token, &input.to_string());
                assert_eq!(scoped_exchange(&live, &access, &raw, time).0, 200, "{op}");
                assert_eq!(scoped_exchange(&live, &access, &raw, time + 1).0, 429, "{op}");
                assert_eq!(
                    scoped_exchange(
                        &live,
                        &access,
                        &post("/v1/call/live_status", &client.token, "{}"),
                        time + 1
                    )
                    .0,
                    200
                );
            }
        }
    }
    #[test]
    fn chat_and_other_clients_results_cannot_leak_or_claim_ids() {
        let live = LiveStore::default();
        let time = now();
        live.sync(br#"{"v":1,"boot":"b","seq":1,"areas":[{"area":"a","cre":[{"id":"1"}]}],"events":[{"seq":1,"kind":"damage","area":"a","amount":5},{"seq":2,"kind":"chat","area":"a","text":"CHAT_SECRET"},{"seq":3,"kind":"dialog","area":"a","text":"DIALOG_SECRET"}]}"#,time).unwrap();
        live.server_log(
            &json!({"kind":"server_log","stream":"s","seq":1,"line":"CHAT_SECRET"}),
            time,
        );
        let c = named(vec![ClientGroup::Read, ClientGroup::Npc], 100);
        let access = allowed_access(std::slice::from_ref(&c));
        let send = |token: &str, path: &str, body: Value| {
            scoped_exchange(&live, &access, &post(path, token, &body.to_string()), time)
        };
        let (_, v, _) = send(&c.token, "/v1/call/live_events", json!({"limit":1}));
        assert_eq!(v["result"]["events"][0]["kind"], "damage");
        assert!(!v.to_string().contains("CHAT_SECRET"));
        assert!(!v.to_string().contains("DIALOG_SECRET"));
        let cmd =
            json!({"id":"owned","op":"npc_say","area":"a","target":"1","text":"COMMAND_SECRET"});
        assert_eq!(send(TOKEN, PATH, cmd.clone()).0, 200);
        assert_eq!(send(&c.token, PATH, cmd).0, 403);
        assert_eq!(
            send(&c.token, "/v1/call/live_wait", json!({"id":"owned","timeout_ms":0})).0,
            400
        );
        assert_eq!(
            send(&c.token, "/v1/call/live_status", json!({})).1["result"]["queued"],
            json!([])
        );
        live.sync(br#"{"v":1,"boot":"b","seq":2,"results":[{"id":"owned","ok":true,"value":"RETURN_SECRET"}],"ack":["owned"]}"#,time).unwrap();
        for op in ["live_status", "live_events"] {
            let (_, v, _) = send(&c.token, &format!("/v1/call/{op}"), json!({}));
            assert!(!v.to_string().contains("SECRET"));
        }
        let c = named(vec![ClientGroup::Read, ClientGroup::ReadChat], 100);
        let a = allowed_access(std::slice::from_ref(&c));
        let (_, v, _) =
            scoped_exchange(&live, &a, &post("/v1/call/live_events", &c.token, "{}"), time);
        assert!(v.to_string().contains("CHAT_SECRET"));
        assert!(v.to_string().contains("DIALOG_SECRET"));
        assert!(!v.to_string().contains("RETURN_SECRET"));
        assert!(
            send(TOKEN, "/v1/call/live_wait", json!({"id":"owned","timeout_ms":0}))
                .1
                .to_string()
                .contains("RETURN_SECRET")
        );
    }

    #[test]
    fn named_client_real_tls_audit_and_descriptor_keep_secrets_private() {
        use crate::{LiveConfig, LiveService};
        let c = named(vec![ClientGroup::Read, ClientGroup::Npc], 1);
        let live = Arc::new(LiveStore::default());
        let dir = std::env::temp_dir().join(format!(
            "mg-live-token-proof-{}-{}",
            std::process::id(),
            now()
        ));
        assert!(!dir.exists());
        let mut service = LiveService::start(
            LiveConfig {
                enabled: true,
                allow_agents: false,
                api_port: 0,
                redis_port: 1,
                token: TOKEN.into(),
                clients: vec![c.clone()],
                keep_logs: false,
                keep_wsl: Some(false),
                ..Default::default()
            },
            live.clone(),
            Arc::new(|| {}),
        )
        .unwrap();
        service.publish_descriptor(&dir).unwrap();
        // A newly bound endpoint needs its own report before accepting commands.
        live.sync(SYNC.as_bytes(), now()).unwrap();
        let descriptor = dir.join(format!("live/sessions/{}.json", std::process::id()));
        assert!(!descriptor.exists(), "off by default means no discovery record");
        service.set_allow_agents(true).unwrap();
        let record =
            std::fs::read_to_string(dir.join(format!("live/sessions/{}.json", std::process::id())))
                .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&record).unwrap()["token"], TOKEN);
        assert!(!record.contains(&c.token));
        assert!(!record.contains("LLM"));
        assert!(!format!("{service:?}").contains(&c.token));
        assert!(!format!("{service:?}").contains(TOKEN));
        let mut roots = rustls::RootCertStore::empty();
        roots.add(rustls::pki_types::CertificateDer::from(service.certificate().to_vec())).unwrap();
        let cfg = Arc::new(
            rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth(),
        );
        let port = service.config().api_port;
        let request = |token: &str, op: &str, body: &str| {
            let socket = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            let conn =
                rustls::ClientConnection::new(cfg.clone(), "moonglow-live".try_into().unwrap())
                    .unwrap();
            let mut stream = rustls::StreamOwned::new(conn, socket);
            stream.write_all(&post(op, token, body)).unwrap();
            stream.flush().unwrap();
            let mut text = String::new();
            stream.read_to_string(&mut text).unwrap();
            let (head, body) = text.split_once("\r\n\r\n").unwrap();
            (head.to_string(), serde_json::from_str::<Value>(body).unwrap())
        };
        let body = r#"{"id":"named-proof","area":"a","target":"7f01","text":"DO_NOT_AUDIT_TEXT"}"#;
        assert!(request("wrong", "/v1/call/live_status", "{}").0.starts_with("HTTP/1.1 401"));
        let (head, reply) = request(&c.token, "/v1/call/live_npc_say", body);
        assert!(head.starts_with("HTTP/1.1 200"));
        assert_eq!(reply["result"]["queued"], "named-proof");
        assert!(request(&c.token, "/v1/call/live_npc_say", body).0.contains("Retry-After: 60"));
        assert!(request(&c.token, "/v1/call/live_spawn", "{}").0.starts_with("HTTP/1.1 403"));
        live.sync(br#"{"v":1,"boot":"b","seq":2,"results":[{"id":"named-proof","ok":false,"code":"invalid","err":"DO_NOT_AUDIT_ERROR","value":"DO_NOT_AUDIT_RETURN"}],"ack":["named-proof"]}"#,now()).unwrap();
        let (_, wait) =
            request(&c.token, "/v1/call/live_wait", r#"{"id":"named-proof","timeout_ms":0}"#);
        assert_eq!(wait["result"]["code"], "invalid");
        let audit = std::fs::read_to_string(dir.join("live/audit.jsonl")).unwrap();
        for secret in
            [TOKEN, &c.token, "DO_NOT_AUDIT_TEXT", "DO_NOT_AUDIT_ERROR", "DO_NOT_AUDIT_RETURN"]
        {
            assert!(!audit.contains(secret));
        }
        let records: Vec<Value> = audit.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert!(records.iter().any(|r| r["who"] == "LLM"
            && r["op"] == "npc_say"
            && r["target"] == "7f01"
            && r["result"] == "queued"));
        assert!(
            records.iter().any(|r| r["who"] == "LLM"
                && r["phase"] == "completion"
                && r["result"] == "invalid")
        );
        assert!(records.iter().any(|r| r["result"] == "rate_limited"));
        assert!(records.iter().any(|r| r["result"] == "forbidden"));
        assert_eq!(service.agents(now())[0].client, "LLM");
        assert!(!service.agents(now())[0].op.contains("DO_NOT_AUDIT"));
        assert!(service.agents(now() + 60_000).is_empty());
        let revision = service.agents_revision();
        service.set_allow_agents(false).unwrap();
        assert!(service.agents_revision() > revision);
        assert!(!descriptor.exists());
        assert!(service.agents(now()).is_empty());
        for token in [TOKEN, c.token.as_str()] {
            let (head, denied) = request(token, "/v1/call/live_status", "{}");
            assert!(head.starts_with("HTTP/1.1 403"));
            assert_eq!(denied, json!({"err":crate::agents::DENIED}));
        }
        service.set_allow_agents(true).unwrap();
        assert!(descriptor.exists());
        assert_eq!(service.config().api_port, port);
        assert!(request(TOKEN, "/v1/call/live_status", "{}").0.starts_with("HTTP/1.1 200"));
        service.publish_descriptor(&dir).unwrap();
        assert!(descriptor.exists(), "repeated publication must not remove its own file");
        service.stop();
        assert!(!descriptor.exists());
        drop(live);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
