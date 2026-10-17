//! VFX browser, preview and effect commands for the Live selection.
use crate::{
    Moonglow,
    live::{LiveObject, Op},
    live_targets::{Selection, enabled, selection, send},
};
use mg_area::ObjectKind;
use mg_core::ResRef;
use mg_preview::vfx::{Size, Vfx};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct Row {
    pub id: u16,
    pub label: String,
    pub kind: String,
    pub procedural: bool,
}

#[derive(Debug, Clone)]
struct Appearance {
    selection: Selection,
    /// None restores each target's own first-reported appearance.
    row: Option<u16>,
    module: Option<std::path::PathBuf>,
}

#[derive(Debug)]
pub struct LiveVfx {
    pub rows: Vec<Row>,
    pub search: String,
    pub kind: String,
    pub row: u16,
    pub duration: String,
    pub seconds: f32,
    pub scale: f32,
    pub offset: [f32; 3],
    pub rotation: [f32; 3],
    pub ground: bool,
    pub last_ground: Option<(ResRef, glam::Vec3, String)>,
    pub notice: Option<String>,
    pub pending: Vec<String>,
    pub(crate) preview_row: u16,
    preview_cache: Option<Result<mg_preview::Preview, String>>,
    revision: Option<usize>,
    appearance: Option<Appearance>,
}
impl Default for LiveVfx {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            search: String::new(),
            kind: String::new(),
            row: 411,
            duration: "temporary".into(),
            seconds: 6.,
            scale: 1.,
            offset: [0.; 3],
            rotation: [0.; 3],
            ground: false,
            last_ground: None,
            notice: None,
            pending: Vec::new(),
            preview_row: 411,
            preview_cache: None,
            revision: None,
            appearance: None,
        }
    }
}

/// Called only from the drawn area tab; floating windows never record a ground click.
pub(crate) fn ground(app: &mut Moonglow, ui: &egui::Ui, area: ResRef) {
    if !ui.input(|i| i.pointer.primary_clicked()) {
        return;
    }
    let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else { return };
    if ui.ctx().layer_id_at(pos) != Some(ui.layer_id()) {
        return;
    }
    let Some(view) = app.area_views.get(&area).filter(|v| v.live.on && v.rect.contains(pos)) else {
        return;
    };
    if let Some(at) = view.ground_at(pos, 0.) {
        app.live_vfx.last_ground =
            Some((area, at, app.live.as_ref().map(|s| s.boot()).unwrap_or_default()));
    }
}

pub(crate) fn preview(app: &Moonglow) -> Result<mg_preview::Preview, String> {
    app.live_vfx.preview_cache.clone().unwrap_or_else(|| Err("Select an effect row.".into()))
}

fn build_preview(app: &Moonglow, row: u16) -> Result<mg_preview::Preview, String> {
    let game = app.game.as_deref().ok_or("No game data.")?;
    let table = game.table("visualeffects").map_err(|e| e.to_string())?;
    let vfx = Vfx::read(&table, row as usize).ok_or("No effect row.")?;
    let body = mg_preview::creature_look(game, &mg_preview::CreatureLook::new(6))
        .map_err(|e| e.to_string())?;
    Ok(mg_preview::vfx::with_models(body, &vfx.models(Size::Medium)))
}

