use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use mg_gateway::{Authenticator, Config, CoreHandle, PolicyResolver, Principal};
use mg_live::{
    access::DeliveryMode,
    store::{LiveStore, Policy},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
#[derive(Debug)]
struct Auth;
impl Authenticator for Auth {
    fn principal(&self, id: &str) -> Option<Principal> {
        (id == "dm:owner").then(|| {
            let mut p = principal();
            p.id = id.into();
            p
        })
    }
    fn authenticate(&self, t: &str) -> Option<Principal> {
        (t == "test-agent-credential-for-local-fixtures").then(principal)
    }
}
fn principal() -> Principal {
    Principal {
        id: "ai:test".into(),
        name: "Test".into(),
        groups: vec!["read".into(), "npc".into()],
    }
}
#[derive(Debug)]
struct PolicyProvider;
impl PolicyResolver for PolicyProvider {
    fn policy(&self) -> Policy {
        Policy::default()
    }
    fn delivery(&self, _: &Principal) -> DeliveryMode {
        DeliveryMode::Auto
    }
    fn command_limit(&self, _: &Principal) -> u32 {
        1
    }
}
async fn app() -> (axum::Router, CoreHandle) {
    let core = CoreHandle::new(Arc::new(LiveStore::default())).await.unwrap();
    core.with_gateway(|g| {
        g.install_ai(
            mg_live::gateway::AiIdentity {
                id: "ai:test".into(),
                owner: "dm:owner".into(),
                name: "Test".into(),
                groups: vec!["read".into(), "npc".into(), "read_chat".into(), "read_tells".into()],
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
    core.with_store(|s| {
        s.sync(
            br#"{"v":1,"boot":"b","seq":1,"areas":[{"area":"a","full":true,"cre":[{"id":"1","hp":[3,10]}]}]}"#,
            1000,
        )
        .unwrap();
    })
    .await
    .unwrap();
    (
        mg_gateway::router(
            core.clone(),
            Arc::new(Auth),
            Arc::new(PolicyProvider),
            Config::default(),
        ),
        core,
    )
}
async fn exchange(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: &str,
    extra: Option<(&str, &str)>,
) -> (u16, Value, Option<String>) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", "Bearer test-agent-credential-for-local-fixtures");
    if let Some((k, v)) = extra {
        request = request.header(k, v);
    }
    let r = app.clone().oneshot(request.body(Body::from(body.to_owned())).unwrap()).await.unwrap();
    let status = r.status().as_u16();
    let tag = r.headers().get("etag").map(|v| v.to_str().unwrap().to_owned());
    (
        status,
        serde_json::from_slice(&to_bytes(r.into_body(), 16 * 1024 * 1024).await.unwrap()).unwrap(),
        tag,
    )
}
#[tokio::test]
async fn gateway_and_pure_route_have_identical_catalogue_errors_and_reads() {
    let (app, core) = app().await;
    let cases = [
        ("GET", "describe", "", None),
        ("GET", "openapi.json", "", None),
        ("GET", "ping", "", None),
        ("POST", "describe", "{}", None),
        ("GET", "missing", "", None),
        ("POST", "call/live_find", "{}", None),
        ("POST", "call/live_npc_move", "{", None),
        ("POST", "call/live_run_script", "{}", None),
        ("POST", "call/live_watch_areas", r#"{"areas":["a"]}"#, None),
        ("POST", "call/live_resync", "{}", None),
        ("POST", "call/live_find", "{}", Some(("If-Match", "stale"))),
        ("POST", "call/live_events", r#"{"wait_ms":30001}"#, None),
        ("POST", "call/live_wait", r#"{"id":"missing","timeout_ms":30001}"#, None),
    ];
    for (method, path, body, extra) in cases {
        let actual = exchange(&app, method, &format!("/agent/v1/{path}"), body, extra).await;
        let expected = core
            .with_store(move |s| {
                let headers = extra.map(|(k, v)| vec![(k.into(), v.into())]).unwrap_or_default();
                mg_live::api::route(
                    method,
                    &format!("/v1/{path}"),
                    &headers,
                    body.as_bytes(),
                    &principal().scope(DeliveryMode::Auto).unwrap(),
                    s,
                )
            })
            .await
            .unwrap();
        assert_eq!(actual, (expected.status, expected.body, expected.etag), "{method} {path}");
    }
}
#[tokio::test]
async fn agent_quotas_etag_and_authentication_have_legacy_bodies() {
    let (app, _) = app().await;
    let (_, _, tag) = exchange(&app, "GET", "/agent/v1/describe", "", None).await;
    assert!(tag.is_some());
    let body = r#"{"area":"a","target":"1","x":1,"y":2,"z":0}"#;
    assert_eq!(
        exchange(&app, "POST", "/agent/v1/call/live_npc_move", body, Some(("If-Match", "old")))
            .await
            .0,
        412
    );
    assert_eq!(exchange(&app, "POST", "/agent/v1/call/live_npc_move", body, None).await.0, 200);
    let (status, value, _) =
        exchange(&app, "POST", "/agent/v1/call/live_npc_move", body, None).await;
    assert_eq!(status, 429);
    assert_eq!(value["code"], "rate_limited");
    assert_eq!(value["error"], "Client command limit exceeded");
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/agent/v1/describe").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    assert_eq!(body, json!({"ok":false,"error":"Unauthorized"}));
    assert_eq!(
        exchange(
            &app,
            "GET",
            "/agent/v1/describe",
            "",
            Some(("Origin", "https://example.invalid"))
        )
        .await
        .0,
        403
    );
}
#[tokio::test]
async fn event_poll_releases_runtime_and_wakes_on_report() {
    let (app, core) = app().await;
    let poll = tokio::spawn(async move {
        exchange(
            &app,
            "POST",
            "/agent/v1/call/live_events",
            r#"{"wait_ms":1000,"since":0,"kinds":["healed"]}"#,
            None,
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    assert!(!poll.is_finished(), "poll should still be waiting");
    core.with_store(|s| {
        s.sync(
            br#"{"v":1,"boot":"b","seq":2,"areas":[{"area":"a","full":true,"cre":[{"id":"1","hp":[5,10]}]}]}"#,
            1100,
        )
        .unwrap();
    })
    .await
    .unwrap();
    let r = tokio::time::timeout(std::time::Duration::from_secs(2), poll).await.unwrap().unwrap();
    assert_eq!(r.0, 200, "{}", r.1);
    assert!(!r.1["result"]["events"].as_array().unwrap().is_empty(), "{}", r.1);
}

#[tokio::test]
async fn agent_chat_requires_a_separate_tell_capability_and_keeps_the_cursor() {
    #[derive(Debug)]
    struct ChatAuth {
        tells: bool,
    }
    impl Authenticator for ChatAuth {
        fn principal(&self, id: &str) -> Option<Principal> {
            (id == "dm:owner").then(|| {
                let mut p = self.authenticate("test-agent-credential-for-local-fixtures").unwrap();
                p.id = id.into();
                p
            })
        }
        fn authenticate(&self, t: &str) -> Option<Principal> {
            if t != "test-agent-credential-for-local-fixtures" {
                return None;
            }
            let mut p = principal();
            p.groups.push("read_chat".into());
            if self.tells {
                p.groups.push("read_tells".into());
            }
            Some(p)
        }
    }
    let (_, core) = app().await;
    let through = core
        .with_store(|s| {
            s.sync(
                &serde_json::to_vec(&json!({"v":1,"boot":"b","seq":2,"areas":[],"events":[
                    {"seq":1,"kind":"chat","channel":1,"text":"public"},
                    {"seq":2,"kind":"chat","channel":4,"text":"private"},
                    {"seq":3,"kind":"chat","channel":20,"text":"private"},
                    {"seq":4,"kind":"chat","channel":"tell","text":"private"}
                ]}))
                .unwrap(),
                1100,
            )
            .unwrap();
            s.event_cursor()
        })
        .await
        .unwrap();
    for tells in [false, true] {
        let app = mg_gateway::router(
            core.clone(),
            Arc::new(ChatAuth { tells }),
            Arc::new(PolicyProvider),
            Config::default(),
        );
        for path in ["/agent/v1/call/live_events", "/agent/v1/call//v1/call/live_events"] {
            let (status, body, _) = exchange(&app, "POST", path, r#"{"since":0}"#, None).await;
            assert_eq!(status, 200);
            let events = body["result"]["events"].as_array().unwrap();
            assert_eq!(events.len(), if tells { 4 } else { 1 });
            assert_eq!(body["result"]["last"], through);
            if !tells {
                assert_eq!(events[0]["data"]["text"], "public");
            }
        }
    }
}
