//! Avatars reach the desktop only through the connected gateway, with the session bearer.
#[allow(dead_code)]
mod link_support;
use link_support::*;
use mg_gateway::{Authenticator, Principal};
use mg_live_link::{AvatarClient, LinkError, LinkService, SessionToken};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug)]
struct Manager;
impl Authenticator for Manager {
    fn authenticate(&self, bearer: &str) -> Option<Principal> {
        (bearer == SECRET).then(|| Principal {
            id: "dm:test".into(),
            name: "Test".into(),
            groups: ["read", "focus", "manage_ai"].map(str::to_owned).to_vec(),
        })
    }
    fn principal(&self, id: &str) -> Option<Principal> {
        (id == "dm:test").then(|| self.authenticate(SECRET)).flatten()
    }
}
fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 200, 100);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&vec![99; 200 * 100 * 4]).unwrap();
    }
    bytes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn avatar_comes_from_the_gateway_with_the_bearer_and_the_origin_is_only_its_business() {
    let host = Host::with_auth(Arc::new(Manager)).await;
    let origin_url = "https://cdn.example.org/scribe.png";
    host.serve_avatar(origin_url, png());
    let link = LinkService::start(host.config(), &tokens(), Arc::new(|| {})).unwrap();
    until(|| link.is_connected()).await;
    let created =
        reply_async(link.ai_create(json!({"name":"Scribe","avatar":origin_url})).unwrap())
            .await
            .unwrap();
    let path = created["ai"]["avatar_path"].as_str().unwrap().to_owned();
    assert_eq!(path, format!("/avatars/{}", created["ai"]["id"].as_str().unwrap()));
    assert_eq!(created["ai"]["avatar"], origin_url);

    let client = link.avatar_client();
    let image = tokio::task::spawn_blocking({
        let (client, path) = (client.clone(), path.clone());
        move || client.fetch(&path)
    })
    .await
    .unwrap()
    .unwrap();
    let info = png::Decoder::new(std::io::Cursor::new(&image)).read_info().unwrap();
    assert_eq!((info.info().width, info.info().height), (128, 64));
    // Only the gateway talked to the avatar host.
    assert_eq!(host.avatar_fetches(), [origin_url]);

    // The hostile-path guard holds: nothing but a gateway /avatars/ path becomes a request.
    for bad in ["https://cdn.example.org/scribe.png", "//cdn.example.org/avatars/x", "/files/v1/x"]
    {
        assert_eq!(client.fetch(bad).unwrap_err(), LinkError::Protocol);
    }
    assert_eq!(host.avatar_fetches().len(), 1);

    // Another credential is refused, and an unknown principal is not found.
    let stranger = AvatarClient::new(
        &host.config().gateway,
        SessionToken::new("not-a-session".into()).unwrap(),
    )
    .unwrap();
    let other = path.clone();
    let result = tokio::task::spawn_blocking(move || stranger.fetch(&other)).await.unwrap();
    assert_eq!(result.unwrap_err(), LinkError::Rejected);
    // The fixture's TLS proxy carries one request per connection, so use a fresh client.
    let fresh =
        AvatarClient::new(&host.config().gateway, SessionToken::new(SECRET.into()).unwrap())
            .unwrap();
    let missing =
        tokio::task::spawn_blocking(move || fresh.fetch("/avatars/ai:nobody")).await.unwrap();
    assert_eq!(missing.unwrap_err(), LinkError::Rejected);
}
