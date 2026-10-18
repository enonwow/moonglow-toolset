use mg_live::store::LiveStore;
use serde_json::json;

#[test]
fn receipts_follow_the_game_batch_and_stop_when_the_game_clears_it() {
    let store = LiveStore::default();
    let report = |seq, results| {
        store
            .sync(
                &serde_json::to_vec(&json!({"v":1,"boot":"b","seq":seq,
            "results":results}))
                .unwrap(),
                seq,
            )
            .unwrap();
    };
    // Control results also need receipts even though they are absent from history.
    report(1, json!([{"id":"watch-control","ok":true,"receipt":"b:1"}]));
    assert_eq!(store.commands(1)["received"], json!(["b:1"]));
    report(2, json!([{"id":"watch-control","ok":true,"receipt":"b:1"}]));
    assert_eq!(store.commands(2)["received"], json!(["b:1"]));
    report(3, json!([]));
    assert_eq!(store.commands(3)["received"], json!([]));
    report(2, json!([{"id":"watch-control","ok":true,"receipt":"b:1"}]));
    assert_eq!(store.commands(4)["received"], json!([]));
}
