//! The single game-bytes -> Unicode boundary for state and server-log input.
//! Native JSON may already have replaced bytes using the wrong game charset;
//! explicit ASCII byte companions let us decode without reversing mojibake.
use mg_core::Codepage;
use serde_json::Value;

fn codepage(message: &Value) -> Codepage {
    match message["text_codepage"].as_str() {
        Some("cp1251") => Codepage::for_label("cp1251").expect("supported codepage"),
        Some("cp1252") => Codepage::WINDOWS_1252,
        _ => Codepage::WINDOWS_1250,
    }
}

fn decode(encoded: &str, cp: Codepage) -> Option<String> {
    if !encoded.is_ascii() || encoded.len() > 196_608 {
        return None;
    }
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut input = encoded.as_bytes().iter().copied();
    while let Some(b) = input.next() {
        bytes.push(match b {
            b'+' => b' ',
            b'%' => {
                let hi = (input.next()? as char).to_digit(16)?;
                let lo = (input.next()? as char).to_digit(16)?;
                ((hi << 4) | lo) as u8
            }
            other => other,
        });
    }
    // Module scripts sometimes contain UTF-8 literals alongside legacy GFF/TLK.
    // This policy is explicit; unmarked Unicode JSON is never decoded again.
    Some(match std::str::from_utf8(&bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => cp.decode(&bytes).into_owned(),
    })
}

fn fields(row: &mut Value, names: &[&str], cp: Codepage) {
    let Some(row) = row.as_object_mut() else { return };
    for name in names {
        let raw = format!("{name}_bytes");
        if let Some(text) = row.get(&raw).and_then(Value::as_str).and_then(|s| decode(s, cp)) {
            row.insert((*name).into(), Value::String(text));
        }
        row.remove(&raw);
    }
}

