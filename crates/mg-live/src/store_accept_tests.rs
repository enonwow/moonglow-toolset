//! Shared acceptance regressions, independent of the transport implementation.
use super::*;
use crate::access::{ClientGroup, DeliveryMode, Scope};

fn store() -> LiveStore {
    let store = LiveStore::default();
    store
        .sync(br#"{"v":1,"boot":"boot","seq":1,"areas":[{"area":"a","cre":[{"id":"1"}]}]}"#, 0)
        .unwrap();
    store
}
fn read_command(id: &str) -> Command {
    serde_json::from_value(json!({"id":id,"area":"a","op":"get_variable",
        "target":"1","type":"int","name":"example"}))
    .unwrap()
}
fn reader(owner: &str) -> Scope {
    Scope::restricted(owner, vec![ClientGroup::Read], DeliveryMode::Auto)
}

#[test]
fn scoped_game_read_quota_survives_scope_recreation_and_preserves_local_access() {
    let store = store();
    for n in 0..60 {
        store.enqueue_scoped(read_command(&format!("read-{n}")), 0, &reader("reader")).unwrap();
    }
    let e = store.enqueue_scoped(read_command("excess"), 1, &reader("reader")).unwrap_err();
    assert_eq!(serde_json::to_value(e.code).unwrap(), json!("rate_limited"));
    assert!(!store.lock().accepted.contains_key("excess"));
    assert!(store.enqueue_scoped(read_command("other"), 1, &reader("other")).is_ok());
    assert!(store.enqueue(read_command("local"), 1).is_ok());
    assert!(store.enqueue_scoped(read_command("next-window"), 60_000, &reader("reader")).is_ok());
}

#[test]
fn global_game_command_budget_bounds_rotating_owner_identities() {
    let store = store();
    for n in 0..600 {
        store
            .enqueue_scoped(read_command(&format!("read-{n}")), 0, &reader(&format!("reader-{n}")))
            .unwrap();
        // Retention/transport are separate concerns from the shared acceptance budget.
        store.lock().queue.clear();
    }
    let e = store.enqueue_scoped(read_command("excess"), 1, &reader("fresh-owner")).unwrap_err();
    assert_eq!(serde_json::to_value(e.code).unwrap(), json!("rate_limited"));
    assert!(!store.lock().accepted.contains_key("excess"));
    assert!(store.enqueue(read_command("local"), 1).is_ok());
    assert!(
        store.enqueue_scoped(read_command("next-window"), 60_000, &reader("fresh-owner")).is_ok()
    );
}

#[test]
fn concurrent_game_commands_share_one_owner_budget() {
    let store = Arc::new(store());
    let scope = reader("reader").with_command_limit(5).unwrap();
    let workers: Vec<_> = (0..20)
        .map(|n| {
            let store = store.clone();
            let scope = scope.clone();
            std::thread::spawn(move || {
                store.enqueue_scoped(read_command(&format!("read-{n}")), 0, &scope).is_ok()
            })
        })
        .collect();
    assert_eq!(workers.into_iter().map(|w| usize::from(w.join().unwrap())).sum::<usize>(), 5);
}

#[test]
fn command_quota_is_charged_once_and_before_durable_id_reservation() {
    let root = std::env::temp_dir()
        .join(format!("mg-quota-{}", crate::service::generate_token().unwrap()));
    let store = store();
    store.lock().command_ids = Some(crate::command_ids::Journal::new(root.clone()).unwrap());
    let scope = reader("reader").with_command_limit(1).unwrap();
    let first = store.enqueue_scoped(read_command("first"), 0, &scope);
    let second = store.enqueue_scoped(read_command("excess"), 1, &scope);
    let reserved_excess =
        store.lock().command_ids.as_ref().unwrap().contains_boot("boot", "excess");
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
    assert!(first.is_ok(), "reservation revalidation must not double-charge");
    assert_eq!(second.unwrap_err().code, ErrorCode::RateLimited);
    assert!(!reserved_excess, "quota refusal must not burn a durable ID");
}
