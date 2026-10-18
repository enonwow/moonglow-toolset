use mg_live::store::{LiveStore, SyncOutcome, Synced};
use serde_json::{Value, json};

fn frame(boot: &str, seq: u64, base: Option<u64>, x: i32) -> Value {
    let mut frame = json!({"v":1,"boot":boot,"seq":seq,
        "areas":[{"area":"room","full":base.is_none(),
                  "cre":[{"id":"7f01","r":"guard","x":x}]}]});
    if let Some(base) = base {
        frame["base_seq"] = json!(base);
    }
    frame
}

fn tap(store: &LiveStore, frame: Value) -> Synced {
    let result = store.sync_tapped(&serde_json::to_vec(&frame).unwrap(), 1000).unwrap();
    assert_eq!(result.reply, store.commands(1000));
    result
}

fn x(store: &LiveStore) -> f32 {
    store.area("room").unwrap().creatures[0].x
}

#[test]
fn full_includes_equal_sequence_replacement_and_initial_zero() {
    let store = LiveStore::default();
    let initial = tap(&store, frame("boot", 0, None, 1));
    assert_eq!(initial.outcome, SyncOutcome::Full);
    assert!(initial.boot_changed);
    let duplicate = tap(&store, frame("boot", 0, None, 2));
    assert_eq!(duplicate.outcome, SyncOutcome::Full);
    assert!(!duplicate.boot_changed);
    assert_eq!(x(&store), 2.0);
}

#[test]
fn delta_extends_the_exact_baseline() {
    let store = LiveStore::default();
    tap(&store, frame("boot", 1, None, 1));
    let delta = tap(&store, frame("boot", 2, Some(1), 2));
    assert_eq!(delta.outcome, SyncOutcome::Delta);
    assert!(!delta.boot_changed);
    assert_eq!(x(&store), 2.0);
}

#[test]
fn gap_keeps_area_state_and_requests_resync_until_a_full_frame() {
    let store = LiveStore::default();
    tap(&store, frame("boot", 1, None, 1));
    let gap = tap(&store, frame("boot", 3, Some(1), 3));
    assert_eq!(gap.outcome, SyncOutcome::Gap);
    assert!(!gap.boot_changed);
    assert_eq!(x(&store), 1.0);
    assert!(gap.reply["commands"].as_array().unwrap().iter().any(|c| c["op"] == "resync"));
    assert_eq!(tap(&store, frame("boot", 4, Some(3), 4)).outcome, SyncOutcome::Gap);
    // A full at the gap's sequence still restores the baseline.
    assert_eq!(tap(&store, frame("boot", 4, None, 4)).outcome, SyncOutcome::Full);
    assert_eq!(tap(&store, frame("boot", 5, Some(4), 5)).outcome, SyncOutcome::Delta);
    assert_eq!(x(&store), 5.0);
}

#[test]
fn stale_covers_older_full_and_older_or_equal_delta() {
    let store = LiveStore::default();
    tap(&store, frame("boot", 2, None, 2));
    for (seq, base) in [(1, None), (1, Some(0)), (2, Some(1))] {
        let stale = tap(&store, frame("boot", seq, base, 99));
        assert_eq!(stale.outcome, SyncOutcome::Stale);
        assert!(!stale.boot_changed);
        assert_eq!(x(&store), 2.0);
    }
}

#[test]
fn changed_boot_is_reported_for_full_and_gap_frames() {
    let store = LiveStore::default();
    tap(&store, frame("first", 5, None, 5));
    let restart = tap(&store, frame("second", 1, None, 1));
    assert_eq!(restart.outcome, SyncOutcome::Full);
    assert!(restart.boot_changed);
    let restart_delta = tap(&store, frame("third", 1, Some(0), 2));
    assert_eq!(restart_delta.outcome, SyncOutcome::Gap);
    assert!(restart_delta.boot_changed);
    assert!(store.area("room").is_none());
}

#[test]
fn gap_covers_missing_baseline_wrong_base_and_unknown_partial_area() {
    for mode in 0..3 {
        let store = LiveStore::default();
        if mode != 0 {
            tap(&store, frame("boot", 1, None, 1));
        }
        let mut delta = frame("boot", 2, Some(if mode == 1 { 0 } else { 1 }), 2);
        if mode == 2 {
            delta["areas"][0]["area"] = json!("unknown");
        }
        assert_eq!(tap(&store, delta).outcome, SyncOutcome::Gap);
    }
}

#[test]
fn tapped_and_wrapper_preserve_validation_errors() {
    let store = LiveStore::default();
    for body in [b"{".as_slice(), br#"{"v":2,"boot":"boot"}"#, br#"{"v":1,"boot":""}"#] {
        assert_eq!(store.sync(body, 1000).unwrap_err(), store.sync_tapped(body, 1000).unwrap_err());
    }
    assert!(store.area("room").is_none());
}
