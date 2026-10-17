//! Opt-in measurement fixture. Read-only source MOD; never launches an engine.
use mg_core::ResType;
use mg_gff::Value;
use mg_resman::{GameInstall, ResKey, ResMan};
use serde_json::json;
use std::{fs, path::PathBuf};

#[test]
fn prepare_private_measure_fixture() {
    if std::env::var("MGLIVE_PREPARE_MEASURE").as_deref() != Ok("1") {
        return;
    }
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-measure")
        .canonicalize()
        .unwrap();
    assert_eq!(fs::read_to_string(out.join(".owner")).unwrap().trim(), "live-measure-v1");
    let input: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("inputs.json")).unwrap()).unwrap();
    let mut module =
        mg_module::Module::open(std::path::Path::new(input["module_windows"].as_str().unwrap()))
            .unwrap();
    // The heartbeat wrapper is authored from the adjacent read-only source.
    // Refuse stale source rather than timing routines different from the MOD.
    let source_dir = std::path::Path::new(input["module_windows"].as_str().unwrap())
        .parent()
        .unwrap()
        .join("src");
    for name in ["city_beat", "city_boot", "city_tick", "city_core", "city_modes"] {
        let key = ResKey::parse(name, ResType::NSS).unwrap();
        let embedded = module.get(&key).unwrap();
        let source = fs::read_to_string(source_dir.join(format!("{name}.nss"))).unwrap();
        assert_eq!(
            std::str::from_utf8(embedded).unwrap().replace("\r\n", "\n").trim(),
            source.replace("\r\n", "\n").trim(),
            "stale routine source: {name}"
        );
    }
    let mut info = module.info().unwrap();
    info.root.set("Mod_OnModLoad", Value::ResRef(b"mp_load".to_vec()));
    module.set_info(&info).unwrap();
    if input["full_scale"] == true {
        // A large loaded catalogue, written only to this disposable output MOD.
        let key = ResKey::parse("repute", ResType::FAC).unwrap();
        let mut factions = mg_gff::Gff::read(module.get(&key).unwrap()).unwrap();
        let mut rows = factions.root.list("FactionList").unwrap().to_vec();
        while rows.len() < 256 {
            let mut row = mg_gff::Struct::new(rows.len() as u32);
            row.set("FactionName", Value::String(format!("Scale {}", rows.len()).into_bytes()));
            row.set("FactionGlobal", Value::Word(1));
            row.set("FactionParentID", Value::Dword(u32::MAX));
            rows.push(row);
        }
        factions.root.set("FactionList", Value::List(rows));
        module.set(key, factions.to_bytes().unwrap());
    }
    let install = GameInstall::new(
        PathBuf::from(std::env::var_os("MGLIVE_RESOURCE_ROOT").unwrap()),
        Some(out.join("user")),
        "en",
    );
    let resman = ResMan::for_game(&install).unwrap();
    let dir = out.join("module");
    let mut scripts = Vec::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "nss") {
            let key =
                ResKey::parse(path.file_stem().unwrap().to_str().unwrap(), ResType::NSS).unwrap();
            module.set(key, fs::read(&path).unwrap());
            scripts.push(key);
        }
    }
    let mut errors = Vec::new();
    for script in scripts {
        for result in mg_module::build::compile_named_scripts(&mut module, &resman, &[script], None)
        {
            if let Err(e) = result.result {
                errors.push(format!("{}: {e}", result.script));
            }
        }
    }
    let result = json!({"failed":errors.len(), "errors":errors, "compiler_binary":std::env::current_exe().unwrap()});
    fs::write(out.join("compile.json"), serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    assert!(errors.is_empty(), "{result}");
    fs::write(out.join("events.mod"), module.to_archive_bytes().unwrap()).unwrap();
}
