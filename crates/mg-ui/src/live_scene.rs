//! Scene editor and manual-command recorder. Game execution stays in mg-live.
use crate::Moonglow;
use mg_live::{
    operations,
    scene::{self, Player, Scene, Step, Wait},
    store::{Command, CommandError, Policy},
};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub struct StepDraft {
    pub op: String,
    pub input: String,
    pub delay_ms: u64,
    pub wait: Wait,
}
impl From<Step> for StepDraft {
    fn from(step: Step) -> Self {
        Self {
            op: step.op,
            input: serde_json::to_string_pretty(&step.input).unwrap_or_default(),
            delay_ms: step.delay_ms,
            wait: step.wait,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub name: String,
    pub steps: Vec<StepDraft>,
    pub dirty: bool,
    saved_name: Option<String>,
}
impl Default for Draft {
    fn default() -> Self {
        Self { name: "New scene".into(), steps: Vec::new(), dirty: false, saved_name: None }
    }
}
impl Draft {
    fn from_scene(scene: Scene) -> Self {
        Self {
            saved_name: Some(scene.name.clone()),
            name: scene.name,
            steps: scene.steps.into_iter().map(Into::into).collect(),
            dirty: false,
        }
    }

    pub fn checked(&self, policy: Policy) -> Result<Scene, String> {
        self.checked_with(&operations::describe(policy))
    }

    fn checked_with(&self, offered: &Value) -> Result<Scene, String> {
        #[cfg(feature = "measure")]
        let _measure = crate::measure::span("scene.validate");
        let mut steps = Vec::new();
        for (i, draft) in self.steps.iter().enumerate() {
            let check = || -> Result<Step, String> {
                let op = offered["operations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|o| o["name"] == draft.op && o["name"] != "live_wait")
                    .ok_or("Operation is unavailable under the current Live policy")?;
                let input: Value = serde_json::from_str(&draft.input).map_err(|e| e.to_string())?;
                validate(&op["input"], &input)?;
                // Typed commands add semantic checks beyond the advertised schema.
                let mut command = input.clone();
                command["op"] = draft.op.trim_start_matches("live_").into();
                let parsed = serde_json::from_value::<Command>(command);
                if let Ok(command) = &parsed {
                    command.validate()?;
                }
                if matches!(draft.wait, Wait::Result { .. }) && parsed.is_err() {
                    return Err("Result wait requires an operation that queues a command".into());
                }
                if let Wait::Near { area, target, x, y, radius, .. } = &draft.wait
                    && (area.is_empty()
                        || area.len() > 16
                        || target.is_empty()
                        || !target.bytes().all(|b| b.is_ascii_hexdigit())
                        || !x.is_finite()
                        || !y.is_finite()
                        || !radius.is_finite()
                        || *radius <= 0.)
                {
                    return Err(
                        "Near wait needs an area, a hex object ID and finite coordinates/radius"
                            .into(),
                    );
                }
                Ok(Step {
                    op: draft.op.clone(),
                    input,
                    delay_ms: draft.delay_ms,
                    wait: draft.wait.clone(),
                })
            };
            steps.push(check().map_err(|e| format!("Step {}: {e}", i + 1))?);
        }
        let scene = Scene { name: self.name.clone(), steps };
        scene.check()?;
        Ok(scene)
    }
}

/// Import is a draft only; validation never sends commands.
fn import_file(path: &std::path::Path, policy: Policy) -> Result<Draft, String> {
    use std::io::Read;
    const LIMIT: u64 = 2 * 1024 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Scene JSON exceeds 2 MiB.".into());
    }
    let scene: Scene = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut draft = Draft::from_scene(scene);
    draft.checked(policy)?;
    draft.saved_name = None;
    draft.dirty = true;
    Ok(draft)
}
fn export_file(path: &std::path::Path, scene: &Scene) -> Result<(), String> {
    if !path.extension().is_some_and(|s| s.eq_ignore_ascii_case("json")) {
        return Err("Choose a .json filename.".into());
    }
    let text = serde_json::to_vec_pretty(scene).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
fn summary(step: &StepDraft, live: Option<&mg_live::store::LiveStore>) -> String {
    let Ok(input) = serde_json::from_str::<Value>(&step.input) else {
        return "Invalid JSON: open Edit step to correct it".into();
    };
    let target = input["target"].as_str().unwrap_or_default();
    let who = live
        .and_then(|s| s.locate(target))
        .map(|(_, _, o)| o.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(
            || if target.is_empty() { String::new() } else { "Unreported object".into() },
        );
    let number = |key: &str| {
        input[key].as_f64().map_or_else(|| input[key].to_string(), |n| format!("{n:.1}"))
    };
    match step.op.as_str() {
        "live_npc_move" => format!(
            "{who} {} to {}, {}",
            if input["run"] == true { "runs" } else { "walks" },
            number("x"),
            number("y")
        ),
        "live_npc_say" | "live_pc_message" => format!(
            "{who} says ‘{}’",
            input["text"].as_str().unwrap_or_default().chars().take(100).collect::<String>()
        ),
        _ => {
            let action = step.op.trim_start_matches("live_").replace('_', " ");
            if who.is_empty() { action } else { format!("{who}: {action}") }
        }
    }
}

/// UI preflight for the discovered schema subset. Execution validates again in
/// mg-live; its validator is private, so this layer neither dispatches nor probes.
fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    if let Some(choices) = schema["oneOf"].as_array() {
        return if choices.iter().filter(|s| validate(s, value).is_ok()).count() == 1 {
            Ok(())
        } else {
            Err("Value must match exactly one allowed type".into())
        };
    }
    let correct = match schema["type"].as_str() {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("number") => value.is_number(),
        Some("null") => value.is_null(),
        _ => false,
    };
    if !correct {
        return Err(format!("Expected {}", schema["type"]));
    }
    if let Some(object) = value.as_object() {
        for key in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if !object.contains_key(key) {
                return Err(format!("Missing {key}"));
            }
        }
        for (key, v) in object {
            let property =
                schema["properties"].get(key).ok_or_else(|| format!("Unknown property {key}"))?;
            validate(property, v).map_err(|e| format!("{key}: {e}"))?;
        }
    }
    if let Some(array) = value.as_array() {
        for v in array {
            validate(&schema["items"], v)?;
        }
    }
    if let Some(s) = value.as_str() {
        let len = s.chars().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|n| len < n)
            || schema["maxLength"].as_u64().is_some_and(|n| len > n)
        {
            return Err("String length is out of range".into());
        }
    }
    if let Some(n) = value.as_f64()
        && (schema["minimum"].as_f64().is_some_and(|min| n < min)
            || schema["maximum"].as_f64().is_some_and(|max| n > max)
            || schema["exclusiveMinimum"].as_f64().is_some_and(|min| n <= min)
            || schema["exclusiveMaximum"].as_f64().is_some_and(|max| n >= max))
    {
        return Err("Number is out of range".into());
    }
    if schema["enum"].as_array().is_some_and(|options| !options.contains(value)) {
        return Err("Value is not in the allowed choices".into());
    }
    Ok(())
}

#[derive(Debug)]
struct Validation {
    policy: Policy,
    name: String,
    steps: Vec<StepDraft>,
    result: Arc<Result<Scene, String>>,
}

#[derive(Debug)]
pub struct LiveScene {
    /// Application data root; tests supply an isolated directory.
    pub data_dir: Option<PathBuf>,
    pub draft: Draft,
    pub recording: bool,
    pub player: Player,
    pub status: Value,
    pub notice: Option<String>,
    module_path: Option<PathBuf>,
    drafts: HashMap<PathBuf, Draft>,
    files: Vec<String>,
    hotkeys: [Option<String>; 4],
    selected: String,
    file_action: Option<(String, Option<String>)>,
    last_recorded: Option<u64>,
    boot: Option<String>,
    catalogue: Option<(Policy, Arc<Value>)>,
    validation: Option<Validation>,
}
impl Default for LiveScene {
    fn default() -> Self {
        Self {
            data_dir: crate::recovery::data_dir(),
            draft: Draft::default(),
            recording: false,
            player: Player::default(),
            status: Value::Null,
            notice: None,
            module_path: None,
            drafts: HashMap::new(),
            files: Vec::new(),
            hotkeys: Default::default(),
            selected: String::new(),
            file_action: None,
            last_recorded: None,
            boot: None,
            catalogue: None,
            validation: None,
        }
    }
}
impl LiveScene {
    fn catalogue(&mut self, policy: Policy) -> Arc<Value> {
        let cache =
            self.catalogue.get_or_insert_with(|| (policy, Arc::new(operations::describe(policy))));
        if cache.0 != policy {
            *cache = (policy, Arc::new(operations::describe(policy)));
        }
        cache.1.clone()
    }

