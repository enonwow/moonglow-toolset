use mg_live::gateway_frames::*;
use serde_json::{Value, json};
#[test]
fn golden_frames_cover_every_envelope_and_push_topic() {
    let frames: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/gateway-v1.json")).unwrap();
    assert_eq!(frames.len(), 18);
    for golden in frames {
        let bytes = serde_json::to_vec(&golden).unwrap();
        let encoded = if matches!(golden["t"].as_str(), Some("hello" | "req" | "ack")) {
            serde_json::to_value(decode_client(&bytes).unwrap()).unwrap()
        } else {
            let DecodedServer::Frame(frame) = decode_server(&bytes).unwrap() else {
                panic!("Known fixture skipped")
            };
            serde_json::to_value(frame).unwrap()
        };
        assert_eq!(encoded, golden);
    }
}
#[test]
fn decoding_is_bounded_and_unknown_server_frames_are_skipped() {
    for bytes in
        [b"".as_slice(), b"[", b"null", b"{}", b"{\"t\":\"res\",\"id\":1,\"ok\":true}", b"\xff"]
    {
        assert!(decode_server(bytes).is_err());
    }
    let fixture: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/gateway-v1.json")).unwrap();
    for row in fixture {
        let bytes = serde_json::to_vec(&row).unwrap();
        for cut in 0..bytes.len() {
            assert!(decode_server(&bytes[..cut]).is_err());
        }
    }
    assert!(decode_client(&vec![b' '; BOOK_FRAME_LIMIT + 1]).is_err());
    assert!(decode_server(&vec![b' '; SERVER_FRAME_LIMIT + 1]).is_err());
    let mut request = json!({"t":"req","id":1,"op":"book.put","a":{"value":"x".repeat(70_000)}});
    assert!(decode_client(&serde_json::to_vec(&request).unwrap()).is_ok());
    request["op"] = json!("cmds");
    assert!(decode_client(&serde_json::to_vec(&request).unwrap()).is_err());
    assert!(matches!(
        decode_server(br#"{"t":"future","secret":"ignored"}"#).unwrap(),
        DecodedServer::Ignored { .. }
    ));
    assert!(matches!(
        decode_server(br#"{"t":"push","topic":"future","body":{}}"#).unwrap(),
        DecodedServer::Ignored { .. }
    ));
    assert!(decode_client(br#"{"t":"ack","results":[],"event_n":0,"future":true}"#).is_ok());
    let deep = format!("{{\"t\":\"future\",\"a\":{}0{}}}", "[".repeat(200), "]".repeat(200));
    assert!(decode_server(deep.as_bytes()).is_err());
}
#[test]
fn handshake_versions_request_identity_and_inflight_limit() {
    let mut connection = Connection::default();
    let ack = ClientFrame::Ack(Ack { results: vec![], event_n: 0 });
    assert!(connection.receive(&ack).is_err());
    let hello = ClientFrame::Hello(Hello {
        proto: 1,
        mirror: mg_live::MIRROR_VERSION,
        client: "test".into(),
        resume: None,
        watch: vec![],
    });
    connection.receive(&hello).unwrap();
    assert!(connection.receive(&hello).is_err());
    for id in 0..64 {
        connection
            .receive(&ClientFrame::Req(Request { id, op: "status".into(), a: json!({}) }))
            .unwrap();
    }
    let extra = ClientFrame::Req(Request { id: 64, op: "future".into(), a: json!({}) });
    assert!(connection.receive(&extra).is_err());
    connection.complete(0);
    connection.receive(&extra).unwrap();
    assert!(
        connection
            .receive(&ClientFrame::Req(Request { id: 0, op: "status".into(), a: json!({}) }))
            .is_err()
    );
}

#[test]
fn unknown_and_mutating_calls_are_refused_by_core_dispatch() {
    use mg_live::{
        gateway::{Gateway, SessionConfig},
        store::LiveStore,
    };
    let gateway = Gateway::new(std::sync::Arc::new(LiveStore::default())).unwrap();
    gateway
        .connect(
            SessionConfig {
                id: "one".into(),
                id_prefix: "one.".into(),
                groups: vec!["read".into()],
                watch_limit: 16,
            },
            None,
            &[],
            0,
        )
        .unwrap();
    for (op, a) in [
        ("future", json!({})),
        ("call", json!({"op":"npc_say","input":{}})),
        ("call", json!({"op":"wait","input":{}})),
    ] {
        assert_eq!(
            gateway.handle("one", Request { id: 1, op: op.into(), a }, 0).result.unwrap_err().code,
            "unsupported"
        );
    }
}
