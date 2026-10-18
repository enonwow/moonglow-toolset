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

fn results(store: &LiveStore, boot: &str, seq: u64, values: Value, ack: Value) {
    store
        .sync(
            json!({"v":1,"boot":boot,"seq":seq,"results":values,"ack":ack}).to_string().as_bytes(),
            seq,
        )
        .unwrap();
}

#[test]
fn late_unknown_result_survives_ui_eviction_and_advances_gateway_revision() {
    use super::gateway::{Gateway, Push, SessionConfig};
    use super::mirror::ResultState;
    let store = Arc::new(store());
    let gateway = Gateway::new(store.clone()).unwrap();
    gateway
        .connect(
            SessionConfig {
                id: "reader".into(),
                id_prefix: "reader.".into(),
                groups: vec!["read".into()],
                watch_limit: 1,
            },
            None,
            &[],
            0,
        )
        .unwrap();
    let id = "reader.1234abcd.1";
    let mut command = serde_json::to_value(read_command(id)).unwrap();
    command["boot"] = json!("boot");
    assert_eq!(gateway.commands("reader", &[command], 0).unwrap().accepted, [id]);
    results(&store, "boot", 2, json!([]), json!([id]));
    let before = gateway
        .pushes("reader", 2)
        .unwrap()
        .into_iter()
        .find_map(|p| match p {
            Push::Result(r) if r.id == id => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(before.state, ResultState::Unknown);
    gateway.ack("reader", &[(id.into(), before.rev)], 0).unwrap();
    let mut finished = Vec::new();
    for n in 0..MAX_RESULTS + 1 {
        let id = store.enqueue(read_command(&format!("history-{n}")), 2).unwrap();
        finished.push(json!({"id":id,"ok":true}));
    }
    store.commands(2);
    results(&store, "boot", 3, json!(finished), json!([]));
    assert!(!store.lock().results.iter().any(|(_, r)| r.id == id));
    let cursor = store.event_cursor();
    results(
        &store,
        "boot",
        4,
        json!([{"id":id,"ok":false,"code":"dead","err":"Original failure"}]),
        json!([]),
    );
    let resolved = store.result(id).expect("late result must resolve after UI eviction");
    assert_eq!(resolved.code, Some(ErrorCode::Dead));
    assert!(!resolved.ok);
    assert_eq!(resolved.by.as_deref(), Some("reader"));
    assert!(store.event_cursor() > cursor);
    let after = gateway
        .pushes("reader", 4)
        .unwrap()
        .into_iter()
        .find_map(|p| match p {
            Push::Result(r) if r.id == id => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(after.state, ResultState::Final);
    assert_eq!(after.rev, before.rev + 1);
    assert_eq!(after.result.unwrap(), resolved);
    results(&store, "boot", 5, json!([{"id":id,"ok":true}]), json!([]));
    assert_eq!(store.result(id), Some(resolved));
    assert!(
        !gateway
            .pushes("reader", 5)
            .unwrap()
            .iter()
            .any(|p| matches!(p, Push::Result(r) if r.id == id))
    );
}

#[test]
fn late_unknown_result_from_another_boot_cannot_resolve_the_old_command() {
    let store = store();
    let id = store.enqueue(read_command("old"), 0).unwrap();
    results(&store, "boot", 2, json!([]), json!([id]));
    assert_eq!(store.result(&id).unwrap().code, Some(ErrorCode::Unknown));
    results(&store, "other-boot", 1, json!([{"id":id,"ok":true}]), json!([]));
    assert_eq!(store.result(&id).unwrap().code, Some(ErrorCode::Unknown));
}

#[test]
fn unresolved_results_do_not_cross_endpoints_even_with_the_same_boot_name() {
    let store = store();
    let id = store.enqueue(read_command("old-endpoint"), 0).unwrap();
    results(&store, "boot", 2, json!([]), json!([id]));
    assert_eq!(store.result(&id).unwrap().code, Some(ErrorCode::Unknown));
    store.bind_server(&crate::bridge::RedisConfig {
        prefix: "other".into(),
        host: "localhost".into(),
        port: 6379,
    });
    results(&store, "boot", 1, json!([{"id":id,"ok":true}]), json!([]));
    assert!(store.result(&id).is_none());
    assert!(store.lock().unresolved.entries.is_empty());
}

#[test]
fn unresolved_registry_bounds_both_id_count_and_retained_payload_bytes() {
    {
        let store = store();
        for n in 0..=MAX_UNRESOLVED {
            let command = read_command(&format!("unknown-{n}"));
            let result = failure(&command.id, ErrorCode::Unknown, "unknown");
            push_result(&mut store.lock(), command, result);
        }
        assert_eq!(store.lock().unresolved.entries.len(), MAX_UNRESOLVED);
        assert!(store.result("unknown-0").is_none());
        assert_eq!(
            store.result(&format!("unknown-{MAX_UNRESOLVED}")).unwrap().code,
            Some(ErrorCode::Unknown)
        );
    }
    let store = store();
    for n in 0..100 {
        let command: Command = serde_json::from_value(json!({"id":format!("large-{n}"),
            "area":"a","op":"spawn_json","x":1,"y":2,"z":0,"f":0,
            "json":{"payload":"x".repeat(200_000)}}))
        .unwrap();
        let result = failure(&command.id, ErrorCode::Unknown, "unknown");
        push_result(&mut store.lock(), command, result);
    }
    let state = store.lock();
    assert!(state.unresolved.bytes <= MAX_UNRESOLVED_BYTES);
    assert!(state.unresolved.entries.len() < 100);
    assert_eq!(state.unresolved.order.len(), state.unresolved.entries.len());
    drop(state);
    assert!(store.result("large-0").is_none());
    assert_eq!(store.result("large-99").unwrap().code, Some(ErrorCode::Unknown));
}