impl LiveVfx {
    fn unavailable(
        &self,
        selected: &Selection,
        players: bool,
        clear: bool,
    ) -> Option<&'static str> {
        if !clear {
            let Some(row) = self.rows.iter().find(|r| r.id == self.row) else {
                return Some("Select an effect row.");
            };
            if self.duration != "instant" && row.kind != "D" {
                return Some("Choose Instant for this effect; only D effects support a duration.");
            }
            if self.ground {
                return None;
            }
        }
        if selected.objects.is_empty() {
            return Some("Select a Live object, or aim an effect on the ground.");
        }
        if !selected.objects.iter().any(|(_, o)| allowed(o, players, clear)) {
            return Some(
                "No eligible targets: enable Players for PCs; make static placeables dynamic. DM and possessed creatures are protected.",
            );
        }
        None
    }

    pub(crate) fn invalidate(&mut self) {
        self.revision = None;
        self.rows.clear();
        self.preview_cache = None;
    }

    fn load(&mut self, app: &Moonglow) {
        let Some(game) = &app.game else {
            self.invalidate();
            return;
        };
        let revision = std::sync::Arc::as_ptr(game) as usize;
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.preview_cache = None;
        self.rows.clear();
        if let Ok(table) = game.table("visualeffects") {
            self.rows = (0..table.rows.len().min(65536))
                .filter_map(|id| {
                    let vfx = Vfx::read(&table, id)?;
                    if vfx.label.starts_with("MGLIVE_COLOUR_") {
                        return None;
                    }
                    Some(Row {
                        id: id as u16,
                        label: vfx.label,
                        kind: table.get(id, "Type_FD")?.to_uppercase(),
                        procedural: ["ProgFX_Impact", "ProgFX_Duration", "ProgFX_Cessation"]
                            .iter()
                            .any(|c| table.get_int(id, c).is_some_and(|n| n > 0)),
                    })
                })
                .collect();
        }
    }
    fn apply(&mut self, app: &mut Moonglow, clear: bool) -> Result<(), String> {
        let selected = selection(app)?;
        let players = app.live.as_ref().is_some_and(|s| s.policy().players);
        if let Some(reason) = self.unavailable(&selected, players, clear) {
            return Err(reason.into());
        }
        let mut commands = Vec::new();
        if self.ground && !clear {
            let op = serde_json::from_value(json!({"op":"apply_vfx","row":self.row,"scale":self.scale,"offset":{"x":self.offset[0],"y":self.offset[1],"z":self.offset[2]},"rotation":{"x":self.rotation[0],"y":self.rotation[1],"z":self.rotation[2]},"duration":self.duration,"seconds":if self.duration=="temporary" {Some(self.seconds)} else {None},"location":{"x":0.,"y":0.,"z":0.}})).map_err(|e|e.to_string())?;
            crate::live_aim::arm(
                app,
                selected.area,
                op,
                "Apply VFX at a place",
                crate::live_aim::Source::Vfx,
            )?;
            self.notice =
                Some("Aim the effect marker, then click the map. Esc/right click cancels.".into());
            return Ok(());
        } else {
            for (_, object) in &selected.objects {
                if !allowed(object, players, clear) {
                    continue;
                }
                commands.push(if clear { json!({"op":"clear_vfx","target":object.id}) }
                    else { json!({"op":"apply_vfx","target":object.id,"row":self.row,"scale":self.scale,"offset":{"x":self.offset[0],"y":self.offset[1],"z":self.offset[2]},"rotation":{"x":self.rotation[0],"y":self.rotation[1],"z":self.rotation[2]},"duration":self.duration,"seconds":if self.duration=="temporary" {Some(self.seconds)} else {None}}) });
            }
        }
        if commands.is_empty() {
            return Err("No eligible targets: players need both switches; static placeables must be made dynamic.".into());
        }
        let mut failures = Vec::new();
        self.pending.clear();
        for op in commands {
            let op = serde_json::from_value(op).map_err(|e| e.to_string())?;
            match send(app, selected.area, op) {
                Ok(id) => self.pending.push(id),
                Err(e) => failures.push(e),
            }
        }
        self.notice =
            Some(format!("Queued {} command(s). {}", self.pending.len(), failures.join("; ")));
        Ok(())
    }
}

