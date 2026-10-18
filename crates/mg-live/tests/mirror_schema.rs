use mg_live::{
    MIRROR_VERSION,
    events::Event,
    store::{AreaLive, CommandResult, LiveObject},
};
use serde_json::{Value, json};

fn declaration(source: &str, name: &str) -> String {
    let definition = source.find(&format!("pub struct {name} ")).unwrap();
    let start = source[..definition].rfind("#[derive").unwrap();
    let end = definition + source[definition..].find("\n}").unwrap() + 2;
    source[start..end]
        .lines()
        .map(|line| line.split("//").next().unwrap())
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[test]
fn mirror_schema_requires_a_versioned_fixture() {
    let path =
        format!("{}/tests/fixtures/mirror-v{MIRROR_VERSION}.json", env!("CARGO_MANIFEST_DIR"));
    let golden: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let store = include_str!("../src/store.rs");
    for (name, source) in [
        ("AreaLive", store),
        ("LiveObject", store),
        ("CommandResult", store),
        ("Event", include_str!("../src/events.rs")),
    ] {
        assert_eq!(
            golden[name],
            declaration(source, name),
            "{name}: bump MIRROR_VERSION and add its fixture"
        );
    }
}

#[test]
fn finished_state_and_result_states_round_trip_without_changing_game_keys() {
    let object: LiveObject =
        serde_json::from_value(json!({"id":"a1","r":"guard","f":1.5,"lookHash":42,"static":true}))
            .unwrap();
    let area = AreaLive {
        creatures: vec![object.clone()],
        has_full: true,
        seq: 7,
        full_ms: 100,
        received_ms: 110,
        ..Default::default()
    };
    assert_eq!(
        area,
        serde_json::from_value::<AreaLive>(serde_json::to_value(&area).unwrap()).unwrap()
    );
    let wire = serde_json::to_value(object).unwrap();
    assert_eq!(wire["r"], "guard");
    assert_eq!(wire["f"], 1.5);
    assert_eq!(wire["lookHash"], 42);
    assert_eq!(wire["static"], true);
    for row in [
        json!({"id":"a","pending":true}),
        json!({"id":"a","ok":true,"receipt":"boot:1"}),
        json!({"id":"a","code":"unknown"}),
        json!({"id":"a","value":null}),
    ] {
        let result: CommandResult = serde_json::from_value(row).unwrap();
        assert_eq!(
            result,
            serde_json::from_value::<CommandResult>(serde_json::to_value(&result).unwrap())
                .unwrap()
        );
    }
    let event: Event = serde_json::from_value(json!({"n":1,"at_ms":100,"kind":"chat","area":"square","id":null,"name":"Guard","target":null,"data":{"text":"Hello"}})).unwrap();
    assert_eq!(
        event,
        serde_json::from_value::<Event>(serde_json::to_value(&event).unwrap()).unwrap()
    );
}
