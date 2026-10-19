use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use futures_util::{SinkExt, StreamExt};
use mg_gateway::{Authenticator, Config, CoreHandle, Principal, files::Publisher};
use mg_live::store::{LiveStore, Policy};
use mg_module::{Module, ModuleLocation, nwsync};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tower::ServiceExt;
#[derive(Debug)]
struct Auth;
impl Authenticator for Auth {
    fn authenticate(&self, token: &str) -> Option<Principal> {
        let (id, groups) = match token {
            "fixture-dm" => ("dm:test", vec!["read", "manage_ai", "allow_llm"]),
            "fixture-blind" => ("dm:blind", vec![]),
            "fixture-player" => ("player:test", vec!["read"]),
            _ => return None,
        };
        Some(Principal {
            id: id.into(),
            name: "Fixture".into(),
            groups: groups.into_iter().map(str::to_owned).collect(),
        })
    }
    fn principal(&self, id: &str) -> Option<Principal> {
        if id == "dm:test" { self.authenticate("fixture-dm") } else { None }
    }
}
fn fixture(
    root: &std::path::Path,
) -> (std::path::PathBuf, Arc<Publisher>, mg_live::resource_files::ResourceGeneration) {
    fixture_with(root, Publisher::new(root.join("published")))
}
fn fixture_with(
    root: &std::path::Path,
    publisher: Publisher,
) -> (std::path::PathBuf, Arc<Publisher>, mg_live::resource_files::ResourceGeneration) {
    let path = root.join("fixture.mod");
    let mut module = Module::new();
    module.set_info(&mg_gff::Gff::new(*b"IFO ")).unwrap();
    module.set(
        mg_resman::ResKey::parse("secret", mg_core::ResType::NCS).unwrap(),
        &b"private bytecode"[..],
    );
    module.save_as(&ModuleLocation::Archive(path.clone())).unwrap();
    let publisher = Arc::new(publisher);
    let hak = nwsync::manifest(&[]);
    let hash = mg_core::sha1::hex(&mg_core::sha1::sha1(&hak));
    let g =
        publisher.refresh(&path, "boot", "https://assets.example.invalid/", &hash, &hak).unwrap();
    (path, publisher, g)
}
#[tokio::test]
async fn files_require_current_dm_and_only_serve_published_module_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let (_, publisher, g) = fixture(dir.path());
    let store = Arc::new(LiveStore::default());
    store.sync(br#"{"v":1,"boot":"boot","areas":[]}"#, 1).unwrap();
    let core = CoreHandle::new(store).await.unwrap();
    let ai_token = core
        .with_gateway(|g| {
            let owner = Auth.authenticate("fixture-dm").unwrap();
            g.refresh_principal(owner.clone()).unwrap();
            g.connect(
                mg_live::gateway::SessionConfig {
                    id: owner.id.clone(),
                    id_prefix: "fixture.".into(),
                    groups: owner.groups,
                    watch_limit: 16,
                },
                None,
                &[],
                mg_live::now_ms(),
            )
            .unwrap();
            g.set_llm_enabled(true);
            let created = g
                .handle(
                    &owner.id,
                    mg_live::gateway_frames::Request {
                        id: 1,
                        op: "ai.create".into(),
                        a: json!({"name":"Fixture","groups":["read"]}),
                    },
                    mg_live::now_ms(),
                )
                .result
                .unwrap();
            g.handle(
                &owner.id,
                mg_live::gateway_frames::Request {
                    id: 2,
                    op: "llm.allow".into(),
                    a: json!({"on":true}),
                },
                mg_live::now_ms(),
            )
            .result
            .unwrap();
            created["token"].as_str().unwrap().to_owned()
        })
        .await
        .unwrap();
    let app = mg_gateway::router(
        core.clone(),
        Arc::new(Auth),
        Arc::new(Policy::default()),
        Config { publisher: Some(publisher), ..Default::default() },
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/agent/v1/describe")
                .header("authorization", format!("Bearer {ai_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/files/v1/generation")
                .header("authorization", format!("Bearer {ai_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    for (token, expected) in [
        (None, 401),
        (Some("fixture-blind"), 403),
        (Some("fixture-player"), 401),
        (Some("mgai_invalid"), 401),
        (Some("fixture-dm"), 200),
    ] {
        let mut request = Request::builder().uri("/files/v1/generation");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        assert_eq!(
            app.clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
                .as_u16(),
            expected
        );
    }
    for (path, status) in [
        (format!("manifests/{}", g.module_manifest), 200),
        (format!("manifests/{}", g.haks_manifest), 404),
        ("latest".into(), 404),
        ("../settings".into(), 404),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/files/v1/{path}"))
                    .header("authorization", "Bearer fixture-dm")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        if status == 200 {
            let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
            let rows = nwsync::read_manifest(&bytes).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].0, Module::info_key());
        }
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/files/v1/generation")
                .header("authorization", "Bearer fixture-dm")
                .header("origin", "https://example.invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    core.shutdown().await.unwrap();
}
#[test]
fn changed_module_publishes_new_version_and_boot_is_bound() {
    let dir = tempfile::tempdir().unwrap();
    let (path, p, g) = fixture(dir.path());
    let hak = nwsync::manifest(&[]);
    let hash = mg_core::sha1::hex(&mg_core::sha1::sha1(&hak));
    assert_eq!(p.refresh(&path, "boot", &g.haks_url, &hash, &hak).unwrap(), g);
    let mut m = Module::open(&path).unwrap();
    m.set(mg_resman::ResKey::parse("authored", mg_core::ResType::TXT).unwrap(), &b"changed"[..]);
    m.save().unwrap();
    let changed = p.refresh(&path, "boot", &g.haks_url, &hash, &hak).unwrap();
    assert_ne!(changed.id(), g.id());
    assert!(p.current("other-boot").is_none());
    let restarted = p.refresh(&path, "other-boot", &g.haks_url, &hash, &hak).unwrap();
    assert_ne!(restarted.id(), changed.id());
    p.disable();
    assert!(p.current("other-boot").is_none());
}
#[tokio::test]
async fn welcome_and_files_request_report_the_same_generation() {
    let dir = tempfile::tempdir().unwrap();
    let (_, p, g) = fixture(dir.path());
    let store = Arc::new(LiveStore::default());
    store.sync(br#"{"v":1,"boot":"boot","areas":[]}"#, 1).unwrap();
    let core = CoreHandle::new(store).await.unwrap();
    let app = mg_gateway::router(
        core.clone(),
        Arc::new(Auth),
        Arc::new(Policy::default()),
        Config { publisher: Some(p), ..Default::default() },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut req = url.into_client_request().unwrap();
    req.headers_mut().insert("authorization", "Bearer fixture-dm".parse().unwrap());
    req.headers_mut().insert("sec-websocket-protocol", "moonglow-gw.1".parse().unwrap());
    let (mut ws, _) = connect_async(req).await.unwrap();
    ws.send(Message::Text(json!({"t":"hello","proto":1,"mirror":mg_live::MIRROR_VERSION,"client":"fixture","watch":[]}).to_string().into())).await.unwrap();
    let welcome: Value =
        serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(welcome["nwsync"]["generation"], serde_json::to_value(&g).unwrap());
    ws.send(Message::Text(json!({"t":"req","id":1,"op":"files","a":{}}).to_string().into()))
        .await
        .unwrap();
    loop {
        let frame: Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        if frame["t"] == "res" {
            assert_eq!(frame["r"], serde_json::to_value(&g).unwrap());
            break;
        }
    }
    ws.close(None).await.unwrap();
    task.abort();
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn download_permits_live_until_the_response_body_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (_, publisher, g) = fixture(dir.path());
    let core = CoreHandle::new(Arc::new(LiveStore::default())).await.unwrap();
    let app = mg_gateway::router(
        core,
        Arc::new(Auth),
        Arc::new(Policy::default()),
        Config { publisher: Some(publisher), ..Default::default() },
    );
    let request = || {
        Request::builder()
            .uri(format!("/files/v1/manifests/{}", g.module_manifest))
            .header("authorization", "Bearer fixture-dm")
            .body(Body::empty())
            .unwrap()
    };
    let mut held = Vec::new();
    for _ in 0..4 {
        let r = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(r.status(), 200);
        held.push(r);
    }
    assert_eq!(app.clone().oneshot(request()).await.unwrap().status(), 429);
    held.pop();
    let r = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(!to_bytes(r.into_body(), 1024 * 1024).await.unwrap().is_empty());
    assert_eq!(app.oneshot(request()).await.unwrap().status(), 200);
}

#[tokio::test]
async fn a_reader_that_stops_reading_loses_its_download_slot() {
    let dir = tempfile::tempdir().unwrap();
    let idle = std::time::Duration::from_millis(150);
    let (_, publisher, g) = fixture_with(
        dir.path(),
        Publisher::new(dir.path().join("published")).with_idle_timeout(idle),
    );
    let core = CoreHandle::new(Arc::new(LiveStore::default())).await.unwrap();
    let app = mg_gateway::router(
        core,
        Arc::new(Auth),
        Arc::new(Policy::default()),
        Config { publisher: Some(publisher), ..Default::default() },
    );
    let request = || {
        Request::builder()
            .uri(format!("/files/v1/manifests/{}", g.module_manifest))
            .header("authorization", "Bearer fixture-dm")
            .body(Body::empty())
            .unwrap()
    };
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(app.clone().oneshot(request()).await.unwrap());
    }
    assert_eq!(app.clone().oneshot(request()).await.unwrap().status(), 429);
    tokio::time::sleep(idle * 4).await;
    let fresh = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(fresh.status(), 200, "stalled readers no longer hold slots");
    assert!(!to_bytes(fresh.into_body(), 1024 * 1024).await.unwrap().is_empty());
    let stalled = held.pop().unwrap();
    assert!(to_bytes(stalled.into_body(), 1024 * 1024).await.is_err(), "the stalled body ends");
}
