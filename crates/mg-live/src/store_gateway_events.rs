//! One audience projection for gateway pushes, paging and scoped event reads.
use super::*;

pub(in crate::store) fn project_event(s: &State, event: &Event, scope: &Scope) -> Option<Event> {
    let groups = scope.gateway_groups.as_ref()?;
    let has = |group: &str| groups.iter().any(|g| g == group);
    if !has("read")
        || (matches!(event.kind.as_str(), "chat" | "dialog" | "server_log") && !has("read_chat"))
        || (event.kind == "chat"
            && (event.data["channel"] == "tell"
                || matches!(event.data["channel"].as_u64(), Some(4 | 20)))
            && !has("read_tells"))
    {
        return None;
    }
    let mut row = event.clone();
    if !matches!(event.kind.as_str(), "command" | "skill_roll") {
        return Some(row);
    }
    let owner = event.data["by"]
        .as_str()
        .or_else(|| event.data["value"]["draft"]["client"].as_str())
        .or_else(|| {
            event.data["id"]
                .as_str()
                .or_else(|| event.data["request"].as_str())
                .and_then(|id| s.owners.get(id).map(String::as_str))
        });
    let full = has("admin")
        || owner == Some(scope.name())
        || (has("approve_drafts")
            && scope.name().starts_with("dm:")
            && owner.is_some_and(|ai| s.gateway.identities.owned_by(ai, scope.name())));
    if !full {
        // Construct an allowlist, never subtract known private fields from a result.
        let outcome = event
            .data
            .get("code")
            .and_then(|v| serde_json::from_value::<ErrorCode>(v.clone()).ok())
            .map(|v| serde_json::to_value(v).expect("error code serializes"))
            .unwrap_or_else(|| {
                json!(if event.data["pending"] == true {
                    "pending"
                } else if event.data["ok"] == true {
                    "ok"
                } else {
                    "failed"
                })
            });
        row.name.clear();
        row.data = json!({"actor":owner, "op":event.data["op"].as_str(),
            "target":event.id.as_deref().or(event.target.as_deref()),
            "area":event.area,"time":event.at_ms,"outcome":outcome});
    }
    Some(row)
}
