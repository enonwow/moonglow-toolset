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
    fn authenticate(&self, token: &str) -> Option<Principal> {
        (token == "test-credential").then(|| Principal {
            id: "dm:test".into(),
            name: "Test".into(),
            groups: vec!["read".into(), "npc".into()],
        })
    }
}
async fn server(config: Config) -> (String, CoreHandle, tokio::task::JoinHandle<()>) {
    let core = CoreHandle::new(Arc::new(LiveStore::default())).await.unwrap();
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
async fn origin_and_missing_credentials_never_upgrade() {
    let (url, _, task) = server(Config::default()).await;
    for (origin, expected) in [(true, 403), (false, 401)] {
        let mut r = request(&url);
        if origin {
            r.headers_mut().insert("origin", "https://example.invalid".parse().unwrap());
        } else {
            r.headers_mut().remove("authorization");
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
