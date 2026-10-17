//! Prepare a new module archive for Live. No UI, shell compiler or repository
//! lookup at runtime; both front ends call this entry point.
mod dialogue;
mod scripts;
mod server;

use anyhow::{Context, Result, ensure};
use mg_core::ResType;
use mg_gff::Value;
use mg_module::Module;
use mg_resman::{GameInstall, ResKey, ResMan};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub use scripts::SOURCES;

pub const NWNX_INCLUDES: &[&str] = &[
    "nwnx_redis",
    "nwnx_redis_lib",
    "nwnx_redis_ps",
    "nwnx_util",
    "nwnx_creature",
    "nwnx_chat",
    "nwnx_damage",
    "nwnx_events",
    "nwnx_object",
    "nwnx_dialog",
    "nwnx_area",
];

#[derive(Debug, Clone)]
pub struct Request {
    pub module: PathBuf,
    /// Directory containing Binaries/NWScript.zip (read only).
    pub nwnx: PathBuf,
    pub out: Option<PathBuf>,
    pub capture_dialogue: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    pub ok: bool,
    pub output: PathBuf,
    pub settings: PathBuf,
    pub start_script: PathBuf,
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    pub original_load: String,
    pub module_ifo: String,
    pub dialogues: Vec<dialogue::Change>,
    pub compiled: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Manifest {
    owner: String,
    original_load: String,
    /// Hashes establish ownership before updating generated scripts.
    scripts: BTreeMap<String, String>,
}

pub fn default_output(module: &Path) -> PathBuf {
    let input = if module.file_name().is_some_and(|n| n == "nasher.cfg") {
        module.parent().unwrap_or(module)
    } else {
        module
    };
    let name = if input.is_dir()
        || !input
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("mod") || e.eq_ignore_ascii_case("nwm"))
    {
        input.file_name()
    } else {
        input.file_stem()
    };
    input.with_file_name(format!("{}-live.mod", name.unwrap_or_default().to_string_lossy()))
}

pub(crate) fn key(name: &str) -> ResKey {
    ResKey::from_filename(name).expect("static or validated resource name")
}

