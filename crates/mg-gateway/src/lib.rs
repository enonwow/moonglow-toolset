//! Public transport for the authoritative Live core. Host authentication stays pluggable.
mod agent;
mod core;
mod socket;

use axum::{
    Json, Router,
    extract::{Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
pub use core::CoreHandle;
use mg_live::{
    MIRROR_VERSION,
    access::DeliveryMode,
    gateway_frames::{PROTOCOL_VERSION, ServerIdentity},
    store::Policy,
};
use serde::Deserialize;
use serde_json::json;
use std::{fmt::Debug, sync::Arc};

pub use mg_live::gateway::Principal;
/// Called on blocking workers, on upgrade and again while the connection is alive.
/// Never include a credential in an error or Debug output.
pub trait Authenticator: Debug + Send + Sync + 'static {
    fn authenticate(&self, bearer: &str) -> Option<Principal>;
    /// Current owner identity for managed AI authorization. Missing owners fail closed.
    fn principal(&self, _id: &str) -> Option<Principal> {
        None
    }
}
pub trait PolicyResolver: Debug + Send + Sync + 'static {
    fn policy(&self) -> Policy;
    fn delivery(&self, principal: &Principal) -> DeliveryMode {
        if principal.id.starts_with("ai:") { DeliveryMode::Drafts } else { DeliveryMode::Auto }
    }
    fn command_limit(&self, _principal: &Principal) -> u32 {
        60
    }
}
impl PolicyResolver for Policy {
    fn policy(&self) -> Policy {
        *self
    }
}
/// Host routes (authentication, files, panel) are composed outside the core routes.
pub trait Extensions {
    fn router(&self, core: CoreHandle) -> Router;
}
#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerIdentity,
    pub queue_frames: usize,
    pub queue_bytes: usize,
    pub watch_limit: usize,
    pub connections: usize,
    pub features: Vec<String>,
    pub nwsync: Option<mg_live::gateway_frames::Nwsync>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerIdentity { id: "main".into(), name: "Live server".into() },
            queue_frames: 256,
            queue_bytes: 32 * 1024 * 1024,
            watch_limit: 16,
            connections: 128,
            features: vec![],
            nwsync: None,
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct App {
    core: CoreHandle,
    auth: Arc<dyn Authenticator>,
    policy: Arc<dyn PolicyResolver>,
    config: Config,
    agents: Arc<agent::AgentState>,
    connections: Arc<tokio::sync::Semaphore>,
}
pub fn router(
    core: CoreHandle,
    auth: Arc<dyn Authenticator>,
    policy: Arc<dyn PolicyResolver>,
    config: Config,
) -> Router {
    Router::new()
        .route("/health", get(|| async { Json(json!({"ok":true})) }))
        .route("/ws/v1", get(upgrade))
        .route("/agent/v1/{*path}", axum::routing::any(agent::handle))
        .with_state(App {
            core,
            auth,
            policy,
            connections: Arc::new(tokio::sync::Semaphore::new(config.connections)),
            config,
            agents: Arc::new(agent::AgentState::default()),
        })
}
#[derive(Debug, Default, Deserialize)]
struct Versions {
    proto: Option<u32>,
    mirror: Option<u32>,
}
async fn authenticate(
    app: &App,
    headers: &HeaderMap,
) -> Result<(String, Principal), Box<Response>> {
    if headers.contains_key("origin") {
        return Err(Box::new(
            (StatusCode::FORBIDDEN, Json(json!({"ok":false,"error":"Origin is not allowed"})))
                .into_response(),
        ));
    }
    let mut values = headers.get_all("authorization").iter();
    let bearer =
        values.next().and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
    let Some(bearer) =
        bearer.filter(|b| !b.is_empty() && b.len() <= 4096 && values.next().is_none())
    else {
        return Err(Box::new(unauthorized()));
    };
    let bearer = bearer.to_owned();
    let principal = authenticate_token(app, &bearer).await;
    match principal {
        Some(p) if p.validate().is_ok() => Ok((bearer, p)),
        _ => Err(Box::new(unauthorized())),
    }
}
pub(crate) async fn authenticate_token(app: &App, bearer: &str) -> Option<Principal> {
    let auth = app.auth.clone();
    let token = bearer.to_owned();
    app.core
        .with_gateway(move |g| {
            if let Some(owner) = g.token_owner(&token) {
                return g.authenticate_ai(&token, auth.principal(&owner), now());
            }
            if token.starts_with("mgai_") {
                return None;
            }
            let principal = auth.authenticate(&token)?;
            if !principal.id.starts_with("dm:") {
                return None;
            }
            g.refresh_principal(principal.clone()).ok()?;
            Some(principal)
        })
        .await
        .ok()
        .flatten()
}
fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"ok":false,"error":"Unauthorized"}))).into_response()
}
async fn upgrade(
    State(app): State<App>,
    Query(versions): Query<Versions>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let (token, principal) = match authenticate(&app, &headers).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if versions.proto.is_some_and(|v| v != PROTOCOL_VERSION)
        || versions.mirror.is_some_and(|v| v != MIRROR_VERSION)
    {
        return (
            StatusCode::UPGRADE_REQUIRED,
            Json(json!({"min_client":env!("CARGO_PKG_VERSION")})),
        )
            .into_response();
    }
    if !headers
        .get_all("sec-websocket-protocol")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(','))
        .any(|p| p.trim() == "moonglow-gw.1")
    {
        return (
            StatusCode::UPGRADE_REQUIRED,
            Json(json!({"min_client":env!("CARGO_PKG_VERSION")})),
        )
            .into_response();
    }
    if app.core.is_shutdown() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Ok(permit) = app.connections.clone().try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"ok":false,"code":"rate_limited","error":"Connection limit exceeded"})),
        )
            .into_response();
    };
    ws.protocols(["moonglow-gw.1"])
        .max_message_size(mg_live::gateway_frames::BOOK_FRAME_LIMIT)
        .max_frame_size(mg_live::gateway_frames::BOOK_FRAME_LIMIT)
        .on_upgrade(move |ws| async move {
            let _permit = permit;
            socket::run(ws, app, token, principal).await;
        })
        .into_response()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
