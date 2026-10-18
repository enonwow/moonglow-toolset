//! Avatar images come from the connected gateway only, with the session bearer and the same
//! pin or WebPKI roots as the link. The third-party URL a gateway advertises is display text:
//! Moonglow never resolves or connects to it.
use crate::{Gateway, LinkError, SessionToken};
use std::{io::Read, sync::Arc, time::Duration};
use url::Url;

/// The gateway serves PNGs of at most 128 by 128 pixels; this is generous.
const MAX_BYTES: u64 = 256 * 1024;

/// Cheap to clone; fetches block, so call from a worker thread.
#[derive(Clone)]
pub struct AvatarClient(Arc<Inner>);
struct Inner {
    origin: Url,
    token: SessionToken,
    agent: ureq::Agent,
}
impl std::fmt::Debug for AvatarClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvatarClient").field("origin", &self.0.origin.as_str()).finish()
    }
}
impl AvatarClient {
    /// Uses the gateway's pin or WebPKI roots, like the link itself.
    pub fn new(gateway: &Gateway, token: SessionToken) -> Result<Self, crate::Error> {
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(Duration::from_secs(10))
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(5))
            .tls_config(gateway.tls_config()?)
            .build();
        Ok(Self(Arc::new(Inner { origin: gateway.origin.clone(), token, agent })))
    }
    /// Whether both handles belong to the same connection (same gateway and credential).
    pub fn same_connection(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// The only URL a path can become: the gateway origin plus an `/avatars/` path.
    pub fn url(&self, path: &str) -> Result<Url, LinkError> {
        if !path.starts_with("/avatars/")
            || path.len() > 256
            || !path.bytes().all(|b| b.is_ascii_graphic())
            || path.contains(['?', '#', '\\'])
        {
            return Err(LinkError::Protocol);
        }
        let url = self.0.origin.join(path).map_err(|_| LinkError::Protocol)?;
        // Dot segments (also percent-encoded) are resolved by `join`: check the result.
        if url.origin() != self.0.origin.origin() || !url.path().starts_with("/avatars/") {
            return Err(LinkError::Protocol);
        }
        Ok(url)
    }
    /// The PNG bytes the gateway serves for `path` (an `avatar_path` from presence or `ai.list`).
    pub fn fetch(&self, path: &str) -> Result<Vec<u8>, LinkError> {
        let url = self.url(path)?;
        let response = self
            .0
            .agent
            .get(url.as_str())
            .set("Authorization", &format!("Bearer {}", self.0.token.expose()))
            .call()
            .map_err(|e| match e {
                ureq::Error::Status(..) => LinkError::Rejected,
                ureq::Error::Transport(_) => LinkError::Transport,
            })?;
        if response.status() != 200 || response.content_type() != "image/png" {
            return Err(LinkError::Protocol);
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| LinkError::Transport)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(LinkError::TooLarge);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(origin: &str) -> AvatarClient {
        AvatarClient::new(
            &Gateway::new(origin, None).unwrap(),
            SessionToken::new("fixture".into()).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn paths_become_urls_on_the_gateway_origin_only() {
        let c = client("https://gw.example.org:8443");
        assert_eq!(
            c.url("/avatars/ai:scribe").unwrap().as_str(),
            "https://gw.example.org:8443/avatars/ai:scribe"
        );
        for bad in [
            "https://evil.example.net/avatars/x",
            "//evil.example.net/avatars/x",
            "/\\evil.example.net/avatars/x",
            "/avatars/../files/v1/generation",
            "/avatars/%2e%2e/files/v1/generation",
            "/avatars/x?y=1",
            "/avatars/x#y",
            "/avatars/x y",
            "avatars/x",
            "/files/v1/x",
            "",
        ] {
            let url = c.url(bad);
            assert!(url.is_err(), "{bad} -> {url:?}");
        }
    }
}
