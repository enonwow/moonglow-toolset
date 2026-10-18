use mg_live::events::Event;
use serde_json::json;

#[test]
fn event_round_trip_preserves_metadata_and_arbitrary_payload() {
    for (area, id, target) in [("market", Some("7f01"), Some("7f02")), ("", None, None)] {
        let event = Event {
            n: u64::MAX,
            at_ms: 123456789,
            kind: "custom_event".into(),
            area: area.into(),
            id: id.map(str::to_owned),
            name: "A name with Unicode: \u{17b}aneta".into(),
            target: target.map(str::to_owned),
            data: json!({"text":"hello", "nested":[null, true, {"damage":12.5}], "seq":u64::MAX}),
        };
        let bytes = serde_json::to_vec(&event).unwrap();
        let decoded: Event = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, event);
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
    }
}
