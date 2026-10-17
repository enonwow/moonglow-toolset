//! Session-sized authored fixture; installed blueprints are read-only inputs.
use mg_core::ResType;
use mg_gff::{Gff, Struct, Value};
use mg_resman::{ResKey, ResMan};

pub(super) fn populate(module: &mut mg_module::Module, resman: &ResMan) {
    let key = |name: &str, kind| ResKey::parse(name, kind).unwrap();
    let area = Gff::read(module.get(&key("events", ResType::ARE)).unwrap()).unwrap();
    let git = Gff::read(module.get(&key("events", ResType::GIT)).unwrap()).unwrap();
    let creature = Gff::read(&resman.get(&key("nw_bartender", ResType::UTC)).unwrap()).unwrap();
    let equipped = Gff::read(&resman.get(&key("nw_bandit001", ResType::UTC)).unwrap()).unwrap();
    let placeable = Gff::read(module.get(&key("mget_chest", ResType::UTP)).unwrap()).unwrap();
    let mut area_rows = Vec::new();
    for a in 0..5 {
        let name = if a == 0 { "events".into() } else { format!("events{}", a + 1) };
        let mut ar = area.clone();
        ar.root.set("Tag", Value::String(name.as_bytes().to_vec()));
        ar.root.set("ResRef", Value::ResRef(name.as_bytes().to_vec()));
        ar.root.set("Width", Value::Int(10));
        ar.root.set("Height", Value::Int(10));
        let tile = ar.root.list("Tile_List").unwrap()[0].clone();
        ar.root.set("Tile_List", Value::List(vec![tile; 100]));
        module.set(key(&name, ResType::ARE), ar.to_bytes().unwrap());
        let mut g = git.clone();
        g.root.set("Door List", Value::List(Vec::new()));
        let mut creatures = Vec::new();
        let mut placeables = Vec::new();
        for n in 0..200 {
            let surrogate = n == 0 || (a == 0 && n == 1);
            let mut o = if surrogate {
                equipped.root.clone()
            } else if n < 50 {
                creature.root.clone()
            } else {
                placeable.root.clone()
            };
            o.id = if n < 50 { 4 } else { 9 };
            o.set("Tag", Value::String(format!("ST_{a}_{n}").into_bytes()));
            o.set(
                "TemplateResRef",
                Value::ResRef(if surrogate {
                    b"nw_bandit001".to_vec()
                } else if n < 50 {
                    b"nw_bartender".to_vec()
                } else {
                    b"mget_chest".to_vec()
                }),
            );
            let x = 5.0 + (n % 20) as f32 * 4.0;
            let y = 5.0 + (n / 20) as f32 * 7.0;
            if n < 50 {
                o.set("XPosition", Value::Float(x));
                o.set("YPosition", Value::Float(y));
                o.set("ZPosition", Value::Float(0.0));
                o.set("XOrientation", Value::Float(1.0));
                o.set("YOrientation", Value::Float(0.0));
                o.set("FactionID", Value::Word(2));
                // Own patrols run separately; no unrelated default AI scripts.
                let names: Vec<_> = o
                    .fields
                    .iter()
                    .filter(|f| f.label.as_bytes().starts_with(b"Script"))
                    .map(|f| f.label.to_string_lossy())
                    .collect();
                for name in names {
                    o.set(&name, Value::ResRef(Vec::new()));
                }
                creatures.push(o);
            } else {
                o.set("X", Value::Float(x));
                o.set("Y", Value::Float(y));
                o.set("Z", Value::Float(0.0));
                o.set("Bearing", Value::Float(0.0));
                placeables.push(o);
            }
        }
        g.root.set("Creature List", Value::List(creatures));
        g.root.set("Placeable List", Value::List(placeables));
        module.set(key(&name, ResType::GIT), g.to_bytes().unwrap());
        let mut entry = Struct::new(6);
        entry.set("Area_Name", Value::ResRef(name.into_bytes()));
        area_rows.push(entry);
    }
    let mut info = module.info().unwrap();
    info.root.set("Mod_Area_list", Value::List(area_rows));
    module.set_info(&info).unwrap();
}
