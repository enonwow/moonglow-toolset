use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode},
};
use mg_gateway::{
    Authenticator, Config, CoreHandle, Principal,
    avatars::{AvatarLimits, AvatarSource, Avatars, HttpsSource},
};
use mg_live::store::{LiveStore, Policy};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
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

/// Serves bodies by URL path, counts fetches and can park a fetch until released.
#[derive(Debug, Default)]
struct Origin {
    bodies: Mutex<BTreeMap<String, Vec<u8>>>,
    calls: AtomicUsize,
    park: Mutex<Option<mpsc::Receiver<()>>>,
}
impl AvatarSource for Origin {
    fn fetch(&self, url: &url::Url) -> Result<Vec<u8>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(release) = self.park.lock().unwrap().as_ref() {
            let _ = release.recv_timeout(Duration::from_secs(5));
        }
        self.bodies.lock().unwrap().get(url.path()).cloned().ok_or_else(|| "missing".into())
    }
}
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![200; width as usize * height as usize * 4])
            .unwrap();
    }
    bytes
}

struct Fixture {
    app: Router,
    origin: Arc<Origin>,
    ai_token: String,
    ai_id: String,
    bare_id: String,
}
async fn fixture(avatars: impl FnOnce(Arc<Origin>) -> Avatars) -> Fixture {
    let store = Arc::new(LiveStore::default());
    store.sync(br#"{"v":1,"boot":"boot","areas":[]}"#, 1).unwrap();
    let core = CoreHandle::new(store).await.unwrap();
    let origin = Arc::new(Origin::default());
    let (ai_token, ai_id, bare_id) = core
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
            let create = |a| {
                g.handle(
                    &owner.id,
                    mg_live::gateway_frames::Request { id: 1, op: "ai.create".into(), a },
                    mg_live::now_ms(),
                )
                .result
                .unwrap()
            };
            let with = create(json!({"name":"Scribe","groups":["read"],
                "avatar":"https://cdn.example.org/scribe.png"}));
            let bare = create(json!({"name":"Plain","groups":["read"]}));
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
            (
                with["token"].as_str().unwrap().to_owned(),
                with["ai"]["id"].as_str().unwrap().to_owned(),
                bare["ai"]["id"].as_str().unwrap().to_owned(),
            )
        })
        .await
        .unwrap();
    let app = mg_gateway::router(
        core,
        Arc::new(Auth),
        Arc::new(Policy::default()),
        Config { avatars: Arc::new(avatars(origin.clone())), ..Default::default() },
    );
    Fixture { app, origin, ai_token, ai_id, bare_id }
}
fn standard(origin: Arc<Origin>) -> Avatars {
    Avatars::new(origin, AvatarLimits::default())
}
async fn get(app: &Router, id: &str, token: Option<&str>) -> Response<Body> {
    let mut request = Request::builder().uri(format!("/avatars/{id}"));
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap()
}
fn dimensions(bytes: &[u8]) -> (u32, u32) {
    let reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
    (reader.info().width, reader.info().height)
}

#[tokio::test]
async fn requires_dm_or_ai_bearer_and_serves_a_small_png() {
    let f = fixture(standard).await;
    f.origin.bodies.lock().unwrap().insert("/scribe.png".into(), png(400, 200));
    for token in [None, Some("fixture-player"), Some("mgai_invalid")] {
        assert_eq!(get(&f.app, &f.ai_id, token).await.status(), StatusCode::UNAUTHORIZED);
    }
    assert_eq!(f.origin.calls.load(Ordering::SeqCst), 0, "no fetch before authentication");
    for token in ["fixture-dm", f.ai_token.as_str()] {
        let response = get(&f.app, &f.ai_id, Some(token)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "private, max-age=300");
        assert_eq!(response.headers()["content-type"], "image/png");
        let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        assert_eq!(dimensions(&body), (128, 64));
    }
    // Encoded ids reach the same principal.
    let encoded = f.ai_id.replace(':', "%3A");
    assert_eq!(get(&f.app, &encoded, Some("fixture-dm")).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn unknown_and_avatarless_principals_are_404() {
    let f = fixture(standard).await;
    for id in [f.bare_id.as_str(), "ai:nobody", "dm:test", "x"] {
        assert_eq!(get(&f.app, id, Some("fixture-dm")).await.status(), StatusCode::NOT_FOUND);
    }
    assert_eq!(f.origin.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn bad_origin_images_are_502_and_never_served() {
    let f = fixture(standard).await;
    let bodies = [
        ("missing", None),
        ("garbage", Some(b"not an image".to_vec())),
        ("truncated", Some(png(16, 16)[..30].to_vec())),
        ("huge pixels", Some(png(2000, 1))),
        ("oversized", Some(vec![0; 512 * 1024 + 1])),
    ];
    for (name, body) in bodies {
        f.origin.bodies.lock().unwrap().clear();
        if let Some(body) = body {
            f.origin.bodies.lock().unwrap().insert("/scribe.png".into(), body);
        }
        let response = get(&f.app, &f.ai_id, Some("fixture-dm")).await;
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY, "{name}");
    }
}

#[tokio::test]
async fn second_request_is_served_from_the_cache() {
    let f = fixture(standard).await;
    f.origin.bodies.lock().unwrap().insert("/scribe.png".into(), png(32, 32));
    for _ in 0..3 {
        assert_eq!(get(&f.app, &f.ai_id, Some("fixture-dm")).await.status(), StatusCode::OK);
    }
    assert_eq!(f.origin.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_unresolvable_origin_is_502_through_the_real_fetcher() {
    // The reserved .invalid TLD never resolves, so this needs no network.
    let f = fixture(|_| Avatars::new(Arc::new(HttpsSource), AvatarLimits::default())).await;
    assert_eq!(get(&f.app, &f.ai_id, Some("fixture-dm")).await.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn concurrent_fetches_are_bounded_per_principal() {
    let f =
        fixture(|origin| Avatars::new(origin, AvatarLimits { global: 4, per_principal: 1 })).await;
    f.origin.bodies.lock().unwrap().insert("/scribe.png".into(), png(32, 32));
    let (release, parked) = mpsc::channel();
    *f.origin.park.lock().unwrap() = Some(parked);
    let first = {
        let (app, id) = (f.app.clone(), f.ai_id.clone());
        tokio::spawn(async move { get(&app, &id, Some("fixture-dm")).await.status() })
    };
    while f.origin.calls.load(Ordering::SeqCst) == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        get(&f.app, &f.ai_id, Some("fixture-dm")).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    release.send(()).unwrap();
    assert_eq!(first.await.unwrap(), StatusCode::OK);
}
