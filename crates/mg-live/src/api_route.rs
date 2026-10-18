//! Shared synchronous HTTP routing. Authentication belongs to the transport.
use super::{catalogue, catalogue_tag};
use crate::{
    access::{ClientGroup, Scope},
    store::LiveStore,
};
use serde_json::{Value, json};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, PartialEq)]
pub struct RouteResponse {
    pub status: u16,
    pub body: Value,
    pub etag: Option<String>,
}
/// Optional transport services. The quota hook runs only after schema ETag and scope checks.
pub struct RouteContext<'a> {
    pub now_ms: u64,
    pub stop: &'a AtomicBool,
    pub charge: &'a dyn Fn() -> Result<(), u64>,
    pub agents: Value,
}
impl std::fmt::Debug for RouteContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouteContext").field("now_ms", &self.now_ms).finish_non_exhaustive()
    }
}
pub fn route(
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
    scope: &Scope,
    live: &LiveStore,
) -> RouteResponse {
    route_with_context(
        method,
        path,
        headers,
        body,
        scope,
        live,
        &RouteContext {
            now_ms: super::now(),
            stop: &AtomicBool::new(false),
            charge: &|| Ok(()),
            agents: json!([]),
        },
    )
}
pub fn route_with_context(
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
    scope: &Scope,
    live: &LiveStore,
    context: &RouteContext<'_>,
) -> RouteResponse {
    let authorized = match live.authorize_scope(scope) {
        Ok(scope) => scope,
        Err(e) => {
            return RouteResponse {
                status: 403,
                body: json!({"ok":false,"error":e.error,"code":e.code}),
                etag: None,
            };
        }
    };
    let scope = &authorized;
    let now_ms = context.now_ms;
    let stop = context.stop;
    let input: Result<Value, _> = serde_json::from_slice(body);
    let op = operation(path, input.as_ref().ok());
    let mut response = RouteResponse { status: 200, body: Value::Null, etag: None };
    if path == "/v1/ping" {
        response.status = if method == "GET" { 200 } else { 405 };
        response.body = if method == "GET" {
            json!({"ok":true})
        } else {
            json!({"ok":false,"error":"Use GET"})
        };
        return response;
    }
    let (status, body) = (|| {
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
            response.etag = Some(catalogue_tag(&value));
            return (200, value);
        }
        if path.starts_with("/v1/call/")
            && let Some(expected) = headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("if-match"))
                .map(|(_, value)| value.as_str())
        {
            let current = catalogue_tag(&catalogue(live, scope, false));
            response.etag = Some(current.clone());
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
            && let Err(seconds) = (context.charge)()
        {
            return (
                429,
                json!({"ok":false,"code":"rate_limited","error":"Client command limit exceeded","retryAfterSeconds":seconds}),
            );
        }
        if path == "/live/v1/status" {
            let mut value = live.status_scoped(now_ms, scope);
            value["agents"] = context.agents.clone();
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
                    v["agents"] = context.agents.clone();
                }
                (200, if legacy { v } else { json!({"ok":true,"result":v}) })
            }
            Err(e) => (
                if e.code == crate::store::ErrorCode::Forbidden { 403 } else { 400 },
                json!({"ok":false,"error":e.error,"code":e.code}),
            ),
        }
    })();
    response.status = status;
    response.body = body;
    response
}
pub(super) fn operation<'a>(path: &'a str, input: Option<&'a Value>) -> &'a str {
    match path {
        "/v1/ping" => "ping",
        "/v1/describe" => "describe",
        "/v1/openapi.json" => "openapi",
        "/live/v1/status" => "live_status",
        "/live/v1/command" => input.and_then(|v| v["op"].as_str()).unwrap_or("unknown"),
        p if p.starts_with("/live/v1/area/") => "live_area_state",
        p if p.starts_with("/v1/call/") => p.trim_start_matches("/v1/call/"),
        _ => "unknown",
    }
}
