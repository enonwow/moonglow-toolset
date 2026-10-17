//! Explicit preparation proof using installed read-only declarations. No server
//! is launched here; tools/live/test-live-build.py runs the resulting owned copy.
use mg_core::ResType;
use mg_gff::{Gff, Struct, Value};
use mg_live::build::{Request, prepare};
use mg_module::{Module, ModuleLocation};
use mg_resman::{GameInstall, ResKey};
use std::{fs, path::PathBuf};

#[test]
fn prepare_private_live_build() {
    let Some(root) = std::env::var_os("MGLIVE_BUILD_RESOURCE_ROOT") else {
        return;
    };
    let nwnx = PathBuf::from(std::env::var_os("MGLIVE_BUILD_NWNX").expect("NWNX folder"));
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output/live-build");
    fs::create_dir_all(&out).unwrap();
    let key = |name| ResKey::from_filename(name).unwrap();
    let mut source = Module::new();
    let fixtures = out.parent().unwrap().join("live-events/module");
    let mut info = Gff::read(&fs::read(fixtures.join("module.ifo")).unwrap()).unwrap();
    info.root.set("Mod_OnModLoad", Value::ResRef(b"lb_original".to_vec()));
    info.root.set("Mod_CustomTlk", Value::String(Vec::new()));
    let mut area = Struct::new(6);
    area.set("Area_Name", Value::ResRef(b"events".to_vec()));
    info.root.set("Mod_Area_list", Value::List(vec![area]));
    source.set_info(&info).unwrap();
    source.set(key("events.are"), fs::read(fixtures.join("events.are")).unwrap());
    source.set(key("events.gic"), Gff::new(*b"GIC ").to_bytes().unwrap());
    source.set(key("events.git"), Gff::new(*b"GIT ").to_bytes().unwrap());
    source.set(key("lb_original.nss"), b"void main(){SetLocalInt(GetModule(),\"LIVE_BUILD_ORIGINAL\",123);WriteTimestampedLogEntry(\"LIVE_BUILD: original before Live=\"+IntToString(GetLocalInt(GetModule(),\"MGLIVE_STARTED\")));}\n".to_vec());
    let mut dlg = Gff::new(*b"DLG ");
    let mut entry = Struct::new(0);
    entry.set("Script", Value::ResRef(b"lb_original".to_vec()));
    entry.set("MixedBytes", Value::String(vec![0xb9, 0xea, 0xb3, 0xc4, 0x85]));
    dlg.root.set("EntryList", Value::List(vec![entry]));
    source.set_gff(key("lb_dialog.dlg"), &dlg).unwrap();
    let path = out.join("source.mod");
    // This exact fixture is ours; overwrite its one current input, no backups.
    fs::write(&path, source.to_archive_bytes().unwrap()).unwrap();
    let before = fs::read(&path).unwrap();
    let install = GameInstall::new(root, None, "en");
    let request = Request {
        module: path.clone(),
        nwnx,
        out: Some(out.join("prepared.mod")),
        capture_dialogue: true,
    };
    // Reruns remove only these three outputs recorded by this fixture, never
    // any other module or directory.
    for name in [
        "prepared.mod",
        "prepared.live.env",
        "prepared.start.sh",
        "updated.mod",
        "updated.live.env",
        "updated.start.sh",
    ] {
        let p = out.join(name);
        if p.exists() {
            assert!(!p.symlink_metadata().unwrap().file_type().is_symlink());
            fs::remove_file(p).unwrap();
        }
    }
    let report = prepare(&request, &install).unwrap();
    fs::write(out.join("build-report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    assert!(report.ok, "{:?}", report.errors);
    assert_eq!(before, fs::read(&path).unwrap());
    let built = Module::open(&report.output).unwrap();
    for name in ["mglive_load", "mglive_start", "mglive_tick", "mglive_ps"] {
        assert!(built.contains(&ResKey::parse(name, ResType::NCS).unwrap()));
    }
    let wrapper = String::from_utf8_lossy(built.get(&key("mglive_start.nss")).unwrap());
    assert!(wrapper.find("lb_original").unwrap() < wrapper.find("mglive_load").unwrap());
    let original_dlg = source.get(&key("lb_dialog.dlg")).unwrap();
    assert_eq!(
        &built.get(&key("lb_dialog.dlg")).unwrap()[56..original_dlg.len()],
        &original_dlg[56..]
    );
    let update = prepare(
        &Request {
            module: report.output.clone(),
            out: Some(out.join("updated.mod")),
            ..request.clone()
        },
        &install,
    )
    .unwrap();
    assert!(update.ok, "{:?}", update.errors);
    assert_eq!(update.original_load, "lb_original");
    assert!(update.dialogues.is_empty());
    let updated = Module::open(&update.output).unwrap();
    assert_eq!(built.get(&key("mglive_start.nss")), updated.get(&key("mglive_start.nss")));
    assert!(prepare(&request, &install).unwrap_err().to_string().contains("already exists"));
    let folder = out.join("folder");
    fs::create_dir_all(&folder).unwrap();
    source.save_as(&ModuleLocation::Folder(folder)).unwrap();
}