    fn checked(&mut self, policy: Policy, offered: &Value) -> Arc<Result<Scene, String>> {
        if self.validation.as_ref().is_none_or(|v| {
            v.policy != policy || v.name != self.draft.name || v.steps != self.draft.steps
        }) {
            self.validation = Some(Validation {
                policy,
                name: self.draft.name.clone(),
                steps: self.draft.steps.clone(),
                result: Arc::new(self.draft.checked_with(offered)),
            });
        }
        self.validation.as_ref().unwrap().result.clone()
    }

    fn location(&self) -> Result<(PathBuf, String), String> {
        let dir = self.data_dir.clone().ok_or("Application data directory is unavailable")?;
        let module = self
            .module_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .ok_or("Save and open a module before using scenes")?
            .to_string();
        scene::folder(&dir, &module)?;
        Ok((dir, module))
    }
    fn refresh(&mut self) -> Result<(), String> {
        let (dir, module) = self.location()?;
        self.files = scene::list(&dir, &module)?;
        Ok(())
    }
    fn hotkeys_path(&self) -> Result<PathBuf, String> {
        let (dir, module) = self.location()?;
        Ok(dir.join("live/scene-keys").join(format!("{module}.json")))
    }
    fn load_hotkeys(&mut self) -> Result<(), String> {
        let path = self.hotkeys_path()?;
        match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 16384 => {
                self.hotkeys =
                    serde_json::from_slice(&bytes).map_err(|e| format!("Macro hotkeys: {e}"))?
            }
            Ok(_) => return Err("Macro hotkeys file exceeds 16 KiB".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Macro hotkeys: {e}")),
        }
        Ok(())
    }
    fn save_hotkeys(&self) -> Result<(), String> {
        let path = self.hotkeys_path()?;
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.part");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.hotkeys).unwrap())
            .map_err(|e| e.to_string())?;
        std::fs::rename(tmp, path).map_err(|e| e.to_string())
    }
    fn play_hotkey(
        &mut self,
        index: usize,
        live: Arc<mg_live::store::LiveStore>,
    ) -> Result<(), String> {
        if self.recording || self.player.status()["playing"].is_string() {
            return Err("Stop recording or playback before playing a macro hotkey.".into());
        }
        let name = self.hotkeys[index]
            .as_deref()
            .ok_or("No macro assigned to this slot. Assign one in Scenes.")?;
        let (dir, module) = self.location()?;
        let scene = scene::load(&dir, &module, name)?;
        let checked = Draft::from_scene(scene).checked(live.policy())?;
        self.player.start(checked, live)?;
        self.status = self.player.status();
        Ok(())
    }
    pub fn stop(&mut self) {
        self.recording = false;
        self.last_recorded = None;
        self.player.stop();
        self.status = self.player.status();
    }
    fn update(&mut self, module: Option<PathBuf>, boot: Option<String>, enabled: bool) {
        if self.module_path != module {
            self.stop();
            if let Some(old) = self.module_path.take() {
                self.drafts.insert(old, std::mem::take(&mut self.draft));
            }
            self.module_path = module;
            self.draft =
                self.module_path.as_ref().and_then(|p| self.drafts.remove(p)).unwrap_or_default();
            self.file_action = None;
            self.selected.clear();
            self.files.clear();
            self.hotkeys = Default::default();
            self.notice = None;
            if self.module_path.is_some() {
                self.notice = self.refresh().and_then(|()| self.load_hotkeys()).err();
            }
        }
        if !enabled || self.boot != boot {
            self.stop();
        }
        self.boot = boot;
        self.status = self.player.status();
    }
    pub fn record_command(&mut self, command: &Command, at_ms: u64) {
        if !self.recording {
            return;
        }
        let delay = self.last_recorded.map_or(0, |last| at_ms.saturating_sub(last));
        if self.draft.steps.len() >= scene::MAX_STEPS || delay > scene::MAX_DELAY_MS {
            self.recording = false;
            self.notice = Some("Command sent, but recording stopped: at most 200 steps and 10 minutes between commands.".into());
            return;
        }
        let mut input = serde_json::to_value(command).expect("a typed command serializes");
        let fields = input.as_object_mut().unwrap();
        let op = format!("live_{}", fields.remove("op").unwrap().as_str().unwrap());
        fields.remove("id"); // Replaying must get a fresh ID, never deduplicate the recording.
        // Typed script commands serialize an absent target as null; agent input
        // selects the area by omitting it, and its schema accepts only strings.
        if fields.get("target").is_some_and(Value::is_null) {
            fields.remove("target");
        }
        self.draft.steps.push(StepDraft::from(Step {
            op,
            input,
            delay_ms: delay,
            wait: Wait::Nothing,
        }));
        self.draft.dirty = true;
        self.last_recorded = Some(at_ms);
    }
}

/// Shared path for commands manually sent from a Live view. Scene playback
/// calls mg-live directly and must not record itself.
pub fn enqueue(app: &mut Moonglow, command: Command) -> Result<String, CommandError> {
    let now = mg_live::now_ms();
    let id = app.live_backend().enqueue(command.clone())?;
    app.live_scene.record_command(&command, now);
    app.live_commands.insert(id.clone());
    Ok(id)
}

pub(crate) fn update(app: &mut Moonglow, ctx: &egui::Context) {
    let module = app.module_path();
    let boot = app.live.as_ref().map(|live| live.boot());
    let enabled = crate::live_targets::enabled(app);
    app.live_scene.update(module, boot, enabled);
    if app.live_scene.status["playing"].is_string() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn wait_ui(ui: &mut egui::Ui, wait: &mut Wait) {
    let mut kind = match wait {
        Wait::Nothing => 0,
        Wait::Result { .. } => 1,
        Wait::Near { .. } => 2,
    };
    let old = kind;
    egui::ComboBox::from_id_salt("wait")
        .selected_text(["No wait", "Command result", "Object near point"][kind])
        .show_ui(ui, |ui| {
            for (i, name) in ["No wait", "Command result", "Object near point"].iter().enumerate() {
                ui.selectable_value(&mut kind, i, *name);
            }
        });
    if kind != old {
        *wait = match kind {
            1 => Wait::Result { timeout_ms: 5000 },
            2 => Wait::Near {
                area: String::new(),
                target: String::new(),
                x: 0.,
                y: 0.,
                radius: 1.,
                timeout_ms: 30000,
            },
            _ => Wait::Nothing,
        };
    }
    if let Wait::Near { area, target, x, y, radius, .. } = wait {
        ui.horizontal_wrapped(|ui| {
            ui.label("Area");
            ui.add(egui::TextEdit::singleline(area).desired_width(110.));
            ui.label("Object ID");
            ui.add(egui::TextEdit::singleline(target).desired_width(100.));
            ui.add(egui::DragValue::new(x).prefix("X "));
            ui.add(egui::DragValue::new(y).prefix("Y "));
            ui.add(egui::DragValue::new(radius).prefix("Radius ").range(0.01..=1000.));
        });
    }
    if let Wait::Result { timeout_ms } | Wait::Near { timeout_ms, .. } = wait {
        ui.add(
            egui::DragValue::new(timeout_ms)
                .prefix("Timeout ")
                .suffix(" ms")
                .range(1..=scene::MAX_TIMEOUT_MS),
        );
    }
}

fn example(app: &Moonglow) -> Result<Scene, String> {
    let selected = crate::live_targets::selection(app)?;
    let [(mg_area::ObjectKind::Creature, npc)] = selected.objects.as_slice() else {
        return Err("Select one living NPC in the Live view.".into());
    };
    if npc.pc || npc.dm || npc.possessed || npc.dead {
        return Err("Select one living NPC, not a player or DM.".into());
    }
    let area = selected.area.to_string();
    let size = app.live.as_ref().and_then(|s| s.area(&area)).and_then(|a| a.size).or_else(|| {
        app.ws
            .as_ref()?
            .module
            .gff(&mg_resman::ResKey::new(selected.area, mg_core::ResType::ARE))?
            .ok()
            .and_then(|g| {
                Some([
                    u32::try_from(g.root.integer("Width")?).ok()? * 10,
                    u32::try_from(g.root.integer("Height")?).ok()? * 10,
                ])
            })
    });
    let x = if size.is_some_and(|[w, _]| npc.x + 5. >= w as f32) { npc.x - 5. } else { npc.x + 5. };
    if x < 0. {
        return Err("There is not 5 m of room beside this NPC.".into());
    }
    Ok(Scene {
        name: "Example scene".into(),
        steps: vec![
            Step {
                op: "live_npc_say".into(),
                input: json!({"area":area,"target":npc.id,"text":"Hello","volume":"talk"}),
                delay_ms: 0,
                wait: Wait::Result { timeout_ms: 5000 },
            },
            Step {
                op: "live_npc_move".into(),
                input: json!({"area":area,"target":npc.id,"x":x,"y":npc.y,"z":npc.z,"run":false,"hold":true,"timeout":15}),
                delay_ms: 1000,
                wait: Wait::Near {
                    area,
                    target: npc.id.clone(),
                    x,
                    y: npc.y,
                    radius: 1.,
                    timeout_ms: 15000,
                },
            },
        ],
    })
}

fn save_draft(state: &mut LiveScene, policy: Policy) -> Result<(), String> {
    let (dir, module) = state.location()?;
    let scene = state.draft.checked(policy)?;
    scene::save(&dir, &module, &scene)?;
    state.selected = scene.name.clone();
    state.draft = Draft::from_scene(scene);
    state.refresh()
}

fn saved_macros(
    ui: &mut egui::Ui,
    state: &mut LiveScene,
    live: Option<&std::sync::Arc<mg_live::store::LiveStore>>,
    policy: Policy,
    enabled: bool,
    playing: bool,
    keys: &crate::keys::Keymap,
) {
    ui.strong("Saved macros");
    let idle = !playing && !state.recording && !state.draft.dirty;
    if state.files.is_empty() {
        ui.weak("No saved macros. Record or try the example, then Save beside its name.");
    }
    egui::ScrollArea::vertical().id_salt("saved-macros").max_height(130.).show(ui, |ui| {
        for name in state.files.clone() {
            ui.horizontal(|ui| {
                let row = ui
                    .add_enabled(idle, egui::Button::new(&name).selected(state.selected == name))
                    .on_disabled_hover_text(
                        "Save or discard this draft and stop recording/playback first.",
                    );
                let mut load = row.clicked();
                let mut play = false;
                row.context_menu(|ui| {
                    if ui.button("Rename...").clicked() {
                        state.file_action = Some((name.clone(), Some(name.clone())));
                        ui.close();
                    }
                    if ui.button("Delete macro...").clicked() {
                        state.file_action = Some((name.clone(), None));
                        ui.close();
                    }
                });

                if ui
                    .add_enabled(idle && enabled, egui::Button::new("Play"))
                    .on_disabled_hover_text(
                        "Enable Live, save/discard the draft, and stop playback first.",
                    )
                    .clicked()
                {
                    load = true;
                    play = true;
                }
                let assigned=state.hotkeys.iter().position(|n|n.as_ref()==Some(&name));
                ui.menu_button(assigned.map_or_else(||"Assign key...".into(),|i|format!("Slot {}",i+1)),|ui| {
                    for (i,cmd) in macro_keys().into_iter().enumerate() {
                        let label=format!("Slot {}: {}",i+1,keys.label(cmd,ui.ctx()));
                        if ui.selectable_label(assigned==Some(i),label).clicked() {
                            let old=state.hotkeys.clone();
                            for n in &mut state.hotkeys {if n.as_ref()==Some(&name) {*n=None;}}
                            state.hotkeys[i]=Some(name.clone());
                            if let Err(e)=state.save_hotkeys() {state.hotkeys=old;state.notice=Some(e);}
                            ui.close();
                        }
                    }
                    if ui.add_enabled(assigned.is_some(),egui::Button::new("Clear assignment")).clicked() {
                        let old=state.hotkeys.clone();for n in &mut state.hotkeys {if n.as_ref()==Some(&name) {*n=None;}}
                        if let Err(e)=state.save_hotkeys(){state.hotkeys=old;state.notice=Some(e);}ui.close();
                    }
                    ui.weak("Change the keys in Options > Keyboard > Live area. Use while the pointer is over a Live map.");
                });
                if load {
                    state.notice = (|| {
                        let (dir, module) = state.location()?;
                        let scene = scene::load(&dir, &module, &name)?;
                        let draft = Draft::from_scene(scene);
                        let scene = draft.checked(policy)?;
                        state.draft = draft;
                        state.selected = name.clone();
                        if play {
                            state.player.start(scene, live.unwrap().clone())?;
                            ui.ctx().request_repaint();
                        }
                        Ok::<_, String>(())
                    })()
                    .err();
                }
            });
        }
    });
    if let Some((name, mut rename)) = state.file_action.take() {
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let title = if rename.is_some() { "Rename macro" } else { "Delete macro" };
        egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).show(
            ui.ctx(),
            |ui| {
                ui.label(&name);
                if let Some(new) = &mut rename {
                    let label = ui.label("New macro name");
                    ui.text_edit_singleline(new).labelled_by(label.id);
                } else {
                    ui.label("Delete this saved macro JSON? This cannot be undone.");
                }
                ui.horizontal(|ui| {
                    cancel = ui.button("Cancel").clicked();
                    apply = ui
                        .add_enabled(
                            idle,
                            egui::Button::new(if rename.is_some() {
                                "Rename"
                            } else {
                                "Delete saved macro"
                            }),
                        )
                        .clicked();
                });
            },
        );
        if apply {
            state.notice = (|| {
                let (dir, module) = state.location()?;
                let mut scene = scene::load(&dir, &module, &name)?;
                let folder = scene::folder(&dir, &module)?;
                let old = folder.join(format!("{name}.json"));
                let metadata = std::fs::symlink_metadata(&old).map_err(|e| e.to_string())?;
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || old.canonicalize().map_err(|e| e.to_string())?.parent()
                        != Some(folder.canonicalize().map_err(|e| e.to_string())?.as_path())
                {
                    return Err("Refusing a macro outside its module folder.".into());
                }
                if let Some(new) = &rename {
                    if new == &name {
                        return Ok(());
                    }
                    scene.name = new.clone();
                    scene.check()?;
                    if folder.join(format!("{new}.json")).exists() {
                        return Err("That macro name already exists.".into());
                    }
                    scene::save(&dir, &module, &scene)?;
                }
                std::fs::remove_file(&old).map_err(|e| e.to_string())?;
                if state.selected == name {
                    if rename.is_some() {
                        state.selected = scene.name.clone();
                        state.draft = Draft::from_scene(scene);
                    } else {
                        state.selected.clear();
                        state.draft = Draft::default();
                    }
                }
                for key in &mut state.hotkeys {
                    if key.as_ref() == Some(&name) {
                        *key = rename.clone();
                    }
                }
                state.save_hotkeys()?;
                state.refresh()?;
                Ok::<_, String>(())
            })()
            .err();
        } else if open && !cancel {
            state.file_action = Some((name, rename));
        }
    }
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("window.scene");
    let live = app.live.clone();
    let policy = live.as_ref().map_or_else(Default::default, |s| s.policy());
    let enabled = crate::live_targets::enabled(app) && live.is_some();
    let example = example(app);
    let dialogs = &mut app.dialogs;
    let state = &mut app.live_scene;
    let (dir, module) = match state.location() {
        Ok(location) => location,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    ui.heading(format!("Scenes for {module}"));
    ui.label("A scene is a macro: a recorded list of DM commands you can replay");
    let playing = state.status["playing"].is_string();
    if ui.add_enabled(enabled && !playing && !state.recording && !state.draft.dirty && example.is_ok(),
        egui::Button::new("Try it: make an example scene with the selected NPC").wrap())
        .on_disabled_hover_text(example.as_ref().err().map(String::as_str).unwrap_or("Save or discard this draft, and stop recording/playback first."))
        .on_hover_text("Create two steps: say Hello, wait for the result, then walk 5 m and wait for arrival. Nothing runs until you press Play scene.")
        .clicked() {
        state.draft = Draft::from_scene(example.unwrap());
        state.draft.saved_name = None;
        state.draft.dirty = true;
        state.selected.clear();
        state.notice = Some("Example ready. Press Play scene to see the selected NPC say Hello and walk 5 m.".into());
    }
    saved_macros(ui, state, live.as_ref(), policy, enabled, playing, &app.keymap);
    let offered = state.catalogue(policy);
    let validation = state.checked(policy, &offered);
    let checked = validation.as_ref();
    ui.menu_button("Scene", |ui| {
        ui.add_enabled_ui(!state.draft.dirty && !playing && !state.recording, |ui| {
            ui.menu_button("Load scene", |ui| {
                if state.files.is_empty() {
                    ui.weak("No saved scenes in this module yet.");
                }
                for name in state.files.clone() {
                    if ui.button(&name).clicked() {
                        match scene::load(&dir, &module, &name) {
                            Ok(scene) => {
                                state.draft = Draft::from_scene(scene);
                                state.selected = name;
                                state.notice = None;
                            }
                            Err(e) => state.notice = Some(e),
                        }
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Refresh scenes").clicked() {
                    state.notice = state.refresh().err();
                }
            });
        })
        .response
        .on_disabled_hover_text(
            "Save or discard changes and stop recording/playback before loading.",
        );
        if ui
            .add_enabled(
                !state.draft.dirty && !playing && !state.recording,
                egui::Button::new("New scene"),
            )
            .on_disabled_hover_text("Save or discard changes and stop recording/playback first.")
            .clicked()
        {
            state.draft = Draft::default();
            state.selected.clear();
            state.notice = None;
        }
        if ui
            .add_enabled(
                state.draft.dirty && !playing && !state.recording,
                egui::Button::new("Discard changes"),
            )
            .on_disabled_hover_text(
                "There are no changes to discard, or recording/playback is active.",
            )
            .clicked()
        {
            if let Some(name) = &state.draft.saved_name {
                match scene::load(&dir, &module, name) {
                    Ok(scene) => state.draft = Draft::from_scene(scene),
                    Err(e) => state.notice = Some(e),
                }
            } else {
                state.draft = Draft::default();
            }
        }
        if ui
            .add_enabled(
                checked.is_ok() && !playing && !state.recording,
                egui::Button::new("Save scene"),
            )
            .on_disabled_hover_text(
                "Stop recording/playback and fix invalid or empty steps before saving.",
            )
            .clicked()
        {
            match scene::save(&dir, &module, checked.as_ref().unwrap()) {
                Ok(()) => {
                    state.draft.dirty = false;
                    state.draft.saved_name = Some(state.draft.name.clone());
                    state.selected = state.draft.name.clone();
                    state.notice = state.refresh().err();
                }
                Err(e) => state.notice = Some(e),
            }
        }
        ui.vertical(|ui| {
            let idle = !playing && !state.recording;
            let suggested =
                scene::folder(&dir, &module).unwrap().join(format!("{}.json", state.draft.name));
            if ui
                .add_enabled(idle && checked.is_ok(), egui::Button::new("Save As..."))
                .on_disabled_hover_text(
                    "Stop recording/playback and fix invalid or empty steps first.",
                )
                .clicked()
                && let Some(path) = dialogs.save_file(crate::FileKind::LiveScene, Some(&suggested))
            {
                state.notice = (|| -> Result<(), String> {
                    let mut copy = checked.as_ref().unwrap().clone();
                    copy.name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .ok_or("Choose a scene name.")?
                        .into();
                    copy.check()?;
                    let folder = scene::folder(&dir, &module)?;
                    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                    let local = folder.join(format!("{}.json", copy.name));
                    let same_file = path == local
                        || (path.canonicalize().ok().is_some()
                            && path.canonicalize().ok() == local.canonicalize().ok());
                    if local.exists()
                        && !same_file
                        && state.draft.saved_name.as_deref() != Some(&copy.name)
                    {
                        return Err(
                        "That scene name already exists in this module. Choose a different name."
                            .into(),
                    );
                    }
                    export_file(&path, &copy)?;
                    // The chosen JSON is also available in this module's scene list.
                    scene::save(&dir, &module, &copy)?;
                    state.selected = copy.name.clone();
                    state.draft = Draft::from_scene(copy);
                    state.refresh()?;
                    Ok(())
                })()
                .err();
            }
            if ui
                .add_enabled(idle && !state.draft.dirty, egui::Button::new("Import..."))
                .on_disabled_hover_text("Save or discard this draft before importing.")
                .clicked()
                && let Some(path) = dialogs.open_file(crate::FileKind::LiveScene, Some(&suggested))
            {
                match import_file(&path, policy) {
                    Ok(draft) => {
                        state.draft = draft;
                        state.selected.clear();
                        state.notice = Some(
                            "Imported draft. Review it, then Save scene to keep it in this module."
                                .into(),
                        );
                    }
                    Err(e) => state.notice = Some(e),
                }
            }
            if ui
                .add_enabled(idle && checked.is_ok(), egui::Button::new("Export..."))
                .on_disabled_hover_text(
                    "Stop recording/playback and fix invalid or empty steps first.",
                )
                .clicked()
                && let Some(path) = dialogs.save_file(crate::FileKind::LiveScene, Some(&suggested))
            {
                state.notice = Some(
                    export_file(&path, checked.as_ref().unwrap())
                        .err()
                        .unwrap_or_else(|| format!("Exported {}", path.display())),
                );
            }
        });
    })
    .response
    .on_hover_text("New, load, save, import or export a scene JSON file.");
    let before = state.draft.steps.clone();
    ui.add_enabled_ui(!playing && !state.recording, |ui| {
        ui.horizontal(|ui| {
            let label = ui.label("Scene name");
            if ui.text_edit_singleline(&mut state.draft.name).labelled_by(label.id).changed() {
                state.draft.dirty = true;
            }
            if ui
                .add_enabled(checked.is_ok(), egui::Button::new("Save"))
                .on_disabled_hover_text("Add valid steps before saving.")
                .clicked()
            {
                state.notice = save_draft(state, policy).err();
            }
            if state.draft.dirty {
                ui.weak("Unsaved");
            }
        });
    });
    ui.separator();
    ui.strong("1  Record");
    ui.weak("Start recording, then issue commands in the Live view. Stop when ready.");
    if ui
        .add_enabled(
            enabled && !playing,
            egui::Button::new(
                egui::RichText::new(if state.recording { "Recording... / Stop" } else { "Record" })
                    .strong()
                    .color(egui::Color32::LIGHT_RED),
            )
            .min_size(egui::vec2(190., 32.)),
        )
        .on_disabled_hover_text("Enable Live and stop playback before recording.")
        .clicked()
    {
        state.recording = !state.recording;
        state.last_recorded = None;
        state.notice = None;
    }
    if state.recording {
        ui.colored_label(egui::Color32::LIGHT_RED, "Recording commands sent from the Live view");
    }
    if let Some(name) = state.status["playing"].as_str() {
        ui.label(format!("Playing {name}"));
    } else if let Some(last) = state.status["last"].as_object() {
        if last.get("ok") == Some(&json!(true)) {
            ui.label(format!("Completed {} steps", last["steps"].as_array().map_or(0, Vec::len)));
        } else {
            ui.colored_label(
                ui.visuals().error_fg_color,
                last["error"].as_str().unwrap_or("Scene failed"),
            );
        }
    }
    if !enabled {
        ui.weak("Enable Live to record or play scenes.");
    }

    if let Err(e) = &checked {
        if state.draft.steps.is_empty() {
            ui.weak("Example: record a guard saying Hello, then walking to the gate. The two commands appear below with their timing.");
        } else {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    }
    if let Some(e) = &state.notice {
        ui.label(e);
    }
    ui.separator();
    ui.strong("2  Review steps");
    ui.weak("Edit the action, target and delay. Expand Edit step for command inputs and waits.");
    ui.add_enabled_ui(!playing && !state.recording, |ui| {
        let ops: Vec<_> = offered["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["name"] != "live_wait")
            .collect();
        let mut remove = None;
        let mut shift = None;
        egui::ScrollArea::vertical().id_salt("scene-steps").max_height((ui.available_height()-110.).max(140.)).show(ui, |ui| {
            for (i, step) in state.draft.steps.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.group(|ui| {
                        ui.strong(format!("{}. {}", i + 1, summary(step, live.as_deref())));
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Action");
                            egui::ComboBox::from_id_salt("scene-action")
                                .selected_text(step.op.trim_start_matches("live_").replace('_', " "))
                                .show_ui(ui, |ui| {
                                    for op in &ops {
                                        let name = op["name"].as_str().unwrap();
                                        ui.selectable_value(&mut step.op, name.into(), name.trim_start_matches("live_").replace('_', " "));
                                    }
                                });
                            if let Ok(mut input) = serde_json::from_str::<Value>(&step.input)
                                && let Some(target) = input.get("target").and_then(Value::as_str).map(str::to_owned) {
                                let name=live.as_ref().and_then(|s|s.locate(&target)).map(|(_,_,o)|crate::live_panel::name(&o).to_string()).unwrap_or_else(||"Unreported object".into());
                                let mut chosen=target.clone();
                                egui::ComboBox::from_id_salt("scene-target").selected_text(name).show_ui(ui,|ui|{
                                    if let Some(area)=input["area"].as_str().and_then(|a|live.as_ref()?.area(a)) {
                                        for o in area.creatures.iter().chain(&area.placeables).chain(&area.doors).chain(&area.items) {
                                            ui.selectable_value(&mut chosen,o.id.clone(),crate::live_panel::name(o)).on_hover_text(&o.id);
                                        }
                                    }
                                }).response.on_hover_text(format!("Object ID: {target}. Edit step JSON for an unreported target."));
                                if chosen!=target {input["target"]=chosen.into();step.input=serde_json::to_string_pretty(&input).unwrap();}
                            }
                            let label=ui.label("Delay");
                            ui.add(egui::DragValue::new(&mut step.delay_ms).suffix(" ms").range(0..=scene::MAX_DELAY_MS))
                                .labelled_by(label.id).on_hover_text("Time after the preceding step and its wait.");
                        });
                        let wait = match &step.wait {
                            Wait::Nothing => "continue immediately".to_string(),
                            Wait::Result { timeout_ms } => format!(
                                "wait for game result ({} s maximum)",
                                *timeout_ms as f64 / 1000.
                            ),
                            Wait::Near { radius, .. } => {
                                format!("wait until target is within {radius} m")
                            }
                        };
                        ui.weak(format!("After {} s; {wait}", step.delay_ms as f64 / 1000.));
                        egui::CollapsingHeader::new("Edit step")
                            .default_open(step.input == "{}")
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(format!("Step {}", i + 1));
                                    if ui.button("Remove step").clicked() {
                                        remove = Some(i);
                                    }
                                    if ui.add_enabled(i > 0, egui::Button::new("Move up")).on_disabled_hover_text("This is already the first step.").clicked()
                                    {
                                        shift = Some(i);
                                    }
                                });
                                egui::ComboBox::from_id_salt("scene-op")
                                    .width(240.)
                                    .selected_text(&step.op)
                                    .show_ui(ui, |ui| {
                                        for op in &ops {
                                            let name = op["name"].as_str().unwrap();
                                            ui.selectable_value(&mut step.op, name.into(), name);
                                        }
                                    });
                                if let Some(op) = ops.iter().find(|o| o["name"] == step.op) {
                                    ui.weak(op["description"].as_str().unwrap_or_default());
                                    ui.collapsing("Input fields", |ui| {
                                        ui.monospace(
                                            serde_json::to_string_pretty(&op["input"]).unwrap(),
                                        );
                                    });
                                }
                                let label = ui.label("Step input (JSON)");
                                ui.add(
                                    egui::TextEdit::multiline(&mut step.input)
                                        .code_editor()
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(4),
                                )
                                .labelled_by(label.id);
                                ui.horizontal_wrapped(|ui| {
                                    wait_ui(ui, &mut step.wait);
                                });
                            });
                    });
                });
            }
            if let Some(i) = remove {
                state.draft.steps.remove(i);
            }
            if let Some(i) = shift {
                state.draft.steps.swap(i, i - 1);
            }
            if ui
                .add_enabled(
                    state.draft.steps.len() < scene::MAX_STEPS,
                    egui::Button::new("Add step"),
                ).on_disabled_hover_text("Stop recording/playback or remove a step; the scene has reached its step limit.")
                .clicked()
                && let Some(op) =
                    ops.iter().find(|o| o["name"] == "live_npc_say").or_else(|| ops.first())
            {
                state.draft.steps.push(StepDraft {
                    op: op["name"].as_str().unwrap().into(),
                    input: "{}".into(),
                    delay_ms: 0,
                    wait: Wait::Nothing,
                });
            }
        });
    });
    ui.separator();
    ui.strong("3  Replay");
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                enabled && checked.is_ok() && !playing && !state.recording,
                egui::Button::new("Play scene"),
            )
            .on_disabled_hover_text(
                "Enable Live, stop recording, and fix invalid or empty steps before playing.",
            )
            .clicked()
        {
            state.notice =
                state.player.start(checked.as_ref().unwrap().clone(), live.clone().unwrap()).err();
            ui.ctx().request_repaint();
        }
        if ui
            .add_enabled(playing, egui::Button::new("Stop scene"))
            .on_disabled_hover_text(
                "No scene is playing. Use the recording button to stop recording.",
            )
            .clicked()
        {
            state.stop();
        }
    });
    ui.weak("Stop prevents later steps. Commands already sent remain in the game.");
    if before != state.draft.steps {
        state.draft.dirty = true;
    }
}

