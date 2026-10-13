//! `mg nui`, on a module made here: a window is added, checked and its
//! scripts written and compiled; `validate` changes nothing and reports as
//! `mg verify` does; what fails, and `--dry-run`, leave the module as it
//! was.

use std::path::{Path, PathBuf};

use mg_gff::Gff;
use mg_module::{Module, ModuleLocation};
use mg_resman::ResKey;
use serde_json::{Value, json};

/// An empty module in a scratch folder of its own.
fn module(test: &str) -> PathBuf {
    let path = mg_testkit::scratch_dir(test).join("keep.mod");
    let mut m = Module::new();
    m.set_info(&Gff::new(*b"IFO ")).unwrap();
    m.save_as(&ModuleLocation::Archive(path.clone())).unwrap();
    path
}

fn key(name: &str) -> ResKey {
    ResKey::from_filename(name).unwrap()
}

fn output(root: Option<&Path>, args: &[&str]) -> std::process::Output {
    let mut mg = crate::mg();
    if let Some(root) = root {
        mg.arg("--root").arg(root);
    }
    mg.arg("--no-user-dir").args(args).output().unwrap()
}

/// `mg --json nui …`: whether it succeeded, and what it printed. Nothing
/// goes to standard error.
fn mg(root: Option<&Path>, args: &[&str]) -> (bool, Value) {
    let args: Vec<&str> = ["--json", "nui"].iter().chain(args).copied().collect();
    let out = output(root, &args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.trim().is_empty(), "{args:?}: stderr {stderr}");
    let json = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{args:?}: {e}: {}", String::from_utf8_lossy(&out.stdout)));
    (out.status.success(), json)
}

#[test]
fn a_new_window_is_saved_and_passes_the_checks() {
    let path = module("mg-nui-new");
    let p = path.to_str().unwrap();
    let (ok, made) = mg(None, &["new", p, "shop"]);
    assert!(ok, "{made}");
    assert_eq!(made["resources"], json!(["shop.jui", "shop.txt"]));
    assert_eq!((&made["saved"], &made["dry_run"]), (&json!(true), &json!(false)));
    let m = Module::open(&path).unwrap();
    assert!(m.contains(&key("shop.jui")) && m.contains(&key("shop.txt")));

    let (ok, checked) = mg(None, &["validate", p, "shop"]);
    assert!(ok, "{checked}");
    assert_eq!((&checked["errors"], &checked["warnings"]), (&json!(0), &json!(0)));
    assert_eq!(checked["findings"], json!([]));
}

