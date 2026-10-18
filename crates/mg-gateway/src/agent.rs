use crate::App;
use axum::{
    Json,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use mg_live::api::{RouteContext, RouteResponse};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Mutex, atomic::AtomicBool},
};

#[derive(Debug)]
pub(crate) struct AgentState {
    windows: Mutex<HashMap<String, (u64, u32)>>,
    slots: std::sync::Arc<tokio::sync::Semaphore>,
}
impl Default for AgentState {
    fn default() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            slots: std::sync::Arc::new(tokio::sync::Semaphore::new(16)),
        }
    }
}
impl AgentState {
    fn charge(&self, id: &str, limit: u32, now: u64) -> Result<(), u64> {
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        windows.retain(|_, (start, _)| now.saturating_sub(*start) < 60_000);
        if !windows.contains_key(id) && windows.len() >= 4096 {
            return Err(60);
        }
        let (start, used) = windows.entry(id.into()).or_insert((now, 0));
        if *used >= limit {
            return Err((60_000 - now.saturating_sub(*start)).div_ceil(1000));
        }
        *used += 1;
        Ok(())
    }
}
pub(crate) async fn handle(State(app): State<App>, request: Request) -> Response {
    let (_, principal) = match crate::authenticate(&app, request.headers()).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !principal.id.starts_with("ai:") {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok":false,"code":"forbidden","error":"AI principal required"})),
        )
            .into_response();
    }
    let Ok(permit) = app.agents.slots.clone().try_acquire_owned() else {
        return response(RouteResponse {
            status: 429,
            body: json!({"ok":false,"code":"rate_limited","error":"Client command limit exceeded","retryAfterSeconds":1}),
            etag: None,
        });
    };
    let method = request.method().as_str().to_owned();
    let path = format!("/v1/{}", request.uri().path().trim_start_matches("/agent/v1/"));
    let headers = request
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_owned(), v.to_owned())))
        .collect::<Vec<_>>();
    let body = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        to_bytes(request.into_body(), 4 * 1024 * 1024),
    )
    .await
    {
        Ok(Ok(b)) => b,
        _ => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"ok":false,"error":"Body too large"})),
            )
                .into_response();
        }
    };
    let policy = app.policy.clone();
    let agents = app.agents.clone();
    let routed = app
        .core
        .with_store(move |store| {
            let _permit = permit;
            store.set_policy(policy.policy());
            let scope = principal.scope(policy.delivery(&principal)).expect("validated principal");
            let mut routed = mg_live::api::route_with_context(
                &method,
                &path,
                &headers,
                &body,
                &scope,
                store,
                &RouteContext {
                    now_ms: crate::now(),
                    stop: &AtomicBool::new(false),
                    charge: &|| {
                        agents.charge(&principal.id, policy.command_limit(&principal), crate::now())
                    },
                    agents: json!([]),
                },
            );
            // Gateway identities add a tell capability absent from legacy loopback scopes.
            // Preserve the global page cursor even when every returned row is private.
            if !principal.groups.iter().any(|g| g == "read_tells")
                && let Some(events) = routed
                    .body
                    .get_mut("result")
                    .and_then(|v| v.get_mut("events"))
                    .and_then(serde_json::Value::as_array_mut)
            {
                events.retain(|e| {
                    !(e["kind"] == "chat"
                        && (e["data"]["channel"] == "tell"
                            || matches!(e["data"]["channel"].as_u64(), Some(4 | 20))))
                });
            }
            routed
        })
        .await;
    match routed {
        Ok(r) => response(r),
        Err(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"ok":false,"error":"Core unavailable"})))
                .into_response()
        }
    }
}
fn response(r: RouteResponse) -> Response {
    let retry = r.body["retryAfterSeconds"].as_u64();
    let mut response =
        (StatusCode::from_u16(r.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(r.body))
            .into_response();
    if let Some(tag) = r.etag.and_then(|s| HeaderValue::from_str(&s).ok()) {
        response.headers_mut().insert("etag", tag);
    }
    if let Some(retry) = retry.and_then(|s| HeaderValue::from_str(&s.to_string()).ok()) {
        response.headers_mut().insert("retry-after", retry);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[derive(Debug)]
    struct Auth;
    impl crate::Authenticator for Auth {
        fn authenticate(&self, _: &str) -> Option<crate::Principal> {
            Some(crate::Principal {
                id: "ai:test".into(),
                name: "Test".into(),
                groups: vec!["read".into()],
            })
        }
    }
    #[tokio::test]
    async fn cancelled_http_requests_do_not_release_running_poll_slots() {
        let core =
            crate::CoreHandle::new(Arc::new(mg_live::store::LiveStore::default())).await.unwrap();
        let app = App {
            core,
            auth: Arc::new(Auth),
            policy: Arc::new(mg_live::store::Policy::default()),
            config: crate::Config::default(),
            agents: Arc::new(AgentState::default()),
            connections: Arc::new(tokio::sync::Semaphore::new(1)),
        };
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/agent/v1/call/live_events")
                .header("authorization", "Bearer test-agent")
                .body(axum::body::Body::from(r#"{"wait_ms":500}"#))
                .unwrap()
        };
        let mut polls = vec![];
        for _ in 0..16 {
            polls.push(tokio::spawn(handle(State(app.clone()), request())));
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while app.agents.slots.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // Give each acquired permit time to reach its blocking worker.
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        for p in polls {
            p.abort();
        }
        tokio::task::yield_now().await;
        assert_eq!(handle(State(app.clone()), request()).await.status(), 429);
        tokio::time::sleep(std::time::Duration::from_millis(550)).await;
        assert_eq!(app.agents.slots.available_permits(), 16);
    }
}