fn allowed(object: &LiveObject, players: bool, clear: bool) -> bool {
    crate::live_targets::allowed(object, players) && (clear || object.is_static != Some(true))
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    let mut state = std::mem::take(&mut app.live_vfx);
    state.load(app);
    ui.heading("Live VFX");
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "Choose an effect, then apply it to the selection or aim it on the map.",
            )
            .weak(),
        )
        .wrap(),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label("Find effect");
        ui.text_edit_singleline(&mut state.search);
        for (value, name) in [
            ("", "All"),
            ("D", "D - lasting"),
            ("F", "F - instant"),
            ("B", "B - beam"),
            ("P", "P - projectile"),
        ] {
            ui.selectable_value(&mut state.kind, value.into(), name);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for (value, name) in
            [("instant", "Instant"), ("temporary", "Seconds"), ("permanent", "Permanent")]
        {
            ui.selectable_value(&mut state.duration, value.into(), name);
        }
        if state.duration == "temporary" {
            ui.add(egui::DragValue::new(&mut state.seconds).range(0.1..=86400.).suffix(" s"));
        }
    });
    ui.collapsing("Scale / Offset / Rotation", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Scale");
            ui.add(egui::DragValue::new(&mut state.scale).range(0.01..=100.).speed(0.01));
        });
        for (label, vector, limit) in
            [("Offset", &mut state.offset, 1000.), ("Rotation", &mut state.rotation, 3600.)]
        {
            ui.horizontal_wrapped(|ui| {
                ui.label(label);
                for (axis, value) in ["X", "Y", "Z"].into_iter().zip(vector) {
                    ui.add(
                        egui::DragValue::new(value)
                            .range(-limit..=limit)
                            .speed(0.1)
                            .prefix(format!("{axis} ")),
                    );
                }
            });
        }
        ui.weak("Transform values apply to the game effect; the thumbnail shows its base form.");
    });
    let selected = selection(app);
    ui.label(match &selected {
        Ok(s) => format!("{}: {} selected", s.area, s.objects.len()),
        Err(e) => e.clone(),
    })
    .on_hover_text(selected.as_ref().map_or_else(
        |e| e.clone(),
        |s| s.objects.iter().map(|(_, o)| o.name.as_str()).collect::<Vec<_>>().join(", "),
    ));
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut state.ground, false, "Selection");
        ui.selectable_value(&mut state.ground, true, "Aim on the ground");
        let players = app.live.as_ref().is_some_and(|s| s.policy().players);
        let reason = |clear| match &selected {
            Ok(s) => state.unavailable(s, players, clear),
            Err(e) => Some(e.as_str()),
        };
        let apply_reason = reason(false);
        let clear_reason = reason(true);
        if ui
            .add_enabled(apply_reason.is_none(), egui::Button::new("Apply VFX"))
            .on_disabled_hover_text(apply_reason.unwrap_or_default())
            .on_hover_text(if state.ground {
                "Start aiming; click the map to apply. Esc or right click cancels."
            } else {
                "Apply to every eligible selected object."
            })
            .clicked()
            && let Err(e) = state.apply(app, false)
        {
            state.notice = Some(e);
        }
        if ui
            .add_enabled(clear_reason.is_none(), egui::Button::new("Clear VFX (ours)"))
            .on_disabled_hover_text(clear_reason.unwrap_or_default())
            .on_hover_text("Remove effects applied by Moonglow from eligible selected objects.")
            .clicked()
            && let Err(e) = state.apply(app, true)
        {
            state.notice = Some(e);
        }
    });
    if let Some((_, at, _)) = &state.last_ground
        && state.ground
    {
        ui.weak(format!("Ground: {:.1}, {:.1}, {:.1}", at.x, at.y, at.z));
    }
    if let Some(note) = &state.notice {
        ui.label(note);
    }
    for id in &state.pending {
        if let Some(result) = app.live.as_ref().and_then(|s| s.result(id)) {
            ui.label(format!("{id}: {}", if result.ok { "Applied".into() } else { result.err }));
        }
    }
    ui.separator();
    ui.columns(2, |cols| {
        let search = state.search.to_lowercase();
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(&mut cols[0], |ui| {
            let mut matches = 0;
            for row in &state.rows {
                if (state.kind.is_empty() || row.kind == state.kind)
                    && (row.label.to_lowercase().contains(&search) || row.id.to_string() == search)
                {
                    matches += 1;
                    if ui
                        .add(
                            egui::Button::selectable(
                                state.row == row.id,
                                format!("{}  {}  {}", row.id, row.kind, row.label),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&row.label)
                        .clicked()
                    {
                        state.row = row.id;
                        if row.kind != "D" {
                            state.duration = "instant".into();
                        }
                    }
                }
            }
            if matches == 0 {
                ui.weak(if state.rows.is_empty() {
                    "No effects loaded. Open a module with game resources."
                } else {
                    "No matching effects. Clear the search or choose All."
                });
            }
        });
        let ui = &mut cols[1];
        ui.spacing_mut().combo_width = (ui.available_width() - 80.).clamp(60., 120.);
        if let Some(row) = state.rows.iter().find(|r| r.id == state.row) {
            ui.heading(&row.label);
            if row.procedural {
                let color = app
                    .game
                    .as_ref()
                    .map(|g| crate::live_effects::Effect::read(g, row.id as i32).color)
                    .unwrap_or(egui::Color32::LIGHT_BLUE);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 42.),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, 4., color);
                ui.weak("Procedural effect: representative colour swatch.");
            }
            if app.viewport.is_some() && app.game.is_some() {
                if state.preview_row != state.row || state.preview_cache.is_none() {
                    state.preview_row = state.row;
                    state.preview_cache = Some(build_preview(app, state.row));
                }
                app.live_vfx.preview_cache = state.preview_cache.clone();
                crate::model_view::ui(app, ui, crate::model_view::Source::LiveVfx);
            } else {
                ui.weak("Animated mannequin preview needs game resources and a renderer.");
            }
        } else {
            ui.label("Load visualeffects.2da from the configured game or module.");
        }
    });
    app.live_vfx = state;
}

