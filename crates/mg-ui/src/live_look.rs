//! The viewport consumes appearance data, not the engine's free-text fields.
//! Older reports omitted the UTC file tag or carried text outside the module's
//! legacy codepage. Neither should hide an otherwise valid creature preview.
use serde_json::Value;

pub(crate) fn visual_json(look: &Value) -> Value {
    let mut visual = look.clone();
    if let Some(root) = visual.as_object_mut() {
        // GFF file types are four bytes, including the trailing space.
        root.entry("__data_type").or_insert_with(|| "UTC ".into());
    }
    strip_text(&mut visual);
    visual
}

fn strip_text(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.retain(|_, field| {
                !matches!(field["type"].as_str(), Some("cexostring" | "cexolocstring"))
            });
            for field in fields.values_mut() {
                strip_text(field);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(strip_text),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_core::Codepage;
    use serde_json::json;

    #[test]
    fn old_npc_without_file_tag_keeps_wide_parts_and_equipment() {
        let raw = json!({
            "__struct_id":0,
            "Appearance_Type":{"type":"word","value":6},
            "xArmorPart_RFoot":{"type":"dword","value":511},
            "Equip_ItemList":{"type":"list","value":[{
                "__struct_id":16,
                "TemplateResRef":{"type":"resref","value":"nw_wswls001"},
                "ModelPart1":{"type":"byte","value":61}
            }]}
        });
        assert!(mg_gff::from_json(&raw, Codepage::default()).is_err());
        let parsed = mg_gff::from_json(&visual_json(&raw), Codepage::default()).unwrap();
        assert_eq!(parsed.file_type, *b"UTC ");
        assert_eq!(parsed.root.integer("xArmorPart_RFoot"), Some(511));
        let item = &parsed.root.list("Equip_ItemList").unwrap()[0];
        assert_eq!(item.id, 16);
        assert_eq!(item.resref("TemplateResRef").unwrap().as_str(), Some("nw_wswls001"));
        assert_eq!(item.integer("ModelPart1"), Some(61));
        assert!(raw.get("__data_type").is_none(), "the stored report is unchanged");
    }

    #[test]
    fn pc_and_nested_item_text_cannot_reject_a_visual_look() {
        let raw = json!({
            "__data_type":"UTC ", "__struct_id":0,
            "Appearance_Type":{"type":"word","value":6},
            "Description":{"type":"cexolocstring","id":4294967295u32,"value":{"0":"Wizard 🧙"}},
            "Equip_ItemList":{"type":"list","value":[{
                "__struct_id":16,"ModelPart1":{"type":"byte","value":61},
                "Name":{"type":"cexostring","value":"Sword 🗡"},
                "Nested":{"type":"struct","value":{
                    "__struct_id":7,"Description":{"type":"cexolocstring","id":4294967295u32,"value":{"0":"🧙"}}
                }}
            }]}
        });
        for codepage in [Codepage::default(), Codepage::WINDOWS_1250] {
            assert!(mg_gff::from_json(&raw, codepage).is_err());
            let parsed = mg_gff::from_json(&visual_json(&raw), codepage).unwrap();
            assert_eq!(parsed.root.integer("Appearance_Type"), Some(6));
            assert_eq!(
                parsed.root.list("Equip_ItemList").unwrap()[0].integer("ModelPart1"),
                Some(61)
            );
        }
        let mut bad_number = raw;
        bad_number["Appearance_Type"]["value"] = json!(65536);
        assert!(mg_gff::from_json(&visual_json(&bad_number), Codepage::default()).is_err());
    }
}
