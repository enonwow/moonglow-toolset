//! Private native proof client. Started only by tools/live/test-events.py --phase rust_api.
//! Uses the production HTTPS API and Redis bridge; never launches a game server.
use mg_live::{LiveConfig, LiveService, store::LiveStore};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::{Duration, Instant},
};
#[path = "live_ops/chat_latency.rs"]
mod chat_latency;

struct Client {
    service: LiveService,
    tls: Arc<rustls::ClientConfig>,
}
impl Client {
    fn new(chunks: bool) -> Self {
        let mut service = LiveService::start(
            LiveConfig {
                enabled: true,
                allow_agents: true,
                api_port: 0,
                redis_port: 6392,
                prefix: "mglive-events".into(),
                keep_logs: false,
                keep_wsl: Some(false),
                policy: mg_live::store::Policy { chunks, ..Default::default() },
                ..LiveConfig::default()
            },
            Arc::new(LiveStore::default()),
            Arc::new(|| {}),
        )
        .unwrap();
        service
            .publish_descriptor(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/test-output/live-events/app-data"),
            )
            .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(rustls::pki_types::CertificateDer::from(service.certificate().to_vec())).unwrap();
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        Self { service, tls: Arc::new(tls) }
    }
    fn request(&self, op: &str, input: Value) -> Value {
        let socket = TcpStream::connect(("127.0.0.1", self.service.config().api_port)).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(35))).unwrap();
        socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let conn =
            rustls::ClientConnection::new(self.tls.clone(), "moonglow-live".try_into().unwrap())
                .unwrap();
        let mut tls = rustls::StreamOwned::new(conn, socket);
        let body = input.to_string(); // Actual UTF-8 API input, not pre-escaped by the test.
        write!(tls,"POST /v1/call/live_{op} HTTP/1.1\r\nHost: moonglow-live\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",self.service.config().token,body.len()).unwrap();
        tls.flush().unwrap();
        let mut response = String::new();
        tls.read_to_string(&mut response).unwrap();
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
    }
    fn call(&self, op: &str, input: Value) -> Value {
        let reply = self.request(op, input);
        assert_eq!(reply["ok"], true, "{op}: {reply}");
        reply["result"].clone()
    }
    fn command(&self, op: &str, mut input: Value) -> Value {
        input["area"] = json!("events");
        let queued = self.call(op, input);
        let result = self.call("wait", json!({"id":queued["queued"],"timeout_ms":20000}));
        assert_eq!(
            result["ok"],
            true,
            "{op}: {result}; status: {}",
            self.call("status", json!({}))
        );
        result
    }
    fn until(&self, mut get: impl FnMut() -> Value, condition: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            let value = get();
            if condition(&value) {
                return value;
            }
            assert!(Instant::now() < deadline, "Timed out awaiting API observation: {value}");
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    fn object(&self, id: &Value) -> Value {
        let area = self.call("area_state", json!({"area":"events"}));
        ["creatures", "placeables", "doors", "items"]
            .iter()
            .flat_map(|k| area[k].as_array().into_iter().flatten())
            .find(|o| o["id"] == *id)
            .cloned()
            .unwrap_or(Value::Null)
    }
}
fn main() {
    // No native Windows game access, even if someone invokes the proof manually.
    let root = std::env::var_os("NWN_ROOT").expect("Set NWN_ROOT to the non-existent test guard");
    assert!(!std::path::Path::new(&root).exists());
    let boot = std::env::args().nth(1).expect("Expected boot from private test-events harness");
    let (process_boot, module_epoch) = boot.split_once('-').unwrap_or((&boot, "0"));
    assert!(process_boot.len() == 32 && process_boot.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(module_epoch.parse::<i32>().is_ok());
    let mode = std::env::args().nth(2).unwrap_or_else(|| "ops".into());
    let client = Client::new(
        mode == "chunk_return"
            || mode == "delta"
            || mode == "module_polish"
            || mode == "mixed_speech",
    );
    let initial = client.until(
        || client.call("status", json!({})),
        |s| s["boot"] == boot && s["redis"]["connected"] == true,
    );
    if mode == "chat_latency" || mode == "chat_latency_before" {
        chat_latency::run(client, boot, mode == "chat_latency_before");
        return;
    }
    if mode == "mixed_speech" {
        let npc = client.command(
            "spawn",
            json!({"id":"mixed-npc","kind":"creature","r":"nw_bartender","x":3,"y":3,"z":0,"f":0}),
        )["obj"]
            .clone();
        let mut observed = Vec::new();
        for stage in 0..6 {
            let expected = if stage % 2 == 0 {
                "Naprawdę sama widziałam"
            } else {
                "Teraz już każdy o tym wie"
            };
            client.command("run_chunk", json!({"id":format!("mixed-mode-{stage}"),"code":format!("SetLocalInt(GetModule(), \"MGET_MIXED_STAGE\", {stage});")}));
            let cursor = client.call("events", json!({}))["last"].clone();
            client.command(
                "run_script",
                json!({"id":format!("mixed-speech-{stage}"),"target":npc,"script":"mget_mixed"}),
            );
            let named = client.until(|| client.object(&npc), |o| o["name"] == expected);
            let chat = client.until(
                || client.call("events", json!({"kinds":["chat"],"object":npc,"since":cursor})),
                |e| {
                    e["events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e["data"]["text"] == expected && e["name"] == expected)
                },
            );
            let log = client.until(
                || client.call("events", json!({"kinds":["server_log"],"since":cursor})),
                |e| {
                    e["events"].as_array().unwrap().iter().any(|e| {
                        e["data"]["line"]
                            .as_str()
                            .is_some_and(|s| s.ends_with(&format!("MGET_MIXED {expected}")))
                    })
                },
            );
            let bytes=client.command("run_chunk",json!({"id":format!("mixed-bytes-{stage}"),"return":true,"code":"MgReturn(GetLocalString(GetModule(), \"MGET_MIXED_PROBE\"));"}))["value"].clone();
            assert_eq!(
                bytes["raw"], bytes["roundtrip"],
                "module JSON round trip must preserve original bytes"
            );
            observed.push(json!({"stage":stage,"expected":expected,"name":named,"chat":chat,"log":log,"bytes":bytes}));
        }
        let final_seq = client.call("status", json!({}))["seq"].clone();
        client.service.stop();
        println!(
            "{}",
            json!({"passed":true,"boot":boot,"final_seq":final_seq,"observed":observed})
        );
        return;
    }
    if mode == "module_polish" {
        let phrase = "Naprawdę: ąęłńóśżź";
        let npc = client.command(
            "spawn",
            json!({"id":"text-npc","kind":"creature","r":"nw_bartender","x":3,"y":3,"z":0,"f":0}),
        )["obj"]
            .clone();
        let plc = client.command(
            "spawn",
            json!({"id":"text-plc","kind":"placeable","r":"mget_polish","x":5,"y":5,"z":0,"f":0}),
        )["obj"]
            .clone();
        let placed =
            client.until(|| client.object(&plc), |o| o["name"] == phrase && o["tag"] == "Naprawd");
        let mut observed = Vec::new();
        for utf8 in [false, true] {
            let expected = if utf8 { format!("{phrase} + %C4%85") } else { phrase.into() };
            client.command("run_chunk",json!({"id":format!("text-mode-{utf8}"),"code":format!("SetLocalInt(GetModule(), \"MGET_POLISH_UTF8\", {});",i32::from(utf8))}));
            client.command(
                "run_script",
                json!({"id":format!("text-speech-{utf8}"),"target":npc,"script":"mget_polish"}),
            );
            let named = client.until(|| client.object(&npc), |o| o["name"] == expected);
            let chat = client.until(
                || client.call("events", json!({"kinds":["chat"],"object":npc})),
                |e| {
                    e["events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e["data"]["text"] == expected && e["name"] == expected)
                },
            );
            let log = client.until(
                || client.call("events", json!({"kinds":["server_log"]})),
                |e| {
                    e["events"].as_array().unwrap().iter().any(|e| {
                        e["data"]["line"]
                            .as_str()
                            .is_some_and(|s| s.ends_with(&format!("MGET_POLISH {expected}")))
                    })
                },
            );
            observed.push(
                json!({"utf8_literal":utf8,"expected":expected,"name":named,"chat":chat,"log":log}),
            );
        }
        let final_seq = client.call("status", json!({}))["seq"].clone();
        client.service.stop();
        println!(
            "{}",
            json!({"passed":true,"boot":boot,"final_seq":final_seq,"observed":observed,"placed_name_and_tag":placed})
        );
        return;
    }
    if mode == "delta" {
        for batch in 0..6 {
            let code = format!(
                r#"int i; for(i=0;i<100;i++) {{ int n=i+{}; object o=CreateObject(OBJECT_TYPE_PLACEABLE,"mget_chest",Location(OBJECT_SELF,Vector(IntToFloat(n%20),IntToFloat(n/20),0.0),0.0)); SetName(o,"Delta fixture "+IntToString(n)); }}"#,
                batch * 100
            );
            client
                .command("run_chunk", json!({"id":format!("delta-populate-{batch}"),"code":code}));
        }
        let area = client.until(
            || client.call("area_state", json!({"area":"events"})),
            |a| a["placeables"].as_array().unwrap().len() == 600,
        );
        client.until(
            || client.call("status", json!({})),
            |s| s["delta"] == true && s["resyncPending"] == false,
        );
        let first = area["placeables"][0]["id"].clone();
        let second = area["placeables"][1]["id"].clone();
        client
            .command("set_name", json!({"id":"delta-name","target":first,"name":"Delta changed"}));
        let changed = client.until(|| client.object(&first), |o| o["name"] == "Delta changed");
        client.command("destroy", json!({"id":"delta-remove","target":second}));
        let removed = client.until(
            || client.call("area_state", json!({"area":"events"})),
            |a| a["placeables"].as_array().unwrap().len() == 599,
        );
        assert!(!removed["placeables"].as_array().unwrap().iter().any(|o| o["id"] == second));
        let resyncs = client.call("status", json!({}))["resyncs"].as_u64().unwrap();
        client.command("run_chunk",json!({"id":"delta-gap","code":"SetLocalInt(GetModule(), \"MGLIVE_SEQ\", GetLocalInt(GetModule(), \"MGLIVE_SEQ\") + 1);"}));
        let recovered = client.until(
            || client.call("status", json!({})),
            |s| s["resyncs"].as_u64().unwrap() > resyncs && s["resyncPending"] == false,
        );
        assert_eq!(
            client.call("area_state", json!({"area":"events"}))["placeables"]
                .as_array()
                .unwrap()
                .len(),
            599
        );
        // Observe a periodic full snapshot, with no second resync forcing it.
        let end = Instant::now() + Duration::from_secs(42);
        let start_seq = recovered["seq"].as_u64().unwrap();
        loop {
            let status = client.call("status", json!({}));
            if status["seq"].as_u64().unwrap() >= start_seq + 31 {
                break;
            }
            assert!(Instant::now() < end, "No periodic full report");
            std::thread::sleep(Duration::from_millis(500));
        }
        let final_state = client.call("area_state", json!({"area":"events"}));
        assert!(final_state["fullAgeMs"].as_u64().unwrap() < 4000);
        assert_eq!(final_state["placeables"].as_array().unwrap().len(), 599);
        let status = client.call("status", json!({}));
        assert_eq!(status["resyncs"], recovered["resyncs"]);
        let evidence = json!({"passed":true,"boot":boot,"initial_seq":initial["seq"],"final_seq":status["seq"],"first":first,"second":second,"changed":changed,"resync":recovered,"final_count":599,"full_age_ms":final_state["fullAgeMs"]});
        client.service.stop();
        println!("{evidence}");
        return;
    }
    if mode == "chunk_return" {
        let mut results = Vec::new();
        for (index, code, wrap, expected) in [
            (
                0,
                r#"MgReturn(JsonDump(JsonObjectSet(JsonObject(), "answer", JsonInt(42))));"#,
                true,
                json!({"answer":42}),
            ),
            (1, r#"void main(){ MgReturn("7"); MgReturn("null"); }"#, false, Value::Null),
            (2, "int unused = 1;", true, Value::Null),
        ] {
            let result = client.command(
                "run_chunk",
                json!({"id":format!("api-return-{index}"),"code":code,"wrap":wrap,"return":true}),
            );
            assert_eq!(result.get("value"), Some(&expected));
            results.push(result);
        }
        for (index, code, expected) in [
            (3, r#"MgReturn("not JSON");"#, "invalid"),
            (
                4,
                r#"string s="x"; int i; for(i=0;i<15;i++)s+=s; MgReturn(JsonDump(JsonString(s)));"#,
                "invalid",
            ),
            (5, "not compiling NWScript", "compile_error"),
        ] {
            let queued=client.call("run_chunk",json!({"id":format!("api-return-{index}"),"area":"events","code":code,"return":true}));
            let result = client.call("wait", json!({"id":queued["queued"],"timeout_ms":20000}));
            assert_eq!(result["code"], expected);
            assert_eq!(result["ok"], false);
            assert!(result.get("value").is_none());
            results.push(result);
        }
        let result = client.command(
            "run_chunk",
            json!({"id":"api-return-after-errors","code":"int untouched = 1;","return":true}),
        );
        assert_eq!(result.get("value"), Some(&Value::Null));
        results.push(result);
        let result =
            client.command("run_chunk", json!({"id":"api-return-old","code":"int untouched = 1;"}));
        assert!(result.get("value").is_none());
        results.push(result);
        let final_seq = client.call("status", json!({}))["seq"].clone();
        client.service.stop();
        println!(
            "{}",
            json!({"passed":true,"boot":boot,"initial_seq":initial["seq"],"final_seq":final_seq,"results":results})
        );
        return;
    }
    let npc=client.command("spawn",json!({"id":"api-spawn-npc","kind":"creature","r":"nw_bartender","x":10,"y":10,"z":0,"f":0}))["obj"].clone();
    let before = client.until(|| client.object(&npc), |o| o["id"] == npc);
    assert_eq!(before["frozen"], false);
    let freeze = client.command("freeze", json!({"id":"api-freeze","target":npc,"seconds":60}));
    let frozen = client.until(|| client.object(&npc), |o| o["frozen"] == true);
    client.command("unfreeze", json!({"id":"api-unfreeze","target":npc}));
    let thawed = client.until(|| client.object(&npc), |o| o["frozen"] == false);
    assert_ne!(before["faction"], 3);
    let faction = client
        .command("set_faction", json!({"id":"api-faction","target":npc,"faction":"merchant"}));
    assert_eq!(faction["faction"], 3);
    let merchant = client.until(|| client.object(&npc), |o| o["faction"] == 3);
    let plc = client.command(
        "spawn",
        json!({"id":"api-spawn-plc","kind":"placeable","r":"mget_chest","x":5,"y":5,"z":0,"f":0}),
    )["obj"]
        .clone();
    let plain = client.until(|| client.object(&plc), |o| o["id"] == plc);
    assert!(plain["vt"].is_null());
    let vt = json!({"scale":1.5,"rotate":{"x":10.0,"y":20.0,"z":30.0},"translate":{"x":1.0,"y":2.0,"z":3.0}});
    let mut args = vt.clone();
    args["target"] = plc.clone();
    args["id"] = json!("api-transform");
    let transform = client.command("set_visual_transform", args);
    let transformed = client.until(|| client.object(&plc), |o| o["vt"] == vt);
    client.command("set_visual_transform", json!({"id":"api-reset","target":plc,"reset":true}));
    let reset = client.until(|| client.object(&plc), |o| o["vt"].is_null());
    let refused = client.request(
        "pc_give_gold",
        json!({"id":"api-refused-gold","area":"events","target":npc,"amount":25}),
    );
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["code"], "disabled");
    let sequence = client.call("status", json!({}))["seq"].as_u64().unwrap();
    client
        .until(|| client.call("status", json!({})), |s| s["seq"].as_u64().unwrap() > sequence + 1);
    let no_gold = client.object(&npc);
    assert_eq!(no_gold["gold"], before["gold"]);
    let polish = "Za\u{017c}\u{00f3}\u{0142}\u{0107} g\u{0119}\u{015b}l\u{0105} ja\u{017a}\u{0144}";
    client.command("set_name", json!({"id":"api-polish-name","target":npc,"name":polish}));
    let named = client.until(|| client.object(&npc), |o| o["name"] == polish);
    client.command(
        "npc_say",
        json!({"id":"api-polish-say","target":npc,"text":polish,"volume":"shout"}),
    );
    let chat = client.until(
        || client.call("events", json!({"kinds":["chat"],"object":npc})),
        |e| {
            e["events"].as_array().unwrap().iter().any(|e| {
                e["data"]["text"] == polish && e["name"] == polish && e["data"]["channel"] == 2
            })
        },
    );
    let final_status = client.call("status", json!({}));
    assert!(final_status["queued"].as_array().unwrap().is_empty());
    let evidence = json!({"passed":true,"boot":boot,"transport":"production LiveService HTTPS API -> Redis -> native NWScript -> Redis -> API",
        "local_players":false,"initial_seq":initial["seq"],"final_seq":final_status["seq"],
        "npc":npc,"placeable":plc,"before":before,"freeze_result":freeze,"frozen":frozen,"unfrozen":thawed,
        "faction_result":faction,"merchant":merchant,"placeable_before":plain,"transform_result":transform,"transformed":transformed,"reset":reset,
        "gold_refusal":refused,"after_refusal":no_gold,"polish_name":named,"polish_chat":chat});
    client.service.stop();
    println!("{}", serde_json::to_string(&evidence).unwrap());
}
