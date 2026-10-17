//! A front end to the same embedded builder as `mg live-build`.
use crate::{Moonglow, dialogs::FileKind};
use mg_live::build::{Report, Request};
use std::{path::PathBuf, sync::mpsc};

#[derive(Debug, Default)]
pub struct LivePrepare {
    pub source: String,
    pub output: String,
    pub capture_dialogue: bool,
    pub report: Option<Report>,
    pub error: Option<String>,
    initialized: bool,
    pending: Option<mpsc::Receiver<Result<Report, String>>>,
}

impl LivePrepare {
    pub fn select(&mut self, source: PathBuf) {
        self.output = mg_live::build::default_output(&source).to_string_lossy().into_owned();
        self.source = source.to_string_lossy().into_owned();
        self.report = None;
        self.error = None;
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

pub fn update(app: &mut Moonglow, ctx: &egui::Context) {
    let state = &mut app.live_prepare;
    if let Some(rx) = &state.pending {
        match rx.try_recv() {
            Ok(Ok(report)) => {
                state.report = Some(report);
                state.pending = None;
            }
            Ok(Err(e)) => {
                state.error = Some(e);
                state.pending = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                state.error = Some("Module preparation worker stopped".into());
                state.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
        }
    }
}

pub fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    let mut state = std::mem::take(&mut app.live_prepare);
    if !state.initialized {
        if let Some(path) = app.module_path() {
            state.select(path.to_path_buf());
        }
        state.initialized = true;
    }
    ui.heading("Prepare Module for Live");
    ui.label("Build a new module copy with Live scripts and server settings.");
    ui.weak("Reads the saved module. Unsaved editor changes are not included; the open module stays unchanged.");
    ui.separator();
    let busy = state.busy();
    ui.add_enabled_ui(!busy, |ui| {
        crate::widgets::field_label(ui, "Source module, folder or nasher project");
        ui.add(egui::TextEdit::singleline(&mut state.source).desired_width(ui.available_width()));
        ui.horizontal(|ui| {
            if ui.button("Use open module").clicked()
                && let Some(path) = app.module_path() { state.select(path.to_path_buf()); }
            if ui.button("Browse module…").clicked()
                && let Some(path) = app.dialogs.open_file(FileKind::Module, None) { state.select(path); }
            if ui.button("Browse folder…").clicked()
                && let Some(path) = app.dialogs.pick_folder("Module folder or nasher project", None) { state.select(path); }
        });
        crate::widgets::field_label(ui, "New output module");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut state.output).desired_width((ui.available_width() - 90.).max(120.)));
            if ui.button("Output…").clicked()
                && let Some(path) = app.dialogs.save_file(FileKind::Module, Some(std::path::Path::new(&state.output))) { state.output = path.to_string_lossy().into_owned(); }
        });
        ui.label(format!("NWNX folder: {}", if app.settings.live_nwnx.is_empty() { "not set — Options > Live" } else { &app.settings.live_nwnx }));
        if ui.link("Options > Live").clicked() { app.actions.push(crate::Action::LiveOptions); }
        ui.checkbox(&mut state.capture_dialogue, "Capture dialogue").on_hover_text("Report NPC lines and player replies; preserves the original action scripts and raw text bytes.");
        let reason = if app.install.is_none() { Some("Set the game installation in Options first") }
            else if app.settings.live_nwnx.is_empty() { Some("Set the NWNX folder in Options > Live") }
            else if state.source.trim().is_empty() || state.output.trim().is_empty() { Some("Choose source and output paths") }
            else { None };
        if ui.add_enabled(reason.is_none(), egui::Button::new("Build Live copy"))
            .on_disabled_hover_text(reason.unwrap_or_default()).clicked() {
            let request = Request { module: state.source.clone().into(), nwnx: app.settings.live_nwnx.clone().into(), out: Some(state.output.clone().into()), capture_dialogue: state.capture_dialogue };
            let install = app.install.clone().unwrap();
            let (tx, rx) = mpsc::channel();
            state.pending = Some(rx); state.report = None; state.error = None;
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result = mg_live::build::prepare(&request, &install).map_err(|e| format!("{e:#}"));
                let _ = tx.send(result); ctx.request_repaint();
            });
        }
    });
    if state.busy() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Compiling and verifying the copy…");
        });
    }
    if let Some(error) = &state.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if let Some(report) = &state.report {
        ui.separator();
        ui.heading(if report.ok {
            "Live copy ready"
        } else {
            "Build failed — no output written"
        });
        ui.label(format!(
            "{} scripts compiled; {} dialogue nodes instrumented",
            report.compiled,
            report.dialogues.len()
        ));
        ui.label(&report.module_ifo);
        if report.ok {
            ui.label(report.output.display().to_string());
            if ui.button("Open folder").clicked() {
                let path = report.output.parent().unwrap_or(std::path::Path::new("."));
                ui.ctx().open_url(egui::OpenUrl::new_tab(crate::script_view::file_url(path)));
            }
        }
        egui::ScrollArea::vertical().max_height(240.).show(ui, |ui| {
            for line in &report.errors {
                ui.colored_label(ui.visuals().error_fg_color, line);
            }
            for line in &report.warnings {
                ui.colored_label(ui.visuals().warn_fg_color, line);
            }
            ui.collapsing(format!("Files added ({})", report.added.len()), |ui| {
                for name in &report.added {
                    ui.label(name);
                }
            });
            ui.collapsing(format!("Files replaced ({})", report.replaced.len()), |ui| {
                for name in &report.replaced {
                    ui.label(name);
                }
            });
        });
    }
    app.live_prepare = state;
}
