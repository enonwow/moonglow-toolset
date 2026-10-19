//! The agent API's request routing, shared with the gateway's /agent/v1. Authentication,
//! consent and transport belong to the host.
#[path = "api_route.rs"]
mod routing;
use crate::{access::ClientGroup, store::LiveStore};
pub use routing::{RouteContext, RouteResponse, route, route_with_context};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn catalogue(live: &LiveStore, scope: &crate::access::Scope, openapi: bool) -> Value {
    let mut policy = live.policy();
    policy.scripts &= scope.has(ClientGroup::Scripts);
    policy.chunks &= scope.has(ClientGroup::Chunks);
    policy.players &= scope.has(ClientGroup::Players);
    let mut value = if openapi {
        crate::operations::openapi(policy)
    } else {
        crate::operations::describe(policy)
    };
    if let Some(ops) = value.get_mut("operations").and_then(Value::as_array_mut) {
        ops.retain(|o| scope.allows(o["name"].as_str().unwrap_or("")));
    }
    if let Some(paths) = value.get_mut("paths").and_then(Value::as_object_mut) {
        paths.retain(|p, _| scope.allows(p.trim_start_matches("/v1/call/")));
    }
    value
}
fn catalogue_tag(value: &Value) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, value.to_string().as_bytes());
    format!("\"{}\"", digest.as_ref().iter().map(|b| format!("{b:02x}")).collect::<String>())
}