/// One small interception in the creature gallery keeps offline editing intact.
pub(crate) fn appearance(
    app: &mut Moonglow,
    ui: &mut egui::Ui,
    state: &mut crate::appearance_gallery::Gallery,
) -> bool {
    if state.kind == crate::appearance_gallery::Kind::Door {
        return false;
    }
    let live_selected = app
        .palette
        .area
        .and_then(|a| app.area_views.get(&a))
        .is_some_and(|v| v.live.on && v.live.selection_ids().next().is_some());
    if !live_selected {
        return false;
    }
    let mut selected = match selection(app) {
        Ok(s) => s,
        Err(e) => {
            ui.label(e);
            return true;
        }
    };
    let players = app.live.as_ref().is_some_and(|s| s.policy().players);
    let expected = if state.kind == crate::appearance_gallery::Kind::Creature {
        ObjectKind::Creature
    } else {
        ObjectKind::Placeable
    };
    selected
        .objects
        .retain(|(kind, o)| *kind == expected && crate::live_targets::allowed(o, players));
    if selected.objects.is_empty() {
        ui.label("Select a live object matching this gallery's kind.");
        return true;
    }
    ui.label("Live selection: click an appearance, then confirm the named target(s).");
    let can_restore = selected.objects.iter().all(|(_, o)| o.app_original.is_some());
    if ui.add_enabled(can_restore, egui::Button::new("Restore original appearance"))
        .on_hover_text("Restore each target's appearance when first reported in this module run. Older servers may not report it.")
        .clicked()
    {
        app.live_vfx.appearance = Some(Appearance { selection: selected.clone(), row: None, module: app.module_path() });
    }
    let current = selected.objects.first().and_then(|(_, o)| o.app).map(i64::from);
    if let Some(row) = crate::appearance_gallery::grid(app, ui, state, current, 0) {
        if let Ok(row) = u16::try_from(row) {
            app.live_vfx.appearance =
                Some(Appearance { selection: selected, row: Some(row), module: app.module_path() });
        } else {
            app.live_vfx.notice = Some("Appearance row exceeds the game's supported range.".into());
        }
    }
    if let Some(note) = &app.live_vfx.notice {
        ui.label(note);
    }
    true
}

