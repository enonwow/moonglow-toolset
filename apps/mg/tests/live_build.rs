//! CLI trust boundary: discovery, failures and no mutation of an authored source.
use mg_gff::Gff;
use mg_module::{Module, ModuleLocation};
use serde_json::Value;

#[test]
fn live_build_is_a_write_and_refuses_invalid_configuration_without_changing_source() {
    let described = crate::mg().args(["describe", "--json"]).output().unwrap();
    let schema: Value = serde_json::from_slice(&described.stdout).unwrap();
    let op =
        schema["commands"].as_array().unwrap().iter().find(|c| c["name"] == "live_build").unwrap();
    assert_eq!(op["writes"], true);
    assert_eq!(op["input"]["properties"]["capture_dialogue"]["type"], "boolean");
    let dir = mg_testkit::scratch_dir("live-build-cli");
    let source = dir.join("ours.mod");
    let mut m = Module::new();
    m.set_info(&Gff::new(*b"IFO ")).unwrap();
    m.save_as(&ModuleLocation::Archive(source.clone())).unwrap();
    let before = std::fs::read(&source).unwrap();
    let run = crate::mg()
        .args(["--json", "--root"])
        .arg(&dir)
        .arg("--no-user-dir")
        .arg("live-build")
        .arg(&source)
        .arg("--nwnx")
        .arg(dir.join("missing"))
        .output()
        .unwrap();
    assert!(!run.status.success());
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert!(report["error"].as_str().unwrap().contains("not a game install"), "{report}");
    assert_eq!(std::fs::read(&source).unwrap(), before);
    assert!(!dir.join("ours-live.mod").exists());
}

#[test]
fn live_build_cli_builds_archive_folder_and_nasher_without_mutating_inputs() {
    let Some(root) = std::env::var_os("MGLIVE_BUILD_RESOURCE_ROOT") else {
        return;
    };
    let nwnx = std::env::var_os("MGLIVE_BUILD_NWNX").unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-build/source.mod");
    let source = std::fs::read(&fixture).expect("run prepare_private_live_build first");
    let dir = mg_testkit::scratch_dir("live-build-cli-success");
    let archive = dir.join("ours.mod");
    std::fs::write(&archive, &source).unwrap();
    let folder = dir.join("folder");
    Module::open(&archive).unwrap().save_as(&ModuleLocation::Folder(folder.clone())).unwrap();
    let project = dir.join("project");
    let init = crate::mg().arg("init").arg(&archive).arg(&project).arg("--json").output().unwrap();
    assert!(init.status.success(), "{}", String::from_utf8_lossy(&init.stdout));
    for (i, input) in [&archive, &folder, &project].iter().enumerate() {
        let before = Module::open(input).unwrap().to_archive_bytes().unwrap();
        let run = crate::mg()
            .arg("--root")
            .arg(&root)
            .arg("--no-user-dir")
            .arg("--json")
            .arg("live-build")
            .arg(input)
            .arg("--nwnx")
            .arg(&nwnx)
            .arg("--out")
            .arg(dir.join(format!("copy-{i}.mod")))
            .arg("--capture-dialogue")
            .output()
            .unwrap();
        assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stdout));
        let report: Value = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["original_load"], "lb_original");
        assert_eq!(report["dialogues"].as_array().unwrap().len(), 1);
        assert_eq!(before, Module::open(input).unwrap().to_archive_bytes().unwrap());
    }
    assert_eq!(source, std::fs::read(archive).unwrap());
}
