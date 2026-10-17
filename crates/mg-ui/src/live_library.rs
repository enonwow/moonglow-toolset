//! Module-scoped native NPC snapshots. Files contain only the native GFF JSON.
use crate::{
    Moonglow,
    live::{Command, Op},
    live_targets::{selection, send},
};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_BYTES: usize = 262_144;

pub fn valid_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default().to_ascii_uppercase();
    mg_live::scene::valid_name(name)
        && !name.ends_with(['.', ' '])
        && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn folder(root: &Path, module: &str) -> Result<PathBuf, String> {
    if !valid_name(module) {
        return Err("Module filename is not a safe library folder name.".into());
    }
    let mut path = root.to_path_buf();
    for part in ["live", "library", module] {
        path.push(part);
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir()) {
            return Err("Library folders must be real directories, not links.".into());
        }
    }
    Ok(path)
}

fn path(root: &Path, module: &str, name: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err("Name: 1-64 letters, digits, spaces, _ - or .; no reserved names or trailing dot/space.".into());
    }
    Ok(folder(root, module)?.join(format!("{name}.json")))
}

pub fn validate(value: &Value) -> Result<(), String> {
    if value["__data_type"] != "UTC " {
        return Err("The NPC library accepts creature snapshots only.".into());
    }
    Command {
        id: String::new(),
        area: "library".into(),
        op: Op::SpawnJson { json: value.clone(), x: 0., y: 0., z: 0., f: 0. },
    }
    .validate()
}

pub fn save(root: &Path, module: &str, name: &str, value: &Value) -> Result<(), String> {
    validate(value)?;
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Snapshot exceeds 256 KiB.".into());
    }
    let path = path(root, module, name)?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    // Never overwrite an existing library entry, including a link.
    let mut file =
        OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
    if let Err(e) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(e.to_string());
    }
    Ok(())
}

pub fn load(root: &Path, module: &str, name: &str) -> Result<Value, String> {
    let path = path(root, module, name)?;
    let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_BYTES as u64 {
        return Err("Snapshot must be a regular JSON file of at most 256 KiB.".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|e| e.to_string())?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Snapshot exceeds 256 KiB.".into());
    }
    let value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    validate(&value)?;
    Ok(value)
}

pub fn list(root: &Path, module: &str) -> Result<Vec<String>, String> {
    let path = folder(root, module)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| e.to_string())?.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_file() && !t.is_symlink()) {
            continue;
        }
        if let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.strip_suffix(".json"))
            .filter(|s| valid_name(s))
        {
            names.push(name.into());
        }
    }
    names.sort();
    Ok(names)
}

pub fn rename(root: &Path, module: &str, old: &str, new: &str) -> Result<(), String> {
    if old == new {
        return Ok(());
    }
    let value = load(root, module, old)?;
    save(root, module, new, &value)?;
    fs::remove_file(path(root, module, old)?)
        .map_err(|e| format!("New entry saved; old entry could not be removed: {e}"))
}

pub fn delete(root: &Path, module: &str, name: &str) -> Result<(), String> {
    let path = path(root, module, name)?;
    let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Not a regular library file.".into());
    }
    fs::remove_file(path).map_err(|e| e.to_string())
}

#[derive(Debug)]
struct Pending {
    id: String,
    boot: String,
    module_path: PathBuf,
    name: String,
    started: u64,
}

#[derive(Debug)]
pub struct LiveLibrary {
    pub data_dir: Option<PathBuf>,
    pub name: String,
    pub selected: String,
    pub files: Vec<String>,
    pub notice: Option<String>,
    pub at_selection: bool,
    pending: Option<Pending>,
    module_path: Option<PathBuf>,
    snapshot: Option<Value>,
    confirm_delete: bool,
    spawn: Option<(String, String)>,
}
impl Default for LiveLibrary {
    fn default() -> Self {
        Self {
            data_dir: crate::recovery::data_dir(),
            name: "New NPC".into(),
            selected: String::new(),
            files: Vec::new(),
            notice: None,
            at_selection: false,
            pending: None,
            module_path: None,
            snapshot: None,
            confirm_delete: false,
            spawn: None,
        }
    }
}
impl LiveLibrary {
    fn location(&self) -> Result<(PathBuf, String), String> {
        let root = self.data_dir.clone().ok_or("Application data directory unavailable.")?;
        let module = self
            .module_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|n| n.to_str())
            .ok_or("Save and open a module before using its NPC library.")?
            .to_string();
        folder(&root, &module)?;
        Ok((root, module))
    }
    fn refresh(&mut self) -> Result<(), String> {
        let (root, module) = self.location()?;
        self.files = list(&root, &module)?;
        Ok(())
    }
    fn capture(&mut self, app: &mut Moonglow) -> Result<(), String> {
        let (root, module) = self.location()?;
        let destination = path(&root, &module, &self.name)?;
        if destination.exists() {
            return Err("That name already exists; choose another name.".into());
        }
        let selected = selection(app)?;
        let [(kind, object)] = selected.objects.as_slice() else {
            return Err("Select exactly one Live NPC to capture.".into());
        };
        if *kind != mg_area::ObjectKind::Creature || object.pc || object.dm || object.possessed {
            return Err("Select an NPC, not a player or DM.".into());
        }
        let id = send(app, selected.area, Op::CaptureObject { target: object.id.clone() })?;
        self.pending = Some(Pending {
            id,
            boot: selected.boot,
            module_path: self.module_path.clone().unwrap(),
            name: self.name.clone(),
            started: mg_live::now_ms(),
        });
        self.notice = Some(format!("Capturing {}...", object.name));
        Ok(())
    }
    fn spawn(&mut self, app: &mut Moonglow) -> Result<(), String> {
        let (root, module) = self.location()?;
        let snapshot = load(&root, &module, &self.selected)?;
        let selected = selection(app)?;
        crate::live_aim::arm(
            app,
            selected.area,
            Op::SpawnJson { json: snapshot, x: 0., y: 0., z: 0., f: 0. },
            &format!("Spawn {}", self.selected),
            crate::live_aim::Source::Library,
        )?;
        self.notice = Some("Aim the NPC ghost, then click the map to spawn. Turn keys rotate; Esc/right click cancels.".into());
        Ok(())
    }
    pub(crate) fn aim_sent(&mut self, id: String, boot: String) {
        self.spawn = Some((id, boot));
        self.notice = Some("Spawn queued; waiting for the game.".into());
    }
}

