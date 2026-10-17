//! Portable schemas are data, never code or names known to this adapter.
use serde_json::Value;

pub fn check(s: &Value, property: bool) -> Result<(), String> {
    let object = s.as_object().ok_or("Schema must be an object")?;
    let allowed = [
        "type",
        "properties",
        "required",
        "items",
        "enum",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "oneOf",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "description",
        "default",
        "additionalProperties",
    ];
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("Unsupported schema keyword".into());
    }
    if property && !s["description"].as_str().is_some_and(|s| !s.is_empty()) {
        return Err("Every property needs a description".into());
    }
    if let Some(alternatives) = s.get("oneOf") {
        let alternatives = alternatives.as_array().ok_or("oneOf must be an array")?;
        if alternatives.is_empty()
            || alternatives.len() > 8
            || object.keys().any(|k| !["oneOf", "description", "default"].contains(&k.as_str()))
        {
            return Err("Invalid oneOf schema".into());
        }
        for option in alternatives {
            check(option, false)?;
        }
        return Ok(());
    }
    match s["type"].as_str() {
        // (An opaque JSON value: explicitly any properties and none listed.)
        Some("object") if s["additionalProperties"] == true && s.get("properties").is_none() => {}
        Some("object") => {
            if s["additionalProperties"] != false {
                return Err("Object schemas must reject additional properties".into());
            }
            let props = s["properties"].as_object().ok_or("Object schema needs properties")?;
            for value in props.values() {
                check(value, true)?;
            }
            if s.get("required").is_some_and(|v| !v.is_array()) {
                return Err("required must be an array".into());
            }
            for name in s["required"].as_array().into_iter().flatten() {
                if !name.as_str().is_some_and(|n| props.contains_key(n)) {
                    return Err("Invalid required property".into());
                }
            }
        }
        Some("array") => check(&s["items"], false)?,
        Some("string" | "number" | "integer" | "boolean" | "null") => {}
        _ => return Err("Unsupported schema type".into()),
    }
    Ok(())
}
pub fn validate(s: &Value, v: &Value) -> Result<(), String> {
    if let Some(alternatives) = s["oneOf"].as_array() {
        return if alternatives.iter().filter(|s| validate(s, v).is_ok()).count() == 1 {
            Ok(())
        } else {
            Err("Value must match exactly one allowed type".into())
        };
    }
    let valid = match s["type"].as_str() {
        Some("object") => v.is_object(),
        Some("array") => v.is_array(),
        Some("string") => v.is_string(),
        Some("boolean") => v.is_boolean(),
        Some("number") => v.is_number(),
        Some("integer") => v.is_i64() || v.is_u64(),
        Some("null") => v.is_null(),
        _ => false,
    };
    if !valid {
        return Err(format!("Expected {}", s["type"]));
    }
    if let Some(obj) = v.as_object() {
        for key in s["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if !obj.contains_key(key) {
                return Err(format!("Missing {key}"));
            }
        }
        for (key, value) in obj.iter().filter(|_| s["additionalProperties"] != true) {
            let p = s["properties"].get(key).ok_or_else(|| format!("Unknown argument {key}"))?;
            validate(p, value).map_err(|e| format!("{key}: {e}"))?;
        }
    }
    if let Some(items) = v.as_array() {
        let n = items.len() as u64;
        if s["minItems"].as_u64().is_some_and(|min| n < min)
            || s["maxItems"].as_u64().is_some_and(|max| n > max)
        {
            return Err("Array length out of range".into());
        }
        for item in items {
            validate(&s["items"], item)?;
        }
    }
    if let Some(text) = v.as_str() {
        let n = text.chars().count() as u64;
        if s["minLength"].as_u64().is_some_and(|min| n < min)
            || s["maxLength"].as_u64().is_some_and(|max| n > max)
        {
            return Err("String length out of range".into());
        }
    }
    if let Some(n) = v.as_f64()
        && (s["minimum"].as_f64().is_some_and(|min| n < min)
            || s["maximum"].as_f64().is_some_and(|max| n > max)
            || s["exclusiveMinimum"].as_f64().is_some_and(|min| n <= min)
            || s["exclusiveMaximum"].as_f64().is_some_and(|max| n >= max))
    {
        return Err("Number out of range".into());
    }
    if s["enum"].as_array().is_some_and(|options| !options.contains(v)) {
        return Err("Value is not in enum".into());
    }
    Ok(())
}
pub fn name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 49
        && name.as_bytes()[0].is_ascii_lowercase()
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

#[cfg(test)]
mod live_schema_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn every_live_schema_is_discoverable() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/live-api.openapi.json");
        // The independent adapter can still be copied and tested outside this workspace.
        if !path.is_file() {
            return;
        }
        let api: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for (path, operation) in api["paths"].as_object().unwrap() {
            check(
                &operation["post"]["requestBody"]["content"]["application/json"]["schema"],
                false,
            )
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        }
    }
    #[test]
    fn alternatives_and_exclusive_bounds_are_enforced() {
        let faction = json!({"description":"Faction.","oneOf":[{"type":"string","enum":["merchant"]},{"type":"integer","minimum":0,"maximum":65535}]});
        check(&faction, true).unwrap();
        for value in [json!("merchant"), json!(2)] {
            validate(&faction, &value).unwrap();
        }
        for value in [json!("none"), json!(2.5), json!(true), json!(65536)] {
            assert!(validate(&faction, &value).is_err());
        }
        let range = json!({"type":"number","exclusiveMinimum":0,"exclusiveMaximum":360});
        assert!(validate(&range, &json!(0)).is_err());
        assert!(validate(&range, &json!(360)).is_err());
        validate(&range, &json!(359.9)).unwrap();
        // An opaque object (a captured GFF snapshot): any keys; a listed object stays strict.
        let opaque = json!({"type":"object","description":"Snapshot.","additionalProperties":true});
        check(&opaque, true).unwrap();
        validate(&opaque, &json!({"anything": [1, 2]})).unwrap();
        assert!(validate(&opaque, &json!([1])).is_err());
        assert!(
            check(&json!({"type":"object","additionalProperties":true,"properties":{}}), false)
                .is_err()
        );
        let points = json!({"type":"array","items":{"type":"number"},"minItems":2,"maxItems":3});
        check(&points, false).unwrap();
        validate(&points, &json!([1, 2])).unwrap();
        assert!(
            validate(&points, &json!([1])).is_err()
                && validate(&points, &json!([1, 2, 3, 4])).is_err()
        );
        // A value that may be cleared: null is one of its alternatives.
        let clearable = json!({"description":"Value.","oneOf":[{"type":"string"},{"type":"null"}]});
        check(&clearable, true).unwrap();
        validate(&clearable, &json!(null)).unwrap();
        assert!(validate(&json!({"type":"null"}), &json!(0)).is_err());
        assert!(check(&json!({"oneOf":[]}), false).is_err());
        assert!(check(&json!({"oneOf":[{"$ref":"x"}]}), false).is_err());
    }
}