pub(crate) fn report(message: &mut Value) {
    let cp = codepage(message);
    if let Some(areas) = message.get_mut("areas").and_then(Value::as_array_mut) {
        for area in areas {
            for kind in [
                "cre",
                "plc",
                "doors",
                "items",
                "waypoints",
                "sounds",
                "encounters",
                "stores",
                "triggers",
            ] {
                if let Some(objects) = area.get_mut(kind).and_then(Value::as_array_mut) {
                    for object in objects {
                        fields(object, &["name", "tag"], cp);
                    }
                }
            }
        }
    }
    if let Some(factions) = message.get_mut("factions").and_then(Value::as_array_mut) {
        for faction in factions {
            fields(faction, &["name"], cp);
        }
    }
    if let Some(events) = message.get_mut("events").and_then(Value::as_array_mut) {
        for event in events {
            fields(event, &["name", "text", "by"], cp);
        }
    }
    if let Some(results) = message.get_mut("results").and_then(Value::as_array_mut) {
        for result in results {
            fields(result, &["by"], cp);
            if result["text_kind"] == "reply"
                && let Some(value) = result.get_mut("value")
            {
                fields(value, &["name", "text"], cp);
            }
            if result["text_kind"] == "dm_reply"
                && let Some(value) = result.get_mut("value")
            {
                fields(value, &["text"], cp);
            }
            if result["text_kind"] == "skill"
                && let Some(value) = result.get_mut("value")
            {
                fields(value, &["name"], cp);
            }
            // Only the inventory schema we produce; leave arbitrary chunk and
            // capture_object JSON values intact, even if they use these keys.
            if result["text_kind"] == "inventory"
                && let Some(items) = result
                    .get_mut("value")
                    .and_then(|v| v.get_mut("items"))
                    .and_then(Value::as_array_mut)
            {
                for item in items {
                    fields(item, &["name", "tag"], cp);
                    if let Some(uses) = item.get_mut("uses").and_then(Value::as_array_mut) {
                        for ability in uses {
                            fields(ability, &["name"], cp);
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn server_log(message: &mut Value) {
    fields(message, &["line"], codepage(message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dialog_nodes_use_the_speech_byte_boundary_without_changing_identity() {
        // Exact CP1250 byte companions measured from authored DLG/TLK actions.
        let mut frame = json!({"events":[
            {"kind":"dialog","id":"4","speaker":"4","target":"5","listener":"5","dialog":"mget_focus","node":0,"node_type":"entry","text":"damaged","text_bytes":"Naprawd%EA%3A+%B9%EA%B3%F1%F3%9C%BF%9F"},
            {"kind":"dialog","id":"5","speaker":"5","target":"4","listener":"4","dialog":"mget_focus","node":0,"node_type":"reply","text":"damaged","text_bytes":"Odpowied%9F%3A+%B9%EA%B3%F1%F3%9C%BF%9F"}
        ]});
        report(&mut frame);
        assert_eq!(frame["events"][0]["text"], "Naprawdę: ąęłńóśżź");
        assert_eq!(frame["events"][1]["text"], "Odpowiedź: ąęłńóśżź");
        assert_eq!(frame["events"][0]["speaker"], "4");
        assert_eq!(frame["events"][1]["listener"], "4");
        assert_eq!(frame["events"][1]["node_type"], "reply");
        assert!(frame["events"][0].get("text_bytes").is_none());
    }

    #[test]
    fn wire_text_demo_mixed_speech_uses_each_strings_original_bytes() {
        let phrases = ["Naprawdę sama widziałam", "Teraz już każdy o tym wie"];
        let mut events = Vec::new();
        for phrase in phrases {
            for bytes in [
                phrase.as_bytes().to_vec(),
                Codepage::WINDOWS_1250.encode(phrase).unwrap().into_owned(),
            ] {
                let raw: String = bytes.iter().map(|b| format!("%{b:02X}")).collect();
                events.push(json!({"text":"native mojibake", "text_bytes":raw}));
            }
        }
        // Native JsonGetString with an unspecified charset has already lost
        // these bytes. Never guess replacements or invent the speaker's words.
        events.push(json!({"text":"fallback","text_bytes":"Naprawd%C4?+sama+widzia%C5?am"}));
        let mut frame = json!({"events":events});
        report(&mut frame);
        for (index, phrase) in phrases.iter().enumerate() {
            assert_eq!(frame["events"][index * 2]["text"], *phrase);
            assert_eq!(frame["events"][index * 2 + 1]["text"], *phrase);
        }
        assert_eq!(frame["events"][4]["text"], "NaprawdÄ? sama widziaĹ?am");
    }

    #[test]
    fn wire_text_legacy_utf8_and_literal_percent_are_decoded_once() {
        let phrase = "ąęłńóśżź + %C4%85 \\\"";
        for bytes in
            [Codepage::WINDOWS_1250.encode(phrase).unwrap().to_vec(), phrase.as_bytes().to_vec()]
        {
            let raw: String = bytes.iter().map(|b| format!("%{b:02X}")).collect();
            assert_eq!(decode(&raw, Codepage::WINDOWS_1250).as_deref(), Some(phrase));
            let mut report = json!({"areas":[{"cre":[{"name":"damaged","name_bytes":raw,"tag":phrase}]}],
                "events":[{"name":phrase,"text":"damaged","text_bytes":raw}],
                "results":[{"value":{"text_bytes":raw,"text":"opaque"}}]});
            super::report(&mut report);
            assert_eq!(report["areas"][0]["cre"][0]["name"], phrase);
            assert_eq!(report["areas"][0]["cre"][0]["tag"], phrase);
            assert_eq!(report["events"][0]["text"], phrase);
            assert_eq!(report["results"][0]["value"]["text"], "opaque");
            assert!(report["events"][0].get("text_bytes").is_none());
            let once = report.clone();
            super::report(&mut report);
            assert_eq!(report, once);
            let mut line = json!({"line":"damaged","line_bytes":raw});
            server_log(&mut line);
            assert_eq!(line["line"], phrase);
        }
        for bad in ["%", "%A", "%GG", "ą"] {
            assert!(decode(bad, Codepage::WINDOWS_1250).is_none());
        }
        assert_eq!(
            decode("ordinary+ASCII", Codepage::WINDOWS_1250).as_deref(),
            Some("ordinary ASCII")
        );
        let mut old = json!({"events":[{"text":"już każdy"}]});
        super::report(&mut old);
        assert_eq!(old["events"][0]["text"], "już każdy");
        let mut inventory = json!({"results":[{"text_kind":"inventory","value":{"items":[{"name":"bad","name_bytes":"%B3%F3d%9F"}]}}]});
        super::report(&mut inventory);
        assert_eq!(inventory["results"][0]["value"]["items"][0]["name"], "łódź");
        let mut absent = json!({"v":1,"boot":"old"});
        let before = absent.clone();
        super::report(&mut absent);
        assert_eq!(absent, before, "missing optional arrays must stay missing");
        let mut western =
            json!({"text_codepage":"cp1252","line":"fallback","line_bytes":"%9Cuvre"});
        server_log(&mut western);
        assert_eq!(western["line"], "œuvre");
        let mut malformed = json!({"line":"valid Unicode ą","line_bytes":"%GG"});
        server_log(&mut malformed);
        assert_eq!(malformed["line"], "valid Unicode ą");
    }
}
