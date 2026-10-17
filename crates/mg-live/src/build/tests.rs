use super::*;
use mg_gff::{Gff, Struct};

#[test]
fn embedded_catalogue_covers_every_live_source() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/live/nwn");
    let mut disk: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("mglive_") && n.ends_with(".nss"))
        .collect();
    disk.sort();
    assert_eq!(disk, SOURCES.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>());
}

#[test]
fn missing_plugin_includes_are_reported_without_output_or_source_mutation() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output/live-build-missing");
    fs::create_dir_all(dir.join("Binaries")).unwrap();
    zip::ZipWriter::new(fs::File::create(dir.join("Binaries/NWScript.zip")).unwrap())
        .finish()
        .unwrap();
    let mut module = Module::new();
    module.set_info(&Gff::new(*b"IFO ")).unwrap();
    let bytes = module.to_archive_bytes().unwrap();
    let source = dir.join("source.mod");
    fs::write(&source, &bytes).unwrap();
    let request =
        Request { module: source.clone(), nwnx: dir.clone(), out: None, capture_dialogue: false };
    let report = prepare(&request, &GameInstall::new(&dir, None, "en")).unwrap();
    assert!(!report.ok);
    assert_eq!(report.errors.len(), NWNX_INCLUDES.len());
    assert!(report.errors.iter().all(|e| e.contains("Missing NWNX plugin include")));
    assert_eq!(bytes, fs::read(source).unwrap());
    assert!(!report.output.exists() && !report.settings.exists());
}

#[test]
fn dialogue_keeps_original_bytes_unknown_fields_and_mixed_encodings() {
    let mut gff = Gff::new(*b"DLG ");
    let mut entry = Struct::new(0);
    entry.set("Speaker", Value::String(vec![b'n', 0xb9]));
    entry.set("Script", Value::ResRef(b"original".to_vec()));
    entry.set("AlienBytes", Value::Void(vec![0xff, 0x80, 0, 1, 12]));
    entry.set("UTF8", Value::String("Zażółć".as_bytes().to_vec()));
    entry.set("CP1250", Value::String(vec![0xb9, 0xea, 0xb3, 0xf1]));
    let mut param = Struct::new(0);
    param.set("Key", Value::String(b"first".to_vec()));
    entry.set("ActionParams", Value::List(vec![param]));
    gff.root.set("EntryList", Value::List(vec![entry]));
    gff.root.set("ReplyList", Value::List(vec![Struct::new(0)]));
    let bytes = gff.to_bytes().unwrap();
    let mut m = Module::new();
    m.set(key("test.dlg"), bytes.clone());
    let changes = dialogue::instrument(&mut m, None).unwrap();
    assert_eq!(changes.len(), 2);
    let patched = m.get(&key("test.dlg")).unwrap();
    assert_eq!(&patched[56..bytes.len()], &bytes[56..]);
    let source =
        String::from_utf8_lossy(m.get(&key(&format!("{}.nss", changes[0].wrapper))).unwrap());
    assert!(source.contains("GetAsciiTableString(), 185, 1"));
    assert!(source.find("MgLiveDialogLine").unwrap() < source.find("ExecuteScript").unwrap());
    assert!(source.contains("GetScriptParam"));
    let previous = Manifest {
        scripts: m
            .keys_of(ResType::NSS)
            .map(|k| (k.to_string(), hash(m.get(k).unwrap())))
            .collect(),
        ..Default::default()
    };
    let before = m.get(&key("test.dlg")).unwrap().to_vec();
    assert!(dialogue::instrument(&mut m, Some(&previous)).unwrap().is_empty());
    assert_eq!(m.get(&key("test.dlg")).unwrap(), before);
}

#[test]
fn defaults_and_legacy_wrapper_do_not_nest() {
    assert_eq!(default_output(Path::new("/tmp/Quest.mod")), Path::new("/tmp/Quest-live.mod"));
    assert_eq!(
        default_output(Path::new("/tmp/Quest/nasher.cfg")),
        Path::new("/tmp/Quest-live.mod")
    );
    let mut m = Module::new();
    m.set(key("mglive_start.nss"), b"// Generated wrapper: preserve the base module's OnModuleLoad.\nvoid main()\n{\n    ExecuteScript(\"old_load\", OBJECT_SELF);\n    ExecuteScript(\"mglive_load\", OBJECT_SELF);\n}\n".to_vec());
    assert_eq!(legacy_original(&m).unwrap(), "old_load");
    m.set(key("mglive_start.nss"), b"void main() {}".to_vec());
    assert!(legacy_original(&m).is_err());
}

#[test]
fn generated_settings_watch_no_extra_areas_by_default() {
    let mut m = Module::new();
    let mut ifo = Gff::new(*b"IFO ");
    let mut area = Struct::new(6);
    area.set("Area_Name", Value::ResRef(b"many_areas".to_vec()));
    ifo.root.set("Mod_Area_list", Value::List(vec![area; 200]));
    m.set_info(&ifo).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR"));
    let settings = server::settings(path, &m, &GameInstall::new(path, None, "en")).unwrap();
    assert!(settings.lines().any(|line| line == "MGLIVE_AREAS=''"));
    assert!(!settings.contains("many_areas"));
    let launcher = include_str!("../../../../tools/live/Start-LiveDemo.ps1");
    assert!(!launcher.contains("$firstArea") && !launcher.contains("MGLIVE_DEMO_AREAS"));
}
