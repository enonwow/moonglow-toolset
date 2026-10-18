use mg_live::store::LiveStore;

#[test]
fn report_limit_reserves_result_batch_headroom_and_remains_bounded() {
    let store = LiveStore::default();
    let mut body = br#"{"v":1,"boot":"budget","seq":1}"#.to_vec();
    body.resize(4 * 1024 * 1024 + 512 * 1024, b' ');
    assert!(store.sync(&body, 0).is_ok());
    body.resize(5 * 1024 * 1024 + 1, b' ');
    assert_eq!(store.sync(&body, 0).unwrap_err(), "Body too large");
}