pub(crate) fn macro_keys() -> [crate::keys::Cmd; 4] {
    use crate::keys::Cmd::*;
    [LiveMacro1, LiveMacro2, LiveMacro3, LiveMacro4]
}
pub(crate) fn macro_held(input: &egui::InputState, keys: &crate::keys::Keymap) -> bool {
    macro_keys().into_iter().any(|cmd| keys.held_but(input, cmd, false, false))
}
pub(crate) fn hotkey(app: &mut Moonglow, ui: &egui::Ui) -> bool {
    if ui.ctx().egui_wants_keyboard_input() {
        return false;
    }
    let pressed = macro_keys().into_iter().position(|cmd| {
        let keys=app.keymap.keys(cmd);
        ui.input(|i|i.events.iter().any(|e|matches!(e,egui::Event::Key {key,pressed:true,repeat:false,modifiers,..} if keys.iter().any(|s|*key==s.logical_key&&modifiers.matches_exact(s.modifiers)))))
    });
    let Some(index) = pressed else { return false };
    let result = if crate::live_targets::enabled(app) {
        app.live
            .clone()
            .ok_or("Connect to a gateway first".into())
            .and_then(|live| app.live_scene.play_hotkey(index, live))
    } else {
        Err("Enable Live commands first".into())
    };
    app.live_scene.notice = result.err();
    ui.ctx().request_repaint();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_keep_unreported_ids_out_of_the_visible_row() {
        let step = StepDraft::from(Step {
            op: "live_npc_move".into(),
            input: json!({"target":"abc","x":40.85216522216797,"y":70}),
            delay_ms: 0,
            wait: Wait::Nothing,
        });
        assert_eq!(summary(&step, None), "Unreported object walks to 40.9, 70.0");
    }

    fn say() -> Command {
        serde_json::from_value(json!({"area":"room","op":"npc_say","target":"7","text":"Hello"}))
            .unwrap()
    }

    #[test]
    fn recording_uses_inter_command_delays_and_scene_validation_preflights_every_step() {
        let mut state = LiveScene { recording: true, ..Default::default() };
        state.record_command(&say(), 1000);
        state.record_command(&say(), 2250);
        assert_eq!(state.draft.steps[0].delay_ms, 0);
        assert_eq!(state.draft.steps[1].delay_ms, 1250);
        let checked = state.draft.checked(Policy::default()).unwrap();
        assert!(checked.steps.iter().all(|s| s.input.get("id").is_none()));
        let mut scripts = LiveScene { recording: true, ..Default::default() };
        for command in [
            json!({"area":"room","op":"run_script","script":"example"}),
            json!({"area":"room","op":"run_chunk","code":"PrintString(\"Hello\");"}),
        ] {
            scripts.record_command(&serde_json::from_value(command).unwrap(), 1000);
        }
        let scripts = scripts.draft.checked(Policy { chunks: true, ..Default::default() }).unwrap();
        assert!(scripts.steps.iter().all(|s| s.input.get("target").is_none()));
        state.draft.steps[1].input =
            r#"{"area":"room","target":"7","text":"Hello","unknown":true}"#.into();
        assert!(
            state
                .draft
                .checked(Policy::default())
                .unwrap_err()
                .contains("Step 2: Unknown property")
        );
        state.draft.steps[1] = Step {
            op: "live_set_faction".into(),
            input: json!({"area":"room","target":"7","faction":-1}),
            delay_ms: 0,
            wait: Wait::Nothing,
        }
        .into();
        assert!(state.draft.checked(Policy::default()).is_err());
        state.draft.steps[1] = Step {
            op: "live_run_chunk".into(),
            input: json!({"area":"room","code":""}),
            delay_ms: 0,
            wait: Wait::Nothing,
        }
        .into();
        assert!(state.draft.checked(Policy::default()).unwrap_err().contains("unavailable"));
        state.record_command(&say(), 700000);
        assert!(
            !state.recording,
            "overlong gaps stop with a visible notice instead of silently changing timing"
        );
    }
    #[test]
    fn cached_preflight_tracks_every_draft_input_and_policy() {
        let mut state = LiveScene { recording: true, ..Default::default() };
        state.record_command(&say(), 1000);
        let policy = Policy::default();
        let offered = state.catalogue(policy);
        assert!(Arc::ptr_eq(&offered, &state.catalogue(policy)));
        let first = state.checked(policy, &offered);
        assert!(first.is_ok());
        assert!(Arc::ptr_eq(&first, &state.checked(policy, &offered)));
        let edits: [fn(&mut Draft); 6] = [
            |d| d.name = "Renamed".into(),
            |d| d.steps[0].delay_ms = 123,
            |d| d.steps[0].wait = Wait::Result { timeout_ms: 1000 },
            |d| d.steps[0].input = r#"{"area":"room","target":"7","text":"Changed"}"#.into(),
            |d| d.steps[0].op = "live_missing".into(),
            |d| d.steps.clear(),
        ];
        let mut previous = first;
        for edit in edits {
            edit(&mut state.draft);
            let next = state.checked(policy, &offered);
            assert!(!Arc::ptr_eq(&previous, &next));
            assert_eq!(next.as_ref(), &state.draft.checked(policy));
            assert!(Arc::ptr_eq(&next, &state.checked(policy, &offered)));
            previous = next;
        }
        let changed = Policy { players: true, chunks: true, ..policy };
        let other = state.catalogue(changed);
        assert!(!Arc::ptr_eq(&offered, &other));
        let next = state.checked(changed, &other);
        assert!(!Arc::ptr_eq(&previous, &next));
        assert_eq!(next.as_ref(), &state.draft.checked(changed));
    }
}
