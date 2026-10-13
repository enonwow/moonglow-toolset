//! `mg nui`: the NUI Creator's windows from the command line, with the
//! checks and the generator the window uses (`mg-nui`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use mg_core::ResType;
use mg_edit::{Command, Workspace};
use mg_module::Module;
use mg_nui::Severity;
use mg_resman::ResKey;
use serde_json::json;

use crate::{Cli, Output, install, module_resman, path_text};

/// `mg nui`'s commands.
#[derive(Subcommand)]
pub(crate) enum NuiCmd {
    /// Add a new NUI window to a module and save: its layout (NAME.jui, a
    /// window with a Close button) and what the NUI Creator keeps about it
    /// (NAME.txt). Refuses a name whose resources the module already has.
    New {
        /// A module archive, folder or nasher project.
        module: PathBuf,
        /// The window's name, the ResRef of its resources: 1 to 14 lower
        /// case letters, digits or underscores (its scripts add `_o` and
        /// `_e`).
        name: String,
        /// List the resources it would add; change nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Check a NUI window as the NUI Creator does: the layout is JSON the
    /// game's NUI functions take, and every bind has a value. Prints each
    /// problem; exits with an error if one is an error. Changes nothing.
    Validate {
        /// A module archive, folder or nasher project.
        module: PathBuf,
        /// The window's name: the ResRef of its layout (NAME.jui).
        name: String,
    },
    /// Write a NUI window's scripts into a module and save, as Build does
    /// in the NUI Creator: the include that opens it (NAME_o.nss) and its
    /// event script (NAME_e.nss, compiled; the code written into it is
    /// kept), and the module's scripts that include the opener compiled
    /// again. All of it or nothing: a window with errors, an opener or
    /// event script edited by hand, or one that does not compile, changes
    /// nothing.
    Generate {
        /// A module archive, folder or nasher project.
        module: PathBuf,
        /// The window's name: the ResRef of its layout (NAME.jui).
        name: String,
        /// Check and compile, and list the resources it would change;
        /// change nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

pub(crate) fn run(cli: &Cli, cmd: &NuiCmd) -> Result<Output> {
    match cmd {
        NuiCmd::New { module, name, dry_run } => {
            let ws = Workspace::new(Module::open(module)?);
            let made = mg_nui::create(&ws.module, name)
                .map_err(anyhow::Error::msg)
                .with_context(|| window_text(module, name))?;
            write(ws, made, module, name, *dry_run, Output::default())
        }
        NuiCmd::Generate { module, name, dry_run } => {
            let ws = Workspace::new(Module::open(module)?);
            let mut out = Output::default();
            let rm = module_resman(&install(cli)?, &ws.module, &mut out)?;
            let made = mg_nui::generate(&ws.module, name, |n, t| {
                rm.get_named(n, t).ok().map(|b| b.into_owned())
            })
            .map_err(anyhow::Error::msg)
            .with_context(|| window_text(module, name))?;
            write(ws, made, module, name, *dry_run, out)
        }
        NuiCmd::Validate { module, name } => validate(module, name),
    }
}

/// The window an error is about: `MODULE: window NAME`.
fn window_text(module: &Path, name: &str) -> String {
    format!("{}: window {name}", path_text(module))
}

/// Applies what `new` or `generate` made (all of it or none) and saves,
/// or with `dry_run` only checks that it applies.
fn write(
    mut ws: Workspace,
    made: Command,
    module: &Path,
    name: &str,
    dry_run: bool,
    mut out: Output,
) -> Result<Output> {
    let mut resources: Vec<ResKey> = Vec::new();
    for e in &made.edits {
        if !resources.contains(e.key()) {
            resources.push(*e.key());
        }
    }
    ws.apply(made).with_context(|| window_text(module, name))?;
    let saved = !dry_run && !resources.is_empty();
    if saved {
        ws.save()?;
    }
    out.json = json!({
        "module": path_text(module),
        "window": name,
        "resources": resources.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "saved": saved,
        "dry_run": dry_run,
    });
    for r in &resources {
        out.line(r.to_string());
    }
    let what = if dry_run { "would change" } else { "changed" };
    let plural = if resources.len() == 1 { "" } else { "s" };
    out.line(format!("{name}: {what} {} resource{plural}", resources.len()));
    Ok(out)
}

/// `mg nui validate`: the NUI Creator's checks of one window, reported as
/// `mg verify` reports its findings.
fn validate(module: &Path, name: &str) -> Result<Output> {
    let m = Module::open(module)?;
    // (Any window the module has: not only one named as `new` names them.)
    let key = |ty: ResType| {
        ResKey::parse(name, ty).ok_or_else(|| anyhow!("{name:?} is not a resource's name"))
    };
    let jui = key(ResType::JUI)?;
    let data = m.get(&jui).with_context(|| format!("{jui} is not in the module"))?;
    let window = mg_nui::parse(data)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("{}: {jui}", path_text(module)))?;
    let txt = key(ResType::TXT)?;
    let settings = m
        .get(&txt)
        .map(mg_nui::Settings::parse)
        .transpose()
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("{}: {txt}", path_text(module)))?
        .unwrap_or_default();
    let findings = mg_nui::validate(&window, &settings);
    let errors = findings.iter().filter(|d| d.severity == Severity::Error).count();
    let severity = |s: Severity| match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let mut out = Output::new(json!({
        "module": path_text(module),
        "window": name,
        "errors": errors,
        "warnings": findings.len() - errors,
        "findings": findings.iter().map(|d| json!({
            "severity": severity(d.severity),
            "path": d.path,
            "message": d.message,
        })).collect::<Vec<_>>(),
    }));
    for d in &findings {
        out.line(format!("{}\t{}: {}", severity(d.severity), d.path, d.message));
    }
    out.line(format!("{} problems", findings.len()));
    if errors > 0 {
        out.failed = true;
        out.note(format!("mg: {jui} has errors"));
    }
    Ok(out)
}
