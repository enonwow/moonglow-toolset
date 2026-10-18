use mg_live::{
    access::{ClientGroup, DeliveryMode, Scope},
    operations,
    store::{Command, ErrorCode, LiveStore},
};
use serde_json::json;
use std::sync::atomic::AtomicBool;

#[test]
fn raw_watches_are_unavailable_to_remote_scopes() {
    let store = LiveStore::default();
    let input = json!({"id":"watch","op":"watch_areas","areas":["market"],"on":false});
    for name in ["reader", "local"] {
        let scope = Scope::restricted(name, vec![ClientGroup::Read], DeliveryMode::Auto);
        let command: Command = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(
            store.enqueue_scoped(command, 1, &scope).unwrap_err().code,
            ErrorCode::Forbidden
        );
        assert!(
            operations::call_scoped(
                "live_watch_areas",
                input.clone(),
                &store,
                &AtomicBool::new(false),
                &scope
            )
            .is_err()
        );
    }
}

#[test]
fn owners_cannot_release_each_others_watches_or_exceed_player_budget() {
    let store = LiveStore::default();
    store
        .sync(
            &serde_json::to_vec(&json!({"v":1,"boot":"b","seq":1,
        "watched_areas":[], "areas":[{"area":"players","full":true,
        "cre":[{"id":"a","pc":true}]}]}))
            .unwrap(),
            0,
        )
        .unwrap();
    store.set_watched_areas_owned("first", &["shared".into()], 1).unwrap();
    store.set_watched_areas_owned("second", &["shared".into()], 1).unwrap();
    store.set_watched_areas_owned("first", &[], 1).unwrap();
    let areas: Vec<String> = (0..63).map(|i| format!("area{i}")).collect();
    assert!(store.set_watched_areas_owned("third", &areas, 1).unwrap_err().contains("64"));
    store.set_watched_areas_owned("third", &areas[..62], 1).unwrap();
    store.set_watched_areas_owned("second", &[], 1).unwrap();
    store.set_watched_areas_owned("third", &areas, 1).unwrap();
    let mut overlap = areas;
    overlap.push("players".into());
    store.set_watched_areas_owned("third", &overlap, 1).unwrap();
}