pub(crate) fn hash(data: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, data)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// All computation precedes publication. Existing output/sidecar files are
/// refused, including symlinks; the input is only ever opened for reading.
pub fn prepare(request: &Request, install: &GameInstall) -> Result<Report> {
    let output = request.out.clone().unwrap_or_else(|| default_output(&request.module));
    ensure!(
        output.extension().is_some_and(|e| e.eq_ignore_ascii_case("mod")),
        "Output must end in .mod"
    );
    let settings = output.with_extension("live.env");
    let start_script = output.with_extension("start.sh");
    for path in [&output, &settings, &start_script] {
        ensure!(
            fs::symlink_metadata(path).is_err(),
            "Output already exists: {}; choose a new output path",
            path.display()
        );
    }
    let mut module = Module::open(&request.module).context("Open source module")?;
    let mut report = Report { output, settings, start_script, ..Default::default() };
    if let Some(project) = &module.project {
        report.warnings.extend(project.warnings.iter().cloned());
    }
    if let Some(folder) = &module.folder {
        report.warnings.extend(folder.warnings.iter().cloned());
    }
    let mut resman = ResMan::for_game(install).context("Read game resources for compilation")?;
    module.haks()?;
    let (missing, wrong) = module.stack_onto(&mut resman, install);
    if let Some(e) = wrong {
        return Err(e.into());
    }
    report.warnings.extend(
        missing.into_iter().map(|h| {
            format!("Missing HAK: {h}; install it before compiling and running this module")
        }),
    );
    let before: BTreeMap<_, _> =
        module.keys().map(|k| (k.to_string(), hash(module.get(k).unwrap()))).collect();
    let previous: Option<Manifest> = module
        .get(&key("mglive_build.txt"))
        .map(serde_json::from_slice)
        .transpose()
        .context("Invalid mglive_build.txt ownership manifest")?;
    if let Some(p) = &previous {
        ensure!(p.owner == "moonglow-live-build/1", "mglive_build.txt belongs to another tool");
    }
    let mut info = module.info()?;
    let old_load = info.root.resref("Mod_OnModLoad").map(|r| r.to_string()).unwrap_or_default();
    let original = if old_load.eq_ignore_ascii_case("mglive_start") {
        if let Some(p) = &previous { p.original_load.clone() } else { legacy_original(&module)? }
    } else {
        old_load
    };
    ensure!(
        valid_resref(&original, true) && original != "mglive_start" && original != "mglive_load",
        "Invalid original OnModuleLoad: {original}"
    );
    report.original_load = original.clone();
    report.module_ifo = format!("Mod_OnModLoad: {original:?} -> mglive_start (original first)");
    let zip_path = request.nwnx.join("Binaries/NWScript.zip");
    let mut zip = zip::ZipArchive::new(
        fs::File::open(&zip_path).with_context(|| format!("Open {}", zip_path.display()))?,
    )?;
    for name in NWNX_INCLUDES {
        let filename = format!("{name}.nss");
        let data = match zip.by_name(&filename) {
            Ok(mut entry) => {
                use std::io::Read;
                ensure!(entry.size() <= 4 * 1024 * 1024, "NWNX include too large: {filename}");
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                data
            }
            Err(_) => {
                report.errors.push(format!(
                    "Missing NWNX plugin include: {filename} in {}",
                    zip_path.display()
                ));
                continue;
            }
        };
        put(&mut module, &filename, data, previous.as_ref(), &mut report);
    }
    if !report.errors.is_empty() {
        return Ok(report);
    }
    for (name, source) in SOURCES {
        put(&mut module, name, source.as_bytes().to_vec(), previous.as_ref(), &mut report);
    }
    let call = if original.is_empty() {
        String::new()
    } else {
        format!("    ExecuteScript(\"{original}\", OBJECT_SELF);\n")
    };
    let wrapper = format!(
        "// Generated wrapper: preserve the base module's OnModuleLoad.\nvoid main()\n{{\n{call}    ExecuteScript(\"mglive_load\", OBJECT_SELF);\n}}\n"
    );
    put(&mut module, "mglive_start.nss", wrapper.into_bytes(), previous.as_ref(), &mut report);
    info.root.set("Mod_OnModLoad", Value::ResRef(b"mglive_start".to_vec()));
    module.set_info(&info)?;
    if request.capture_dialogue {
        report.dialogues = dialogue::instrument(&mut module, previous.as_ref())?;
    } else if previous.is_some() {
        report.warnings.push("Existing dialogue instrumentation is preserved; omitting --capture-dialogue does not undo an earlier build".into());
    }
    // Compile serially: a preparation window must not saturate every core or
    // require an external compiler. Includes are checked by their callers.
    let names: Vec<_> = module.keys_of(ResType::NSS).copied().collect();
    for name in names {
        for result in mg_module::build::compile_named_scripts(&mut module, &resman, &[name], None) {
            if let Err(e) = result.result {
                report.errors.push(format!("{}: {e}", result.script));
            } else if module.contains(&ResKey::new(name.resref, ResType::NCS)) {
                report.compiled += 1;
            }
        }
    }
    if !report.errors.is_empty() {
        return Ok(report);
    }
    verify(&module, &resman, install, &mut report);
    if !report.errors.is_empty() {
        return Ok(report);
    }
    let manifest = Manifest {
        owner: "moonglow-live-build/1".into(),
        original_load: original,
        scripts: module
            .keys_of(ResType::NSS)
            .filter(|k| {
                k.resref.to_string().starts_with("mglive_")
                    || k.resref.to_string().starts_with("mgd_")
                    || k.resref.to_string().starts_with("nwnx_")
            })
            .map(|k| (k.to_string(), hash(module.get(k).unwrap())))
            .collect(),
    };
    module.set(key("mglive_build.txt"), serde_json::to_vec_pretty(&manifest)?);
    for k in module.keys() {
        let name = k.to_string();
        match before.get(&name) {
            None => report.added.push(name),
            Some(h) if *h != hash(module.get(k).unwrap()) => report.replaced.push(name),
            _ => {}
        }
    }
    let env = server::settings(&request.nwnx, &module, install)?;
    let start = server::start(&report.output, &report.settings)?;
    let archive = module.to_archive_bytes()?;
    // Reserve all three paths before writing any bytes. A competing build
    // cannot replace the source or another result between check and write.
    let outputs = [
        (&report.settings, env.as_bytes()),
        (&report.start_script, start.as_bytes()),
        (&report.output, archive.as_slice()),
    ];
    let mut created = Vec::new();
    let write = (|| -> Result<()> {
        let mut handles = Vec::new();
        for (path, data) in outputs {
            let f = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
            created.push(path);
            handles.push((f, data));
        }
        for (mut f, data) in handles {
            f.write_all(data)?;
            f.sync_all()?;
        }
        Ok(())
    })();
    if let Err(e) = write {
        for path in created {
            let _ = fs::remove_file(path);
        }
        return Err(e.context("Publish Live module copy"));
    }
    report.ok = true;
    Ok(report)
}