#[test]
fn new_refuses_a_window_the_module_has_and_a_name_too_long() {
    let path = module("mg-nui-refused");
    let p = path.to_str().unwrap();
    assert!(mg(None, &["new", p, "shop"]).0);
    let before = std::fs::read(&path).unwrap();
    let (ok, again) = mg(None, &["new", p, "shop"]);
    assert!(!ok && again["error"].as_str().unwrap().contains("shop.jui"), "{again}");
    // (Fourteen characters at most: the scripts' names add two.)
    let (ok, long) = mg(None, &["new", p, "a_name_of_15_ch"]);
    assert!(!ok && long["error"].as_str().unwrap().contains("a_name_of_15_ch"), "{long}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn validate_reports_problems_as_verify_does_and_changes_nothing() {
    let path = module("mg-nui-validate");
    let p = path.to_str().unwrap();
    let mut m = Module::open(&path).unwrap();
    m.set(
        key("broken.jui"),
        br#"{"version": 2, "root": {"type": "row", "children": []}}"#.to_vec(),
    );
    m.save().unwrap();
    let before = std::fs::read(&path).unwrap();

    let (ok, checked) = mg(None, &["validate", p, "broken"]);
    assert!(!ok, "{checked}");
    assert!(checked["errors"].as_u64().unwrap() >= 1, "{checked}");
    let findings = checked["findings"].as_array().unwrap();
    assert_eq!(
        findings.len() as u64,
        checked["errors"].as_u64().unwrap() + checked["warnings"].as_u64().unwrap()
    );
    let version = findings.iter().find(|f| f["path"] == "/version").unwrap();
    assert_eq!(version["severity"], "error");
    assert!(version["message"].as_str().unwrap().contains("version 1"));
    assert_eq!(std::fs::read(&path).unwrap(), before);

    // For people: a line each on standard output, and the failure said.
    let out = output(None, &["nui", "validate", p, "broken"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().any(|l| l.starts_with("error\t/version: ")), "{text}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("broken.jui has errors"));

    let (ok, missing) = mg(None, &["validate", p, "nowhere"]);
    assert!(!ok && missing["error"].as_str().unwrap().contains("nowhere.jui"), "{missing}");
}

#[test]
fn validate_checks_a_window_of_any_name() {
    let path = module("mg-nui-any-name");
    let p = path.to_str().unwrap();
    assert!(mg(None, &["new", p, "shop"]).0);
    // Sixteen characters: a name `new` would not give one.
    let mut m = Module::open(&path).unwrap();
    for ext in ["jui", "txt"] {
        let data = m.get(&key(&format!("shop.{ext}"))).unwrap().to_vec();
        m.set(key(&format!("a_name_of_16_chr.{ext}")), data);
    }
    m.save().unwrap();
    let (ok, checked) = mg(None, &["validate", p, "a_name_of_16_chr"]);
    assert!(ok && checked["errors"] == 0, "{checked}");
    // A name no resource has is an error said, not a crash.
    let (ok, bad) = mg(None, &["validate", p, "a_name_longer_than_16"]);
    assert!(!ok && bad["error"].is_string(), "{bad}");
}

#[test]
fn generate_writes_the_scripts_and_compiles_the_event_script() {
    let root = mg_testkit::corpus!();
    let path = module("mg-nui-generate");
    let p = path.to_str().unwrap();
    assert!(mg(None, &["new", p, "shop"]).0);
    let (ok, made) = mg(Some(&root), &["generate", p, "shop"]);
    assert!(ok, "{made}");
    assert_eq!(made["saved"], true);
    let resources = made["resources"].as_array().unwrap();
    assert!(resources.contains(&json!("shop_o.nss")) && resources.contains(&json!("shop_e.ncs")));
    // The opener is an include: it has no compiled script of its own.
    let m = Module::open(&path).unwrap();
    assert!(m.contains(&key("shop_o.nss")) && m.contains(&key("shop_e.nss")));
    assert!(m.contains(&key("shop_e.ncs")) && !m.contains(&key("shop_o.ncs")));

    // For people: the resources and a summary on standard output, and
    // nothing on standard error.
    let out = output(Some(&root), &["nui", "generate", p, "shop", "--dry-run"]);
    assert!(out.status.success());
    assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().last().unwrap().starts_with("shop: would change "), "{text}");
}

#[test]
fn a_second_generate_changes_nothing() {
    let root = mg_testkit::corpus!();
    let path = module("mg-nui-again");
    let p = path.to_str().unwrap();
    assert!(mg(None, &["new", p, "shop"]).0);
    assert!(mg(Some(&root), &["generate", p, "shop"]).0);
    let before = std::fs::read(&path).unwrap();
    let (ok, again) = mg(Some(&root), &["generate", p, "shop"]);
    assert!(ok, "{again}");
    assert_eq!((&again["resources"], &again["saved"]), (&json!([]), &json!(false)));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_generate_that_fails_saves_nothing() {
    let root = mg_testkit::corpus!();
    let path = module("mg-nui-failed");
    let p = path.to_str().unwrap();
    assert!(mg(None, &["new", p, "shop"]).0);
    assert!(mg(Some(&root), &["generate", p, "shop"]).0);
    // The event script, written over with one that isn't the window's.
    let mut m = Module::open(&path).unwrap();
    m.set(key("shop_e.nss"), b"void main() { InvalidFunction(); }".to_vec());
    m.save().unwrap();
    let before = std::fs::read(&path).unwrap();
    let (ok, failed) = mg(Some(&root), &["generate", p, "shop"]);
    assert!(!ok, "{failed}");
    let error = failed["error"].as_str().unwrap();
    assert!(error.contains("keep.mod: window shop: "), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_dry_run_lists_the_resources_and_writes_nothing() {
    let root = mg_testkit::corpus!();
    let path = module("mg-nui-dry-run");
    let p = path.to_str().unwrap();
    let empty = std::fs::read(&path).unwrap();
    let (ok, new) = mg(None, &["new", p, "shop", "--dry-run"]);
    assert!(ok, "{new}");
    assert_eq!(new["resources"], json!(["shop.jui", "shop.txt"]));
    assert_eq!((&new["saved"], &new["dry_run"]), (&json!(false), &json!(true)));
    assert_eq!(std::fs::read(&path).unwrap(), empty);

    assert!(mg(None, &["new", p, "shop"]).0);
    let before = std::fs::read(&path).unwrap();
    let (ok, made) = mg(Some(&root), &["generate", p, "shop", "--dry-run"]);
    assert!(ok, "{made}");
    assert!(made["resources"].as_array().unwrap().contains(&json!("shop_e.ncs")), "{made}");
    assert_eq!((&made["saved"], &made["dry_run"]), (&json!(false), &json!(true)));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn code_that_does_not_compile_in_a_handlers_section_saves_nothing() {
    let root = mg_testkit::corpus!();
    let path = module("mg-nui-bad-section");
    let p = path.to_str().unwrap();
    assert!(mg(None, &["new", p, "shop"]).0);
    assert!(mg(Some(&root), &["generate", p, "shop"]).0);
    // Bad code between the markers of the Close button's handler, a
    // change to the window, and a script that includes the opener: none
    // of what Build would write for them may be saved.
    let mut m = Module::open(&path).unwrap();
    let script = String::from_utf8(m.get(&key("shop_e.nss")).unwrap().to_vec()).unwrap();
    let broken =
        mg_nui::insert_code(&script, "click mg_close", "        NoSuchFunction(oPlayer);\n")
            .unwrap();
    m.set(key("shop_e.nss"), broken.into_bytes());
    let mut window: Value = serde_json::from_slice(m.get(&key("shop.jui")).unwrap()).unwrap();
    window["title"] = json!("Another title");
    m.set(key("shop.jui"), serde_json::to_vec(&window).unwrap());
    m.set(
        key("my_enter.nss"),
        b"#include \"shop_o\"\nvoid main() { Open_shop(GetEnteringObject()); }\n".to_vec(),
    );
    m.save().unwrap();
    let before = std::fs::read(&path).unwrap();
    let (ok, failed) = mg(Some(&root), &["generate", p, "shop"]);
    assert!(!ok, "{failed}");
    let error = failed["error"].as_str().unwrap();
    assert!(error.contains("keep.mod: window shop: ") && error.contains("shop_e.nss("), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!Module::open(&path).unwrap().contains(&key("my_enter.ncs")));
}

#[test]
fn a_script_that_includes_the_opener_and_fails_is_printed_and_the_window_is_built() {
    let root = mg_testkit::corpus!();
    let path = module("mg-nui-bad-caller");
    let p = path.to_str().unwrap();
    assert!(mg(None, &["new", p, "shop"]).0);
    let mut m = Module::open(&path).unwrap();
    m.set(
        key("my_broken.nss"),
        b"#include \"shop_o\"\nvoid main() { Open_shop(NoSuchFunction()); }\n".to_vec(),
    );
    m.set(key("my_broken.ncs"), b"NCS as it was".to_vec());
    m.set(
        key("my_enter.nss"),
        b"#include \"shop_o\"\nvoid main() { Open_shop(GetEnteringObject()); }\n".to_vec(),
    );
    m.save().unwrap();
    // As `mg compile` does of a script that fails: said, the rest saved.
    let (ok, made) = mg(Some(&root), &["generate", p, "shop"]);
    assert!(ok, "{made}");
    assert_eq!((&made["saved"], &made["failed"]), (&json!(true), &json!(1)), "{made}");
    assert_eq!(made["errors"][0]["script"], "my_broken", "{made}");
    let resources = made["resources"].as_array().unwrap();
    assert!(resources.contains(&json!("my_enter.ncs")), "{made}");
    assert!(!resources.contains(&json!("my_broken.ncs")), "{made}");
    let m = Module::open(&path).unwrap();
    assert!(m.contains(&key("shop_e.ncs")) && m.contains(&key("my_enter.ncs")));
    assert_eq!(m.get(&key("my_broken.ncs")).unwrap(), b"NCS as it was");
    // For people: the compiler's line on standard output.
    let out = output(Some(&root), &["nui", "generate", p, "shop", "--dry-run"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().any(|l| l.starts_with("my_broken.nss(")), "{text}");
}

/// What `mg nui generate` compiled, `mg compile` passes over: both keep
/// what they compiled in one place.
#[test]
fn compile_passes_over_what_generate_compiled() {
    let root = mg_testkit::corpus!();
    // (A module no earlier run compiled: what is remembered goes by its path.)
    let made = module("mg-nui-remembered");
    let stamp = std::time::SystemTime::UNIX_EPOCH.elapsed().unwrap().as_nanos();
    let path = made.with_file_name(format!("fresh{stamp}.mod"));
    std::fs::rename(&made, &path).unwrap();
    let p = path.to_str().unwrap();
    assert!(mg(None, &["new", p, "shop"]).0);
    assert!(mg(Some(&root), &["generate", p, "shop"]).0);
    let out = output(Some(&root), &["--json", "compile", p]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let compiled: Value = serde_json::from_slice(&out.stdout).unwrap();
    // (The opener is looked at, an include: the event script is not.)
    assert_eq!(
        (&compiled["scripts"], &compiled["unchanged"]),
        (&json!(1), &json!(1)),
        "{compiled}"
    );
}
