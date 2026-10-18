use mg_live::store::{COMMAND_TTL_MS, Command, ErrorCode, LiveStore};
use serde_json::json;

fn report(store: &LiveStore, boot: &str, seq: u64, results: serde_json::Value, now: u64) {
    store
        .sync(
            &serde_json::to_vec(&json!({"v":1,"boot":boot,"seq":seq,
        "areas":[{"area":"market","full":true}],"results":results}))
            .unwrap(),
            now,
        )
        .unwrap();
}
fn queued(store: &LiveStore, id: &str) {
    let command: Command = serde_json::from_value(json!({"id":id,"area":"market",
        "op":"run_script","script":"test_script"}))
    .unwrap();
    store.enqueue(command, 0).unwrap();
}

#[test]
fn timeout_distinguishes_unsent_from_handed_off_and_late_failure_resolves_unknown() {
    let store = LiveStore::default();
    report(&store, "b", 1, json!([]), 0);
    queued(&store, "sent");
    assert_eq!(store.commands(0)["commands"].as_array().unwrap().len(), 1);
    queued(&store, "unsent");
    store.status(COMMAND_TTL_MS);
    assert_eq!(store.result("sent").unwrap().code, Some(ErrorCode::Unknown));
    assert_eq!(store.result("unsent").unwrap().code, Some(ErrorCode::Expired));
    report(
        &store,
        "b",
        2,
        json!([{"id":"sent","ok":false,"err":"native failure"}]),
        COMMAND_TTL_MS + 1,
    );
    assert_eq!(store.result("sent").unwrap().err, "native failure");
    assert!(!store.result("sent").unwrap().ok);
}

#[test]
fn restart_preserves_uncertainty_only_for_sent_commands() {
    let store = LiveStore::default();
    report(&store, "old", 1, json!([]), 0);
    queued(&store, "sent");
    store.commands(0);
    queued(&store, "unsent");
    report(&store, "new", 1, json!([]), 1);
    assert_eq!(store.result("sent").unwrap().code, Some(ErrorCode::Unknown));
    assert_eq!(store.result("unsent").unwrap().code, Some(ErrorCode::Expired));
}
