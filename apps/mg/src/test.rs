//! `mg test`: a module's `test_*` scripts run in the game's server, in a
//! scratch copy of the module that gets the driver; the builder's module
//! is only read.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use mg_core::ResType;
use mg_module::testing::{self, Report};
use mg_module::{Module, ModuleLocation};
use mg_resman::{GameInstall, ResKey, ResMan};

use crate::serve;

pub(crate) struct Config {
    pub module: PathBuf,
    pub filter: Option<String>,
    /// Folders of more test scripts, put into the copy.
    pub tests: Vec<PathBuf>,
    /// The module runs its tests itself and says when they are done.
    pub own_driver: bool,
    pub settle: f32,
    pub timeout: f64,
    pub serve: serve::Config,
}

pub(crate) struct Run {
    pub report: Report,
    pub seconds: f64,
    pub name: String,
    /// What could not be compiled: nothing was run.
    pub compile_errors: Vec<String>,
    pub notes: Vec<String>,
    /// How the server ended, where that is why the run was cut short.
    pub ended: Option<String>,
}

/// The scratch copy's folder: one for a module, used again by its next
/// run.
fn scratch_dir(module: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let module = std::fs::canonicalize(module).unwrap_or_else(|_| module.to_path_buf());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    module.hash(&mut h);
    let stem = module.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    std::env::temp_dir().join("moonglow-test").join(format!("{stem}-{:08x}", h.finish() as u32))
}

pub(crate) fn run(
    gi: &GameInstall,
    c: &Config,
    resman: impl Fn(&Module, &mut Vec<String>) -> Result<ResMan>,
    echo: &mut dyn FnMut(&str),
) -> Result<Run> {
    let started = Instant::now();
    let mut notes = Vec::new();
    // (Changed in memory only, and saved elsewhere.)
    let mut m = Module::open(&c.module).with_context(|| c.module.display().to_string())?;
    for dir in &c.tests {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .with_context(|| dir.display().to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("nss")))
            .collect();
        files.sort();
        for file in files {
            let name = file.file_name().unwrap_or_default().to_string_lossy();
            let key = ResKey::from_filename(&name)
                .with_context(|| format!("{} is not a script's name", file.display()))?;
            m.set(key, std::fs::read(&file).with_context(|| file.display().to_string())?);
        }
    }
    let tests = match c.own_driver {
        true => Vec::new(),
        false => testing::test_scripts(&m, c.filter.as_deref()),
    };
    // The tests that say when their main() has returned.
    let mut returns = Vec::new();
    if !c.own_driver {
        if tests.is_empty() {
            bail!("no tests: the module has no test_* script with a main() that matches");
        }
        let (own, said) = testing::inject(&mut m, &tests, c.settle)?;
        returns = said;
        if own {
            notes.push(format!(
                "the module's own {}.nss is used, not Moonglow's",
                testing::INCLUDE_NAME
            ));
        }
    }
    // The tests, the driver, and whatever has no compiled script yet.
    let rm = resman(&m, &mut notes)?;
    let mut names = tests.clone();
    if !c.own_driver {
        names.push(ResKey::parse(testing::DRIVER_NAME, ResType::NSS).expect("a resource name"));
    }
    let uncompiled: Vec<ResKey> = (m.keys_of(ResType::NSS))
        .filter(|k| !names.contains(k) && !m.contains(&ResKey::new(k.resref, ResType::NCS)))
        .copied()
        .collect();
    names.extend(uncompiled);
    let compiled = mg_module::build::compile_named_scripts(&mut m, &rm, &names, None);
    let compile_errors: Vec<String> = (compiled.iter())
        .filter_map(|r| r.result.as_ref().err().map(|e| e.message.clone()))
        .collect();
    let name = serve::module_name(&c.module);
    if !compile_errors.is_empty() {
        let seconds = started.elapsed().as_secs_f64();
        let report = Report::default();
        return Ok(Run { report, seconds, name, compile_errors, notes, ended: None });
    }
    let dir = c.serve.dir.clone().unwrap_or_else(|| scratch_dir(&c.module));
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    std::fs::create_dir_all(dir.join("modules"))?;
    let copy = dir.join("modules").join(format!("{name}.mod"));
    m.save_as(&ModuleLocation::Archive(copy.clone()))?;
    let _ = std::fs::remove_file(copy.with_extension("mod.bak"));

    let config = serve::Config { module: copy, dir: Some(dir), stdin: false, ..c.serve.clone() };
    let mut report = Report::default();
    report.returns = returns;
    let mut late = false;
    let end = serve::serve(gi, &config, &mut |e| {
        match e {
            serve::Event::Log(line) => {
                echo(line);
                if report.line(line) {
                    return serve::Flow::Stop;
                }
            }
            serve::Event::Engine(line) => report.engine_line(line),
            serve::Event::Note(note) => notes.push(note.clone()),
            _ => {}
        }
        if started.elapsed().as_secs_f64() > c.timeout {
            late = true;
            return serve::Flow::Stop;
        }
        serve::Flow::Continue
    })?;
    let ended = match end {
        _ if report.done => None,
        serve::End::Stopped if late => {
            Some(format!("the tests were not done after {} seconds", c.timeout))
        }
        serve::End::Stopped => Some("the server was stopped".into()),
        serve::End::NotLoaded => Some("the server could not load the module".into()),
        serve::End::Exited(code) => Some(format!("the server ended ({code:?})")),
    };
    if !report.done {
        report.close(&tests);
    }
    Ok(Run { report, seconds: started.elapsed().as_secs_f64(), name, compile_errors, notes, ended })
}
