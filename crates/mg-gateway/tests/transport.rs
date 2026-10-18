use futures_util::{SinkExt, StreamExt};
use mg_gateway::{Authenticator, Config, CoreHandle, Principal};
use mg_live::store::{LiveStore, Policy};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

#[derive(Debug)]
struct Auth;
impl Authenticator for Auth {
    fn principal(&self, id: &str) -> Option<Principal> {
        match id {
            "dm:test" => self.authenticate("test-credential"),
            "dm:reader" => self.authenticate("test-reader"),
            _ => None,
        }
    }
    fn authenticate(&self, token: &str) -> Option<Principal> {
        let (id, groups) = match token {
            "test-credential" => (
                "dm:test",
                vec![
                    "read",
                    "npc",
                    "speech",
                    "read_chat",
                    "read_tells",
                    "focus",
                    "manage_ai",
                    "allow_llm",
                    "approve_drafts",
                ],
            ),
            "test-reader" => ("dm:reader", vec!["read", "approve_drafts"]),
            "test-admin" => ("dm:admin", vec!["read", "admin"]),
            "test-chat" => ("dm:chat", vec!["read", "read_chat"]),
            "test-agent-credential-for-local-fixtures" => ("ai:test", vec!["read"]),
            _ => return None,
        };
        Some(Principal {
            id: id.into(),
            name: "Test".into(),
            groups: groups.into_iter().map(str::to_owned).collect(),
        })
    }
}
async fn server(config: Config) -> (String, CoreHandle, tokio::task::JoinHandle<()>) {
    server_store(config, Arc::new(LiveStore::default())).await
}
async fn server_store(
    config: Config,
    store: Arc<LiveStore>,
) -> (String, CoreHandle, tokio::task::JoinHandle<()>) {
    let core = CoreHandle::new(store).await.unwrap();
    let app = mg_gateway::router(core.clone(), Arc::new(Auth), Arc::new(Policy::default()), config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, core, task)
}
fn request(url: &str) -> tokio_tungstenite::tungstenite::http::Request<()> {
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert("authorization", "Bearer test-credential".parse().unwrap());
    request.headers_mut().insert("sec-websocket-protocol", "moonglow-gw.1".parse().unwrap());
    request
}
fn hello() -> Value {
    json!({"t":"hello","proto":1,"mirror":mg_live::MIRROR_VERSION,"client":"test","watch":[]})
}
#[tokio::test]
async fn authenticated_handshake_and_version_refusal() {
    let (url, _, task) = server(Config::default()).await;
    let (mut ws, response) = connect_async(request(&url)).await.unwrap();
    assert_eq!(response.headers()["sec-websocket-protocol"], "moonglow-gw.1");
    ws.send(Message::Text(hello().to_string().into())).await.unwrap();
    let welcome: Value =
        serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(welcome["t"], "welcome");
    assert_eq!(welcome["user"]["id"], "dm:test");
    assert_eq!(welcome["resumed"], false);
    ws.close(None).await.unwrap();
    let error = connect_async(request(&format!("{url}?proto=999&mirror=1"))).await.unwrap_err();
    match error {
        tokio_tungstenite::tungstenite::Error::Http(r) => assert_eq!(r.status(), 426),
        other => panic!("{other}"),
    }
    let (mut ws, _) = connect_async(request(&url)).await.unwrap();
    let mut h = hello();
    h["mirror"] = json!(999);
    ws.send(Message::Text(h.to_string().into())).await.unwrap();
    let bye: Value =
        serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(bye["code"], "upgrade");
    task.abort();
}
#[tokio::test]
async fn origin_missing_credentials_and_ai_principals_never_upgrade() {
    let (url, _, task) = server(Config::default()).await;
    for (mode, expected) in [("origin", 403), ("missing", 401), ("agent", 401)] {
        let mut r = request(&url);
        if mode == "origin" {
            r.headers_mut().insert("origin", "https://example.invalid".parse().unwrap());
        } else if mode == "missing" {
            r.headers_mut().remove("authorization");
        } else {
            r.headers_mut().insert(
                "authorization",
                "Bearer test-agent-credential-for-local-fixtures".parse().unwrap(),
            );
        }
        match connect_async(r).await.unwrap_err() {
            tokio_tungstenite::tungstenite::Error::Http(r) => assert_eq!(r.status(), expected),
            other => panic!("{other}"),
        }
    }
    task.abort();
}
#[test]
fn all_external_scopes_are_restricted_and_names_are_validated() {
    use mg_live::access::DeliveryMode;
    for id in ["default", "local", "dm:default", "ai:local", "dm:", "root:one"] {
        assert!(
            Principal { id: id.into(), name: "name".into(), groups: vec![] }
                .scope(DeliveryMode::Auto)
                .is_err()
        );
    }
    let p = Principal {
        id: "dm:admin".into(),
        name: "local".into(),
        groups: vec!["admin".into(), "read".into()],
    };
    let scope = p.scope(DeliveryMode::Auto).unwrap();
    assert!(!scope.unrestricted());
    assert_eq!(scope.name(), "dm:admin");
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}
async fn receive(ws: &mut Ws) -> Value {
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(3), ws.next())
            .await
            .expect("frame deadline")
            .expect("connection closed")
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(&text).unwrap();
        }
    }
}
async fn connect(
    url: &str,
    watch: Vec<&str>,
    resume: Option<mg_live::gateway::Resume>,
    token: &str,
) -> (Ws, Value) {
    let mut r = request(url);
    r.headers_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = connect_async(r).await.unwrap();
    let mut h = hello();
    h["watch"] = json!(watch);
    if let Some(resume) = resume {
        h["resume"] = json!(resume);
    }
    ws.send(Message::Text(h.to_string().into())).await.unwrap();
    let welcome = receive(&mut ws).await;
    assert_eq!(welcome["t"], "welcome", "{welcome}");
    (ws, welcome)
}
async fn call(ws: &mut Ws, id: u32, op: &str, a: Value) -> Value {
    ws.send(Message::Text(json!({"t":"req","id":id,"op":op,"a":a}).to_string().into()))
        .await
        .unwrap();
    loop {
        let v = receive(ws).await;
        if v["t"] == "res" && v["id"] == id {
            return v;
        }
    }
}
fn report(seq: u64) -> Value {
    json!({"v":1,"boot":"boot","seq":seq,"areas":[{"area":"square","full":true,"cre":[{"id":"a1","name":"Guard","r":"guard","hp":[5,9]}]}]})
}
async fn feed(core: &CoreHandle, report: Value) -> Value {
    core.with_store(move |s| s.sync(&serde_json::to_vec(&report).unwrap(), now()).unwrap())
        .await
        .unwrap()
}
#[tokio::test]
async fn commands_are_scoped_attributed_and_results_are_acknowledged() {
    let (url, core, task) = server(Config::default()).await;
    feed(&core, report(1)).await;
    let (mut reader, w) = connect(&url, vec!["square"], None, "test-reader").await;
    let command = |prefix: &str, n: u32| json!({"id":format!("{prefix}1234abcd.{n}"),"boot":"boot","ttl_ms":30000,"op":"stop_patrol","area":"square","target":"a1"});
    let r = call(
        &mut reader,
        1,
        "cmds",
        json!({"commands":[command(w["id_prefix"].as_str().unwrap(),1)]}),
    )
    .await;
    assert_eq!(r["r"]["rejected"][0]["code"], "forbidden");
    reader.close(None).await.unwrap();
    let (mut ws, w) = connect(&url, vec!["square"], None, "test-credential").await;
    let c = command(w["id_prefix"].as_str().unwrap(), 1);
    let id = c["id"].as_str().unwrap().to_owned();
    let mut forged = c.clone();
    forged["id"] = json!("another.1234abcd.1");
    assert_eq!(
        call(&mut ws, 1, "cmds", json!({"commands":[forged]})).await["r"]["rejected"][0]["code"],
        "invalid"
    );
    assert_eq!(
        call(&mut ws, 2, "cmds", json!({"commands":[c]})).await["r"]["accepted"],
        json!([id])
    );
    let reply = feed(&core, report(2)).await;
    assert_eq!(reply["commands"][0]["by"], "dm:test", "{reply}");
    let mut done = report(3);
    done["results"] = json!([{"id":id,"ok":true,"by":"dm:forged"}]);
    feed(&core, done).await;
    let row = loop {
        let v = receive(&mut ws).await;
        if v["topic"] == "result" && v["body"]["state"] == "final" {
            break v["body"].clone();
        }
    };
    assert_eq!(row["result"]["by"], "dm:test");
    ws.send(Message::Text(
        json!({"t":"ack","results":[[id,row["rev"]]],"event_n":0}).to_string().into(),
    ))
    .await
    .unwrap();
    call(&mut ws, 3, "watch.set", json!({"areas":["square"]})).await;
    ws.close(None).await.unwrap();
    let resume = mg_live::gateway::Resume {
        gw_epoch: w["gw_epoch"].as_str().unwrap().into(),
        boot: "boot".into(),
        event_n: 0,
        areas: Default::default(),
    };
    let (mut ws, w) = connect(&url, vec!["square"], Some(resume), "test-credential").await;
    assert_eq!(w["resumed"], true);
    tokio::time::sleep(std::time::Duration::from_millis(320)).await;
    ws.send(Message::Text(
        json!({"t":"req","id":10,"op":"watch.set","a":{"areas":["square"]}}).to_string().into(),
    ))
    .await
    .unwrap();
    loop {
        let v = receive(&mut ws).await;
        assert_ne!(v["topic"], "result", "acked final was replayed");
        if v["t"] == "res" && v["id"] == 10 {
            break;
        }
    }
    ws.close(None).await.unwrap();
    core.shutdown().await.unwrap();
    task.abort();
}
#[tokio::test]
async fn queued_overflow_and_shutdown_send_explicit_byes() {
    let (url, core, task) = server(Config { queue_frames: 1, ..Config::default() }).await;
    feed(&core, report(1)).await;
    let (mut ws, w) = connect(&url, vec!["square"], None, "test-credential").await;
    assert_eq!(w["t"], "welcome");
    let bye = loop {
        let v = receive(&mut ws).await;
        if v["t"] == "bye" {
            break v;
        }
    };
    assert_eq!(bye["code"], "overloaded");
    core.shutdown().await.unwrap();
    task.abort();
    let (url, core, task) = server(Config::default()).await;
    let (mut ws, _) = connect(&url, vec![], None, "test-credential").await;
    core.request_shutdown();
    let bye = loop {
        let v = receive(&mut ws).await;
        if v["t"] == "bye" {
            break v;
        }
    };
    assert_eq!(bye["code"], "shutdown");
    task.abort();
}
struct Mirror {
    store: LiveStore,
    resume: mg_live::gateway::Resume,
}
impl Mirror {
    fn new(w: &Value) -> Self {
        Self {
            store: LiveStore::new_mirror(),
            resume: mg_live::gateway::Resume {
                gw_epoch: w["gw_epoch"].as_str().unwrap().into(),
                ..Default::default()
            },
        }
    }
    fn apply(&mut self, v: Value) {
        if v["t"] != "push" {
            return;
        }
        let f: mg_live::gateway_frames::ServerFrame = serde_json::from_value(v).unwrap();
        let mg_live::gateway_frames::ServerFrame::Push(push) = f else { unreachable!() };
        match &push {
            mg_live::gateway::Push::Area(a) => {
                self.resume.areas.insert(a.area.clone(), a.rev);
            }
            mg_live::gateway::Push::AreaGone { area, .. } => {
                self.resume.areas.remove(area);
            }
            mg_live::gateway::Push::Boot { .. } => self.resume.areas.clear(),
            _ => {}
        }
        push.apply(&self.store, now()).unwrap();
        self.resume.boot = self.store.boot();
        self.resume.event_n = self.store.event_cursor();
    }
    async fn barrier(&mut self, ws: &mut Ws, id: u32) {
        tokio::time::sleep(std::time::Duration::from_millis(320)).await;
        ws.send(Message::Text(
            json!({"t":"req","id":id,"op":"watch.set","a":{"areas":["square"]}}).to_string().into(),
        ))
        .await
        .unwrap();
        loop {
            let v = receive(ws).await;
            if v["t"] == "res" && v["id"] == id {
                break;
            }
            self.apply(v);
        }
    }
    async fn equal(&self, core: &CoreHandle) {
        let (mut a, find, locate, events) = core
            .with_store(|s| {
                (s.area("square"), s.find("", 200), s.locate("a1"), s.events(&Default::default()))
            })
            .await
            .unwrap();
        let mut b = self.store.area("square");
        if let Some(a) = &mut a {
            a.received_ms = 0;
        }
        if let Some(b) = &mut b {
            b.received_ms = 0;
        }
        assert_eq!(a, b);
        assert_eq!(find, self.store.find("", 200));
        assert_eq!(locate, self.store.locate("a1"));
        assert_eq!(events, self.store.events(&Default::default()));
    }
}
#[tokio::test]
async fn recorded_reports_mirror_over_real_socket_and_resume_after_drop() {
    let (url, core, task) = server(Config::default()).await;
    let (mut ws, w) = connect(&url, vec!["square"], None, "test-credential").await;
    let mut mirror = Mirror::new(&w);
    let reports: Vec<Value> =
        serde_json::from_str(include_str!("../../mg-live/tests/fixtures/mirror-reports.json"))
            .unwrap();
    for (i, r) in reports.into_iter().enumerate() {
        feed(&core, r).await;
        mirror.barrier(&mut ws, i as u32).await;
        mirror.equal(&core).await;
    }
    let prefix = w["id_prefix"].clone();
    ws.close(None).await.unwrap();
    let mut r = report(2);
    r["boot"] = json!(mirror.store.boot());
    feed(&core, r).await;
    let (mut ws, w) =
        connect(&url, vec!["square"], Some(mirror.resume.clone()), "test-credential").await;
    assert_eq!(w["resumed"], true);
    assert_eq!(w["id_prefix"], prefix);
    mirror.barrier(&mut ws, 100).await;
    mirror.equal(&core).await;
    ws.close(None).await.unwrap();
    core.shutdown().await.unwrap();
    task.abort();
}
#[tokio::test]
async fn oversized_input_closes_with_message_too_big() {
    let (url, core, task) = server(Config::default()).await;
    let (mut ws, _) = connect(&url, vec![], None, "test-credential").await;
    ws.send(Message::Text(
        json!({"t":"req","id":1,"op":"unknown","a":{"text":"x".repeat(65_536)}}).to_string().into(),
    ))
    .await
    .unwrap();
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(3), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Close(Some(frame)) = message {
            assert_eq!(u16::from(frame.code), 1009);
            break;
        }
    }
    core.shutdown().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn http_agent_route_matches_direct_route_over_loopback_tcp() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (url, core, task) = server(Config::default()).await;
    core.with_gateway(|g| {
        g.install_ai(
            mg_live::gateway::AiIdentity {
                id: "ai:test".into(),
                owner: "dm:test".into(),
                name: "Test".into(),
                groups: vec!["read".into()],
                expires_ms: u64::MAX,
                llm_allowed: true,
            },
            "test-agent-credential-for-local-fixtures",
        )
        .unwrap();
        g.set_llm_enabled(true);
    })
    .await
    .unwrap();
    let address = url.trim_start_matches("ws://").trim_end_matches("/ws/v1");
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream.write_all(b"GET /agent/v1/describe HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer test-agent-credential-for-local-fixtures\r\nConnection: close\r\n\r\n").await.unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(3), stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200"));
    let actual: Value = serde_json::from_str(body).unwrap();
    let expected = core
        .with_store(|s| {
            mg_live::api::route(
                "GET",
                "/v1/describe",
                &[],
                &[],
                &Auth
                    .authenticate("test-agent-credential-for-local-fixtures")
                    .unwrap()
                    .scope(mg_live::access::DeliveryMode::Drafts)
                    .unwrap(),
                s,
            )
        })
        .await
        .unwrap();
    assert_eq!(actual, expected.body);
    core.shutdown().await.unwrap();
    task.abort();
}
#[tokio::test]
async fn replacement_connection_closes_previous_and_connection_limit_refuses_upgrade() {
    let (url, core, task) = server(Config::default()).await;
    let (mut old, _) = connect(&url, vec![], None, "test-credential").await;
    let (mut new, _) = connect(&url, vec![], None, "test-credential").await;
    let bye = loop {
        let v = receive(&mut old).await;
        if v["t"] == "bye" {
            break v;
        }
    };
    assert_eq!(bye["code"], "replaced");
    new.close(None).await.unwrap();
    core.shutdown().await.unwrap();
    task.abort();
    let (url, core, task) = server(Config { connections: 0, ..Config::default() }).await;
    match connect_async(request(&url)).await.unwrap_err() {
        tokio_tungstenite::tungstenite::Error::Http(r) => assert_eq!(r.status(), 429),
        other => panic!("{other}"),
    }
    core.shutdown().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn gateway_restart_changes_epoch_and_resets_unsent_commands_to_unknown() {
    let store = Arc::new(LiveStore::default());
    let (url, core, task) = server_store(Config::default(), store.clone()).await;
    feed(&core, report(1)).await;
    let (mut ws, w) = connect(&url, vec!["square"], None, "test-credential").await;
    let mut mirror = Mirror::new(&w);
    mirror.store.set_id_prefix(w["id_prefix"].as_str().unwrap()).unwrap();
    mirror.barrier(&mut ws, 1).await;
    mirror.equal(&core).await;
    let c =
        serde_json::from_value(json!({"op":"stop_patrol","area":"square","target":"a1"})).unwrap();
    let id = mirror.store.enqueue(c, now()).unwrap();
    ws.close(None).await.unwrap();
    core.shutdown().await.unwrap();
    task.abort();
    let (url, core, task) = server_store(Config::default(), store).await;
    let (mut ws, w) =
        connect(&url, vec!["square"], Some(mirror.resume.clone()), "test-credential").await;
    assert_ne!(w["gw_epoch"], mirror.resume.gw_epoch);
    assert_eq!(w["resumed"], false);
    mirror.store.mirror_reset("gateway restart").unwrap();
    mirror.resume.areas.clear();
    mirror.resume.gw_epoch = w["gw_epoch"].as_str().unwrap().into();
    assert_eq!(mirror.store.result(&id).unwrap().code, Some(mg_live::store::ErrorCode::Unknown));
    assert!(mirror.store.mirror_commands(now()).unwrap().is_empty());
    mirror.barrier(&mut ws, 2).await;
    mirror.equal(&core).await;
    assert!(core.with_store(move |s| s.result(&id)).await.unwrap().is_none());
    ws.close(None).await.unwrap();
    core.shutdown().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn host_can_publish_authorized_auxiliary_pushes() {
    let (url, core, task) =
        server(Config { features: vec!["notice".into()], ..Config::default() }).await;
    let (mut ws, w) = connect(&url, vec![], None, "test-credential").await;
    assert_eq!(w["features"], json!(["notice"]));
    core.publish(
        "dm:test".into(),
        mg_live::gateway::Push::Notice { level: "info".into(), text: "Ready".into() },
    )
    .await
    .unwrap();
    let notice = loop {
        let v = receive(&mut ws).await;
        if v["topic"] == "notice" {
            break v;
        }
    };
    assert_eq!(notice["body"]["text"], "Ready");
    ws.close(None).await.unwrap();
    core.shutdown().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn ping_and_idle_deadlines_close_a_silent_client() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn frame(stream: &mut tokio::net::TcpStream) -> (u8, Vec<u8>) {
        let mut head = [0; 2];
        stream.read_exact(&mut head).await.unwrap();
        assert_ne!(head[0] & 0x80, 0);
        assert_eq!(head[1] & 0x80, 0);
        let size = match head[1] & 0x7f {
            126 => stream.read_u16().await.unwrap() as u64,
            127 => stream.read_u64().await.unwrap(),
            n => u64::from(n),
        };
        assert!(size <= mg_live::gateway_frames::SERVER_FRAME_LIMIT as u64);
        let mut bytes = vec![0; size as usize];
        stream.read_exact(&mut bytes).await.unwrap();
        (head[0] & 0xf, bytes)
    }
    let (url, core, task) = server(Config::default()).await;
    // Use tungstenite for the upgrade, then raw frames BEFORE hello. No application
    // bytes can be buffered yet, and reading Ping must not generate an automatic Pong.
    let (mut ws, _) = connect_async(request(&url)).await.unwrap();
    let tokio_tungstenite::MaybeTlsStream::Plain(stream) = ws.get_mut() else {
        panic!("loopback TCP expected")
    };
    let hello = serde_json::to_vec(&hello()).unwrap();
    assert!(hello.len() < 126);
    let mask = [1, 2, 3, 4];
    let mut packet = vec![0x81, 0x80 | hello.len() as u8];
    packet.extend(mask);
    packet.extend(hello.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    stream.write_all(&packet).await.unwrap();
    let started = std::time::Instant::now();
    tokio::time::timeout(std::time::Duration::from_secs(50), async {
        let (kind, welcome) = frame(stream).await;
        assert_eq!(kind, 1);
        assert_eq!(serde_json::from_slice::<Value>(&welcome).unwrap()["t"], "welcome");
        let mut pings = 0;
        loop {
            let (kind, bytes) = frame(stream).await;
            match kind {
                9 => {
                    pings += 1;
                    assert!(started.elapsed() >= std::time::Duration::from_secs(14));
                }
                8 => {
                    assert_eq!(&bytes[..2], &1001u16.to_be_bytes());
                    assert_eq!(&bytes[2..], b"idle");
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(pings, 2);
        assert!(started.elapsed() >= std::time::Duration::from_secs(44));
    })
    .await
    .unwrap();
    core.shutdown().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn presence_is_authoritative_across_focus_drop_and_reconnect() {
    async fn snapshot(ws: &mut Ws, count: usize, minimum: u64) -> Value {
        loop {
            let v = receive(ws).await;
            if v["topic"] == "presence"
                && v["body"]["rev"].as_u64().unwrap() > minimum
                && v["body"]["actors"].as_array().unwrap().len() == count
            {
                return v["body"].clone();
            }
        }
    }
    let (url, core, task) = server(Config::default()).await;
    let (mut one, _) = connect(&url, vec![], None, "test-credential").await;
    let (mut two, _) = connect(&url, vec![], None, "test-reader").await;
    let initial = snapshot(&mut two, 2, 0).await;
    assert!(
        initial["actors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "dm:test" && a["name"] == "Test")
    );
    assert_eq!(
        call(&mut one, 1, "focus.set", json!({"objects":[{"area":"square","id":"a1"}]})).await["ok"],
        true
    );
    let focused = snapshot(&mut two, 2, initial["rev"].as_u64().unwrap()).await;
    assert_eq!(focused["focus"]["a1"], json!(["dm:test"]));
    one.close(None).await.unwrap();
    let gone = snapshot(&mut two, 1, focused["rev"].as_u64().unwrap()).await;
    let (_back, _) = connect(&url, vec![], None, "test-credential").await;
    let back = snapshot(&mut two, 2, gone["rev"].as_u64().unwrap()).await;
    assert_eq!(back["focus"], json!({}));
    core.shutdown().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn managed_ai_requests_enforce_ownership_consent_and_revoke_both_transports() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let (url, core, task) = server(Config::default()).await;
    feed(&core, report(1)).await;
    let (mut dm, _) = connect(&url, vec![], None, "test-credential").await;
    let created = call(&mut dm, 1, "ai.create", json!({"name":"Helper"})).await;
    assert_eq!(created["ok"], true, "{created}");
    let token = created["r"]["token"].as_str().unwrap().to_owned();
    let id = created["r"]["ai"]["id"].as_str().unwrap().to_owned();
    let app = mg_gateway::router(
        core.clone(),
        Arc::new(Auth),
        Arc::new(Policy::default()),
        Config::default(),
    );
    async fn http(app: &axum::Router, token: &str) -> u16 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/agent/v1/describe")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let code = response.status().as_u16();
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        code
    }
    assert_eq!(http(&app, &token).await, 401);
    core.with_gateway(|g| g.set_llm_enabled(true)).await.unwrap();
    assert_eq!(http(&app, &token).await, 401);
    assert_eq!(call(&mut dm, 2, "llm.allow", json!({"on":true})).await["ok"], true);
    assert_eq!(http(&app, &token).await, 200);
    let (mut ai, welcome) = connect(&url, vec![], None, &token).await;
    assert_eq!(welcome["user"]["id"], id);
    assert_eq!(call(&mut dm, 4, "ai.update", json!({"id":id,"name":"Updated"})).await["ok"], true);
    assert_eq!(call(&mut dm, 5, "ai.list", json!({})).await["r"][0]["name"], "Updated");
    assert_eq!(call(&mut dm, 6, "ai.revoke", json!({"id":id})).await["ok"], true);
    assert_eq!(http(&app, &token).await, 401);
    let mut r = request(&url);
    r.headers_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
    match connect_async(r).await.unwrap_err() {
        tokio_tungstenite::tungstenite::Error::Http(r) => assert_eq!(r.status(), 401),
        e => panic!("{e}"),
    }
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(Ok(frame)) = ai.next().await {
            if matches!(frame, Message::Close(_)) {
                break;
            }
        }
    })
    .await
    .expect("revoked AI session closes");
    core.shutdown().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn book_requests_conflict_and_invalidate_other_sessions() {
    let root = std::env::temp_dir()
        .join(format!("gateway-book-{}", mg_live::service::generate_token().unwrap()));
    let (url, core, task) = server(Config::default()).await;
    let data = root.clone();
    core.with_store(move |s| {
        s.assistant_context(data.clone(), data.join("fixture.mod"), "fixture".into()).unwrap()
    })
    .await
    .unwrap();
    let (mut one, _) = connect(&url, vec![], None, "test-credential").await;
    let (mut two, _) = connect(&url, vec![], None, "test-reader").await;
    let put = json!({"kind":"gm","key":"","value":{"house_rules":"Shared"},"if_rev":0});
    assert_eq!(call(&mut one, 1, "book.put", put.clone()).await["r"]["rev"], 1);
    loop {
        let v = receive(&mut two).await;
        if v["topic"] == "book" {
            assert_eq!(v["body"]["rev"], 1);
            break;
        }
    }
    assert_eq!(call(&mut two, 1, "book.put", put).await["err"]["code"], "conflict");
    assert_eq!(
        call(&mut two, 2, "call", json!({"op":"gm_card"})).await["r"]["card"]["house_rules"],
        "Shared"
    );
    core.shutdown().await.unwrap();
    task.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn only_the_owning_dm_can_decide_an_ai_draft() {
    let (url, core, task) = server(Config::default()).await;
    feed(&core, report(1)).await;
    let (mut dm, _) = connect(&url, vec![], None, "test-credential").await;
    let (mut other, _) = connect(&url, vec![], None, "test-reader").await;
    let created = call(&mut dm, 1, "ai.create", json!({"name":"Speaker"})).await;
    let token = created["r"]["token"].as_str().unwrap();
    core.with_gateway(|g| g.set_llm_enabled(true)).await.unwrap();
    call(&mut dm, 2, "llm.allow", json!({"on":true})).await;
    let (mut ai, welcome) = connect(&url, vec![], None, token).await;
    let command_id = format!("{}1234abcd.1", welcome["id_prefix"].as_str().unwrap());
    let result=call(&mut ai,1,"cmds",json!({"commands":[{"id":command_id,"boot":"boot","ttl_ms":30000,"area":"square","op":"npc_say","target":"a1","text":"Hello"}]})).await;
    assert_eq!(result["r"]["accepted"], json!([command_id]), "{result}");
    let forbidden = call(&mut other, 1, "draft.decide", json!({"id":command_id,"text":null})).await;
    assert_eq!(forbidden["err"]["code"], "forbidden", "{forbidden}");
    let accepted =
        call(&mut dm, 3, "draft.decide", json!({"id":command_id,"text":"Approved"})).await;
    assert_eq!(accepted["ok"], true, "{accepted}");

    assert!(core.with_store(|s| s.drafts().is_empty()).await.unwrap());
    let second = format!("{}1234abcd.2", welcome["id_prefix"].as_str().unwrap());
    let reply=call(&mut ai,2,"cmds",json!({"commands":[{"id":second,"boot":"boot","ttl_ms":30000,"area":"square","op":"npc_say","target":"a1","text":"Another"}]})).await;
    assert_eq!(reply["r"]["accepted"], json!([second]));
    let (mut admin, _) = connect(&url, vec![], None, "test-admin").await;
    assert_eq!(
        call(&mut admin, 1, "draft.decide", json!({"id":second,"text":null})).await["ok"],
        true
    );
    assert!(core.with_store(|s| s.drafts().is_empty()).await.unwrap());
    core.shutdown().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn note_reads_and_filtered_event_pages_cross_the_websocket() {
    let root = std::env::temp_dir()
        .join(format!("gateway-read-{}", mg_live::service::generate_token().unwrap()));
    let (url, core, task) = server(Config::default()).await;
    let mut state = report(1);
    state["events"] = json!([
        {"seq":1,"kind":"chat","area":"square","channel":1,"text":"one"},
        {"seq":2,"kind":"chat","area":"square","channel":4,"text":"private"},
        {"seq":3,"kind":"chat","area":"square","channel":1,"text":"two"}
    ]);
    feed(&core, state).await;
    let data = root.clone();
    core.with_store(move |s| {
        s.assistant_context(data.clone(), data.join("fixture.mod"), "fixture".into()).unwrap();
        s.save_object_note("a1", "Remote note").unwrap();
    })
    .await
    .unwrap();
    let (mut ws, _) = connect(&url, vec![], None, "test-chat").await;
    let notes =
        call(&mut ws, 1, "call", json!({"op":"area_notes","input":{"area":"square"}})).await;
    assert_eq!(notes["r"]["notes"]["a1"], "Remote note");
    assert_eq!(notes["r"]["revs"]["a1"], 1);
    let first = call(&mut ws, 2, "events.since", json!({"n":0,"limit":1,"kinds":["chat"]})).await;
    let second = call(
        &mut ws,
        3,
        "events.since",
        json!({"n":first["r"]["through"],"limit":1,"kinds":["chat"]}),
    )
    .await;
    assert_eq!(first["r"]["rows"][0]["data"]["text"], "one");
    assert_eq!(second["r"]["rows"][0]["data"]["text"], "two");
    assert_eq!(second["r"]["through"], core.with_store(|s| s.event_cursor()).await.unwrap());
    core.shutdown().await.unwrap();
    task.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[derive(Debug)]
struct LowCommandLimit;
impl mg_gateway::PolicyResolver for LowCommandLimit {
    fn policy(&self) -> Policy {
        Policy::default()
    }
    fn command_limit(&self, _: &Principal) -> u32 {
        1
    }
}
#[tokio::test]
async fn websocket_uses_the_hosts_lower_authoritative_command_limit() {
    let core = CoreHandle::new(Arc::new(LiveStore::default())).await.unwrap();
    feed(&core, report(1)).await;
    let app = mg_gateway::router(
        core.clone(),
        Arc::new(Auth),
        Arc::new(LowCommandLimit),
        Config::default(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut ws, w) = connect(&url, vec![], None, "test-credential").await;
    let prefix = w["id_prefix"].as_str().unwrap();
    for n in 1..=2 {
        let r=call(&mut ws,n,"cmds",json!({"commands":[{"id":format!("{prefix}1234abcd.{n}"),"boot":"boot","op":"stop_patrol","area":"square","target":"a1"}]})).await;
        if n == 1 {
            assert_eq!(r["r"]["accepted"].as_array().unwrap().len(), 1, "{r}");
        } else {
            assert_eq!(r["r"]["rejected"][0]["code"], "rate_limited", "{r}");
        }
    }
    core.with_gateway(|g| g.set_llm_enabled(true)).await.unwrap();
    let created = call(&mut ws, 3, "ai.create", json!({"name":"Writer"})).await;
    call(&mut ws, 4, "llm.allow", json!({"on":true})).await;
    let (mut ai, w) = connect(&url, vec![], None, created["r"]["token"].as_str().unwrap()).await;
    let id = format!("{}1234abcd.1", w["id_prefix"].as_str().unwrap());
    let r=call(&mut ai,1,"cmds",json!({"commands":[{"id":id,"boot":"boot","op":"npc_say","area":"square","target":"a1","text":"draft"}]})).await;
    assert_eq!(r["r"]["accepted"], json!([id]), "{r}");
    let r = call(&mut ws, 5, "draft.decide", json!({"id":id,"text":"approved"})).await;
    assert_eq!(r["err"]["code"], "rate_limited", "{r}");
    task.abort();
}
