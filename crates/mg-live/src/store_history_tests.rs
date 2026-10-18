//! Bounded idempotency acceptance with an injected clock.
use super::*;
const START: u64 = 1_800_000_000_000;
const DAY: u64 = 24 * 60 * 60 * 1000;
fn command(n: u64, now: u64) -> Command {
    serde_json::from_value(
        json!({"id":format!("test.1234abcd.{}", (1u64<<63) | (now*1024) | (n%1024)),
        "area":"a","op":"get_variable","target":"1","type":"int","name":"example"}),
    )
    .unwrap()
}
fn store() -> LiveStore {
    let store = LiveStore::default();
    store
        .sync(br#"{"v":1,"boot":"boot","seq":1,"areas":[{"area":"a","cre":[{"id":"1"}]}]}"#, START)
        .unwrap();
    store
}
#[test]
fn two_hundred_thousand_sequential_commands_never_hit_a_session_id_ceiling() {
    let store = store();
    for n in 0..200_000 {
        let now = START + n * 1000;
        let result = store.enqueue(command(n, now), now);
        assert!(result.is_ok(), "command {n}: {result:?}");
        let mut state = store.lock();
        state.queue.clear();
        assert!(state.history.bytes <= crate::command_history::HISTORY_BYTES);
        assert!(state.accepted.len() < 65_536);
        assert!(state.owners.len() < 65_536);
    }
}
#[test]
fn timed_replay_inside_horizon_is_idempotent_and_outside_is_expired() {
    let store = store();
    let command = command(1, START);
    store.enqueue(command.clone(), START).unwrap();
    store.enqueue(command.clone(), START + 1).unwrap();
    assert_eq!(store.lock().queue.len(), 1);
    store.lock().queue.clear();
    assert_eq!(store.enqueue(command, START + DAY + 1).unwrap_err().code, ErrorCode::Expired);
    assert!(store.lock().queue.is_empty());
}

#[test]
fn reused_id_returns_the_current_boot_outcome_after_returning_to_an_earlier_boot() {
    let store = store();
    let c = command(1, START);
    for (n, boot, value) in [(2, "boot", "first"), (4, "second", "second")] {
        store
            .sync(
                &serde_json::to_vec(&json!({"v":1,"boot":boot,"seq":n,
            "areas":[{"area":"a","full":true,"cre":[{"id":"1"}]}]}))
                .unwrap(),
                START + n,
            )
            .unwrap();
        store.enqueue(c.clone(), START + n).unwrap();
        store
            .sync(
                &serde_json::to_vec(&json!({"v":1,"boot":boot,"seq":n+1,
            "results":[{"id":c.id,"ok":true,"value":value}]}))
                .unwrap(),
                START + n + 1,
            )
            .unwrap();
    }
    store.sync(br#"{"v":1,"boot":"boot","seq":1}"#, START + 10).unwrap();
    store.enqueue(c.clone(), START + 10).unwrap();
    assert!(store.lock().queue.is_empty());
    assert_eq!(store.result(&c.id).unwrap().value, Some(json!("first")));
    assert!(
        store.lock().results.is_empty(),
        "another boot's UI row must not survive an ID collision"
    );
}