pub(crate) fn valid_resref(name: &str, empty: bool) -> bool {
    (empty || !name.is_empty())
        && name.len() <= 16
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Instrument a module in memory (also used by the private engine fixtures).
/// The caller decides whether/where to write it; no source paths are touched.
pub fn capture_dialogue(module: &mut Module) -> Result<Vec<dialogue::Change>> {
    let previous: Option<Manifest> =
        module.get(&key("mglive_build.txt")).map(serde_json::from_slice).transpose()?;
    dialogue::instrument(module, previous.as_ref())
}

fn put(m: &mut Module, name: &str, data: Vec<u8>, old: Option<&Manifest>, report: &mut Report) {
    if let Some(existing) = m.get(&key(name))
        && existing != data
        && !old.and_then(|p| p.scripts.get(name)).is_some_and(|h| *h == hash(existing))
    {
        report
            .warnings
            .push(format!("Script name clash: replacing {name} in the output copy only"));
    }
    m.set(key(name), data);
}

fn legacy_original(m: &Module) -> Result<String> {
    let bytes = m.get(&key("mglive_start.nss")).context("Existing mglive_start has no source/ownership manifest; cannot safely recover its original handler")?;
    let text = std::str::from_utf8(bytes)?;
    ensure!(
        text.starts_with("// Generated wrapper: preserve the base module's OnModuleLoad."),
        "Unrecognized mglive_start; cannot safely chain it"
    );
    let calls: Vec<_> = text
        .lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("ExecuteScript(\"")
                .and_then(|s| s.strip_suffix("\", OBJECT_SELF);"))
        })
        .collect();
    ensure!(
        calls.last() == Some(&"mglive_load") && calls.len() <= 2,
        "Unrecognized mglive_start calls"
    );
    Ok(if calls.len() == 2 { calls[0] } else { "" }.into())
}

fn verify(m: &Module, rm: &ResMan, gi: &GameInstall, report: &mut Report) {
    for finding in mg_module::verify::missing(m, rm) {
        let line = format!(
            "{}{}: missing {} {}",
            finding.reference.from,
            finding.reference.path,
            finding.reference.kind.name(),
            finding.reference.target
        );
        if finding.is_error() {
            report.errors.push(line);
        } else {
            report.warnings.push(line);
        }
    }
    let count = |data: Vec<u8>| mg_tlk::Tlk::read(&data).map(|t| t.entries.len()).ok();
    let base = fs::read(gi.talk_table(false)).ok().and_then(count).unwrap_or(0);
    let custom =
        m.custom_tlk().ok().flatten().and_then(|n| {
            mg_module::talk::find(rm, &gi.tlk_dirs(), &n).and_then(|f| count(f.data))
        });
    for f in mg_module::doctor::examine(m, rm, mg_module::doctor::TalkTables { base, custom }) {
        let line = format!("{}: {}{}: {}", f.check, f.resource, f.at, f.message);
        if f.severity == mg_module::doctor::Severity::Error {
            report.errors.push(line);
        } else {
            report.warnings.push(line);
        }
    }
}

#[cfg(test)]
mod tests;