pub(crate) fn update(app: &mut Moonglow, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.live_library);
    let module = app.module_path();
    if state.module_path != module {
        state.module_path = module;
        state.pending = None;
        state.selected.clear();
        state.snapshot = None;
        state.confirm_delete = false;
        state.spawn = None;
        state.files.clear();
        state.notice = state.refresh().err();
    }
    if let Some(pending) = &state.pending {
        let store = app.live.as_ref();
        if store.is_none_or(|s| s.boot() != pending.boot)
            || state.module_path.as_ref() != Some(&pending.module_path)
            || !crate::live_targets::enabled(app)
        {
            state.notice = Some("Capture cancelled: live session or module changed.".into());
            state.pending = None;
        } else if let Some(result) = store.and_then(|s| s.result(&pending.id)) {
            let name = pending.name.clone();
            let saved = (|| {
                if !result.ok {
                    return Err(result.err);
                }
                let snapshot = result.object_json.ok_or("Capture result contains no snapshot.")?;
                let (root, module) = state.location()?;
                save(&root, &module, &name, &snapshot)?;
                state.snapshot = Some(snapshot);
                state.selected = name.clone();
                state.name = name;
                state.refresh()?;
                Ok::<_, String>(())
            })();
            state.notice =
                Some(saved.err().unwrap_or_else(|| "NPC saved to the module library.".into()));
            state.pending = None;
        } else if mg_live::now_ms().saturating_sub(pending.started) > 120_000 {
            state.notice = Some("Capture timed out; no file saved.".into());
            state.pending = None;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    if let Some((id, boot)) = &state.spawn {
        if app.live.as_ref().is_none_or(|s| s.boot() != *boot) {
            state.notice = Some("Spawn interrupted: live session changed.".into());
            state.spawn = None;
        } else if let Some(r) = app.live.as_ref().and_then(|s| s.result(id)) {
            state.notice = Some(if r.ok { format!("Spawned {}.", r.obj) } else { r.err });
            state.spawn = None;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    app.live_library = state;
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    let mut state = std::mem::take(&mut app.live_library);
    let location = state.location();
    ui.heading("NPC Library");
    ui.separator();
    if let Ok((root, module)) = &location {
        ui.weak(format!("Module: {module}"))
            .on_hover_text(format!("Stored in {}", folder(root, module).unwrap().display()));
    }
    ui.label("Capture an NPC, select its saved entry, then Spawn NPC and click the map.");
    let selected = selection(app);
    let capture_reason = if state.pending.is_some() {
        Some("Wait for the current capture to finish.".into())
    } else if let Err(e) = &location {
        Some(e.clone())
    } else if !valid_name(&state.name) {
        Some("Enter a valid library name (letters, numbers, spaces, _ - or .).".into())
    } else if let Err(e) = &selected {
        Some(e.clone())
    } else if !selected.as_ref().is_ok_and(|s| matches!(s.objects.as_slice(), [(mg_area::ObjectKind::Creature, o)] if !o.pc && !o.dm && !o.possessed)) {
        Some("Select exactly one NPC on the Live map.".into())
    } else { None };
    let entry_reason = "Select a saved library entry first.";
    ui.horizontal(|ui| {
        ui.label("Entry name");
        ui.add(egui::TextEdit::singleline(&mut state.name).desired_width(200.));
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(capture_reason.is_none(), egui::Button::new("Capture selected NPC"))
            .on_disabled_hover_text(capture_reason.as_deref().unwrap_or_default())
            .on_hover_text("Save this NPC, including its inventory and scripts, under Entry name.")
            .clicked()
            && let Err(e) = state.capture(app)
        {
            state.notice = Some(e);
        }
        if ui
            .add_enabled(location.is_ok(), egui::Button::new("Refresh library"))
            .on_disabled_hover_text("Open a saved module to use its library.")
            .on_hover_text("Read entries saved by this or another tool.")
            .clicked()
        {
            state.notice = state.refresh().err();
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !state.selected.is_empty() && selected.is_ok(),
                egui::Button::new("Spawn NPC"),
            )
            .on_disabled_hover_text(
                selected.as_ref().err().map(String::as_str).unwrap_or(entry_reason),
            )
            .on_hover_text("Aim a preview on the map, then click to spawn. Esc cancels.")
            .clicked()
            && let Err(e) = state.spawn(app)
        {
            state.notice = Some(e);
        }
        if ui
            .add_enabled(!state.selected.is_empty(), egui::Button::new("Rename entry"))
            .on_disabled_hover_text(entry_reason)
            .on_hover_text("Rename the selected file to Entry name.")
            .clicked()
        {
            state.notice = Some(
                (|| {
                    let (r, m) = state.location()?;
                    rename(&r, &m, &state.selected, &state.name)?;
                    state.selected = state.name.clone();
                    state.refresh()?;
                    Ok::<_, String>("Entry renamed.".into())
                })()
                .unwrap_or_else(|e| e),
            );
        }
        if ui
            .add_enabled(!state.selected.is_empty(), egui::Button::new("Delete entry..."))
            .on_disabled_hover_text(entry_reason)
            .on_hover_text("Remove the saved file after confirmation. Spawned NPCs are unaffected.")
            .clicked()
        {
            state.confirm_delete = true;
        }
    });
    if state.confirm_delete {
        ui.horizontal(|ui| {
            ui.label(format!("Delete '{}' from this library?", state.selected));
            if ui.button("Delete permanently").clicked() {
                state.notice = Some(
                    (|| {
                        let (r, m) = state.location()?;
                        delete(&r, &m, &state.selected)?;
                        state.selected.clear();
                        state.snapshot = None;
                        state.refresh()?;
                        Ok::<_, String>("Entry deleted.".into())
                    })()
                    .unwrap_or_else(|e| e),
                );
                state.confirm_delete = false;
            }
            if ui.button("Keep entry").clicked() {
                state.confirm_delete = false;
            }
        });
    }
    if let Some(note) = &state.notice {
        ui.label(note);
    }
    if let Err(e) = location {
        ui.label(e);
    }
    ui.separator();
    if state.files.is_empty() {
        ui.label("No saved NPCs in this module's library.");
        ui.weak("Select an NPC in the area, enter a name, then Capture selected NPC.");
    }
    ui.columns(2, |cols| {
        egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
            for name in state.files.clone() {
                if ui.selectable_label(state.selected == name, &name).clicked() {
                    state.selected = name.clone();
                    state.name = name.clone();
                    state.confirm_delete = false;
                    match state.location().and_then(|(r, m)| load(&r, &m, &name)) {
                        Ok(value) => {
                            state.snapshot = Some(value);
                            state.notice = None;
                        }
                        Err(e) => {
                            state.snapshot = None;
                            state.notice = Some(e);
                        }
                    }
                }
            }
        });
        if let Some(value) = &state.snapshot {
            let ui = &mut cols[1];
            ui.heading(&state.selected);
            if let Some(row) =
                value["Appearance_Type"]["value"].as_u64().and_then(|r| u16::try_from(r).ok())
            {
                let mut look = mg_preview::CreatureLook::new(row);
                look.gender = value["Gender"]["value"].as_u64().unwrap_or(0) as u8;
                let key = crate::model_view::Pictured::Look(look);
                if let Some(Some(id)) = app.thumbnails.get(key) {
                    let edge = ui.available_width().min(192.);
                    ui.image((id, egui::vec2(edge, edge)));
                }
                ui.weak(
                    "Appearance thumbnail; the saved snapshot also includes inventory and scripts.",
                );
            }
            ui.label(format!("{} bytes", serde_json::to_vec(value).map_or(0, |v| v.len())));
        }
    });
    app.live_library = state;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_unsafe_names_and_non_creature_snapshots() {
        for name in ["../npc", "npc/one", ".hidden", "CON", "con.txt", "LPT1", "npc.", "npc "] {
            assert!(!valid_name(name), "{name}");
        }
        assert!(valid_name("Guard 01"));
        assert!(validate(&serde_json::json!({"__data_type":"UTP "})).is_err());
        assert!(validate(&serde_json::json!({"__data_type":"UTC ","Bad":{"type":"cexostring","value":"a".repeat(MAX_BYTES)}})).is_err());
    }
}
