//! `mg init` and `mg build`: a shipped module put into a nasher project and
//! built from it gives the module's resources back, scripts compiled.

use mg_core::ResType;
use mg_module::Module;
use mg_resman::ResKey;

#[test]
fn init_then_build_gives_the_module_back() {
    let root = mg_testkit::corpus!();
    let module = root.join("data/mod/Neverwinter Chess.mod");
    if !module.is_file() {
        eprintln!("skipped: {} not found", module.display());
        return;
    }
    let dir = mg_testkit::scratch_dir("mg-nasher");
    let project = dir.join("chess");
    let run = |args: &[&str]| {
        let out = crate::mg().args(args).output().unwrap();
        assert!(out.status.success(), "mg {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    run(&["init", module.to_str().unwrap(), project.to_str().unwrap()]);
    assert!(project.join("nasher.cfg").is_file());
    // Lowercase names, as nasher's unpack writes them (the module has Module.ifo).
    assert!(project.join("src/module.ifo.json").is_file());
    assert!(project.join("src/bishop_b.utc.json").is_file());
    let out = dir.join("built.mod");
    run(&[
        "--root",
        root.to_str().unwrap(),
        "--no-user-dir",
        "build",
        project.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    let original = Module::open(&module).unwrap();
    let built = Module::open(&out).unwrap();
    // Compiled scripts without source (this module has one) aren't kept.
    let mut want: Vec<_> = original
        .keys()
        .filter(|k| {
            k.restype != ResType::NCS || original.contains(&ResKey::new(k.resref, ResType::NSS))
        })
        .copied()
        .collect();
    let mut have: Vec<_> = built.keys().copied().collect();
    want.sort();
    have.sort();
    assert_eq!(have, want, "the built module has the original's resources, scripts compiled");
    // --ndb: each compiled script's debug file is packed with it.
    let debug = dir.join("debug.mod");
    let (r, p, d) = (root.to_str().unwrap(), project.to_str().unwrap(), debug.to_str().unwrap());
    run(&["--root", r, "--no-user-dir", "build", p, "-o", d, "--ndb"]);
    let with = Module::open(&debug).unwrap();
    let count = |m: &Module, t| m.keys_of(t).count();
    assert_eq!(count(&built, ResType::NDB), 0);
    assert!(count(&with, ResType::NDB) > 10);
    assert_eq!(count(&with, ResType::NDB), count(&with, ResType::NCS));
}