pub(crate) fn confirm_appearance(app: &mut Moonglow, ctx: &egui::Context) {
    let Some(pending) = app.live_vfx.appearance.clone() else { return };
    let same = app.module_path() == pending.module
        && enabled(app)
        && app.live.as_ref().is_some_and(|s| {
            s.boot() == pending.selection.boot
                && s.area(&pending.selection.area.to_string())
                    .is_some_and(|a| mg_live::now_ms().saturating_sub(a.received_ms) <= 10_000)
        });
    let mut close = false;
    let mut open = true;
    egui::Window::new("Change live appearance?")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(pending.row.map_or_else(
                || "Restore original appearance for:".into(),
                |row| format!("Appearance {row} for:"),
            ));
            for (_, o) in &pending.selection.objects {
                ui.label(format!("{} ({})", o.name, o.id));
                if pending.row.is_none() {
                    ui.weak(format!(
                        "Original row {}",
                        o.app_original.map_or_else(|| "unknown".into(), |row| row.to_string())
                    ));
                }
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(same, egui::Button::new("Confirm appearance")).clicked() {
                    let mut errors = Vec::new();
                    for (_, o) in &pending.selection.objects {
                        let Some(row) = pending.row.or(o.app_original) else { continue };
                        if let Err(e) = send(
                            app,
                            pending.selection.area,
                            Op::SetAppearance { target: o.id.clone(), row },
                        ) {
                            errors.push(e);
                        }
                    }
                    app.live_vfx.notice = Some(if errors.is_empty() {
                        "Appearance queued; readback follows in Live.".into()
                    } else {
                        errors.join("; ")
                    });
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close || !open || !same {
        app.live_vfx.appearance = None;
    }
}

/// Particle simulation for effects attached to the mannequin. The viewer already
/// handles the base model, mesh animation and chunk drawing.
#[derive(Debug, Default)]
pub(crate) struct PreviewParticles {
    sims: std::collections::HashMap<usize, mg_render::particles::Particles>,
    pub vertices: usize,
    bounds: Option<(glam::Vec3, glam::Vec3)>,
    elapsed: f32,
    framed: bool,
}
pub(crate) struct ParticleFrame {
    pub batches: Vec<mg_render::particles::ParticleBatch>,
    pub chunks: Vec<mg_render::particles::Chunk>,
    pub live: bool,
    pub reframe: bool,
}
impl PreviewParticles {
    pub(crate) fn bounds(&self, min: glam::Vec3, max: glam::Vec3) -> (glam::Vec3, glam::Vec3) {
        self.bounds.map_or((min, max), |(low, high)| (min.min(low), max.max(high)))
    }
    pub(crate) fn step(
        &mut self,
        composed: &mg_preview::compose::Composed,
        animation: Option<&str>,
        time: (f32, f32),
        stand: glam::Mat4,
        camera: glam::Mat4,
    ) -> ParticleFrame {
        let mut frame =
            ParticleFrame { batches: Vec::new(), chunks: Vec::new(), live: false, reframe: false };
        for (index, source) in composed.all_emitters(animation, time.0).into_iter().enumerate() {
            if source.is_base {
                continue;
            }
            let sim = self
                .sims
                .entry(index)
                .or_insert_with(|| mg_render::particles::Particles::new(source.model));
            let at = stand * source.place;
            sim.update(source.model, source.playing, time.0, time.1, &source.pose, at);
            frame.batches.extend(sim.batches(
                source.model,
                source.playing,
                time.0,
                &source.pose,
                at,
                camera,
            ));
            frame.chunks.extend(sim.chunks(source.model, source.playing, time.0, &source.pose, at));
            frame.live |= sim.live();
        }
        let mut low = glam::Vec3::splat(f32::MAX);
        let mut high = glam::Vec3::splat(f32::MIN);
        for v in frame.batches.iter().flat_map(|b| &b.vertices) {
            let p = glam::Vec3::from(v.pos);
            if p.is_finite() {
                low = low.min(p);
                high = high.max(p);
            }
        }
        self.bounds = (low.x <= high.x).then_some((low, high));
        self.elapsed += time.1;
        frame.reframe = !self.framed && self.elapsed >= 0.25 && self.bounds.is_some();
        self.framed |= frame.reframe;
        self.vertices = frame.batches.iter().map(|b| b.vertices.len()).sum();
        frame
    }
}

/// Animated VFX meshes often have zero alpha in their rest pose. Composed
/// instances supply their moving nodes; apply their own mesh controllers too.
pub(crate) fn animate_meshes(instances: &mut [mg_render::Instance], time: f32) {
    for instance in instances.iter_mut().skip(1) {
        let model = &instance.model;
        if let Some(animation) =
            model.model.animation("duration").or_else(|| model.model.animation("impact"))
        {
            instance.state =
                Some(std::sync::Arc::new(mg_render::anim::mesh_state(model, animation, time)));
        }
    }
}
