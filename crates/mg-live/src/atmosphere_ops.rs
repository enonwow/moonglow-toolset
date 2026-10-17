use super::{Handler, Operation, field, text};
use serde_json::{Value, json};
pub(super) fn operations() -> Vec<Operation> {
    let area = || text("Reported area resref.", 1, 16);
    let rgb = || json!({"type":"integer","minimum":0,"maximum":16777215,"description":"RGB integer 0xRRGGBB."});
    let period =
        || json!({"type":"string","enum":["day","night"],"description":"Day or night settings."});
    let fade = || json!({"type":"number","minimum":0,"maximum":30,"default":0,"description":"Client transition duration in seconds."});
    let row = || json!({"type":"integer","minimum":0,"maximum":65535,"description":"Row of the loaded 2DA; engine checks existence."});
    let action =
        || json!({"type":"string","enum":["play","stop"],"description":"Start or stop playback."});
    let mut ops = Vec::new();
    let mut add = |op: &'static str, description: &str, required: Vec<&str>, properties: Value| {
        ops.push(Operation{handler:Handler::Command(op),descriptor:json!({"name":format!("live_{op}"),"title":op.replace('_'," "),"group":"live","description":description,"readOnly":false,"destructive":true,"input":{"type":"object","additionalProperties":false,"required":required,"properties":properties}})});
    };
    add(
        "set_weather",
        "Set outdoor area weather, or all outdoor areas. default restores area settings.",
        vec!["area", "weather"],
        json!({"area":area(),"weather":{"type":"string","enum":["clear","rain","snow","default"],"description":"Outdoor weather; default restores the area configuration."},"all":{"type":"boolean","default":false,"description":"Apply weather to every outdoor area."}}),
    );
    add(
        "set_fog",
        "Change fog RGB and/or density. Returns native day/night settings.",
        vec!["area", "period"],
        json!({"area":area(),"period":{"type":"string","enum":["day","night","both"],"description":"Fog settings to change."},"color":rgb(),"amount":{"type":"integer","minimum":0,"maximum":100,"description":"Fog density percent."},"fade":fade()}),
    );
    add(
        "set_area_light",
        "Change day/night ambient and/or diffuse RGB light.",
        vec!["area", "period"],
        json!({"area":area(),"period":period(),"ambient":rgb(),"diffuse":rgb(),"fade":fade()}),
    );
    add(
        "set_skybox",
        "Set skyboxes.2da row.",
        vec!["area", "row"],
        json!({"area":area(),"row":row()}),
    );
    add(
        "set_music",
        "Set ambientmusic.2da track and/or play/stop. Background play uses current day/night slot.",
        vec!["area", "slot"],
        json!({"area":area(),"slot":{"type":"string","enum":["day","night","battle"],"description":"Music slot; playback uses current background time or battle."},"track":row(),"action":action()}),
    );
    add(
        "set_ambient_sound",
        "Set ambientsound.2da track, volume and/or play/stop.",
        vec!["area", "period"],
        json!({"area":area(),"period":period(),"track":row(),"volume":{"type":"integer","minimum":0,"maximum":127,"description":"Ambient sound volume."},"action":action()}),
    );
    add(
        "sound_at",
        "Play an installed sound from a temporary invisible helper at a point. Result confirms submission, not client audibility.",
        vec!["area", "resref", "x", "y", "z"],
        json!({"area":area(),"resref":text("WAV resource resref.",1,16),"x":field("number","X metres inside area."),"y":field("number","Y metres inside area."),"z":field("number","Z metres.")}),
    );
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Command, Policy};
    #[test]
    fn atmosphere_wire_and_validation() {
        let good = [
            json!({"op":"set_weather","weather":"rain","all":false}),
            json!({"op":"set_fog","period":"both","color":1193046,"amount":40,"fade":2.0}),
            json!({"op":"set_area_light","period":"night","ambient":255,"diffuse":65280}),
            json!({"op":"set_skybox","row":1}),
            json!({"op":"set_music","slot":"battle","track":2,"action":"play"}),
            json!({"op":"set_ambient_sound","period":"day","volume":100,"track":3}),
            json!({"op":"sound_at","resref":"as_wind1","x":5.0,"y":6.0,"z":0.0}),
        ];
        for mut wire in good {
            wire["area"] = json!("room");
            wire["id"] = json!("test");
            let command: Command = serde_json::from_value(wire.clone()).unwrap();
            command.validate().unwrap();
            let back = serde_json::to_value(command).unwrap();
            for (k, v) in wire.as_object().unwrap() {
                assert_eq!(back[k], *v, "{k}");
            }
            let op = wire.as_object_mut().unwrap().remove("op").unwrap();
            let descriptor = crate::operations::describe(Policy::default())["operations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["name"] == format!("live_{}", op.as_str().unwrap()))
                .unwrap()
                .clone();
            super::super::validate(&descriptor["input"], &wire).unwrap();
        }
        for bad in [
            json!({"op":"set_fog","period":"both"}),
            json!({"op":"set_fog","period":"both","amount":101}),
            json!({"op":"set_area_light","period":"day","ambient":16777216}),
            json!({"op":"set_skybox","row":65536}),
            json!({"op":"set_music","slot":"day"}),
            json!({"op":"set_ambient_sound","period":"night","volume":128}),
            json!({"op":"sound_at","resref":"../bad","x":0,"y":0,"z":0}),
        ] {
            let mut bad = bad;
            bad["area"] = json!("room");
            assert!(serde_json::from_value::<Command>(bad).unwrap().validate().is_err());
        }
    }
}
