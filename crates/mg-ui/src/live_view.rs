//! The running game's objects in an area view. Commands never edit the GIT.
use std::collections::{HashMap, HashSet};
use std::f32::consts::FRAC_PI_2;

use egui::{Color32, Pos2, Rect, Stroke};
use mg_area::{AreaObject, ObjectKind};
use mg_core::ResType;
use mg_resman::ResKey;

mod looks;

use crate::Moonglow;
use crate::area_view::{self, AreaView};
use crate::live::{AreaLive, Command, GitObject, LiveObject, Op};
use crate::live_colors::{ColorMode, Palette};
use crate::live_effects::{Effect, EffectsMode};

#[derive(Debug, Clone)]
struct LiveLook {
    object: AreaObject,
    size: mg_preview::vfx::Size,
}

#[derive(Debug, Clone)]
pub enum Aim {
    Move(String),
    Run(String),
    Attack(String),
    Spawn(ResKey),
}

#[derive(Debug)]
pub struct ScriptDraft {
    pub targets: Vec<String>,
    pub target: Option<String>,
    pub text: String,
    pub chunk: bool,
    pub wrap: bool,
}

#[derive(Debug)]
pub struct Destination {
    pub target: String,
    pub command: String,
    pub at: glam::Vec3,
    pub started_ms: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LabelMode {
    All,
    #[default]
    NearSelected,
    Hover,
    None,
}

impl LabelMode {
    pub const ALL: [Self; 4] = [Self::All, Self::NearSelected, Self::Hover, Self::None];

    pub fn name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::NearSelected => "Near and selected",
            Self::Hover => "Hover only",
            Self::None => "None",
        }
    }

    fn shows(self, selected: bool, hovered: bool, distance: f32) -> bool {
        match self {
            Self::All => true,
            Self::NearSelected => selected || distance <= 15.,
            Self::Hover => hovered,
            Self::None => false,
        }
    }
}

#[derive(Debug, Default)]
pub struct LiveView {
    report: Option<std::sync::Arc<AreaLive>>,
    report_source: std::sync::Weak<crate::live::LiveStore>,
    report_stamp: Option<(String, u64)>,
    pub(crate) cached: Option<LiveFrame>,
    /// Diagnostic CPU timing: report/view preparation and live model poses (ms).
    pub frame_cpu_ms: [f64; 2],
    pub report_reads: u64,
    pub(crate) notes: HashMap<String, String>,
    note_revision: Option<u64>,
    pub reconciliations: u64,
    pub(crate) patrol: crate::live_patrol::Patrol,
    pub patrol_paths: usize,
    pub on: bool,
    pub(crate) layout_on: bool,
    pub labels: LabelMode,
    pub colors: ColorMode,
    pub effects: EffectsMode,
    /// Short-lived spell/projectile/damage cues currently alive.
    pub transient_effects: usize,
    /// Speakers whose most recent chat message is less than four seconds old.
    pub speech_bubbles: usize,
    pub(crate) cues: crate::live_cues::Cues,
    pub filter: crate::live_panel::LiveFilter,
    pub hide_module_markers: bool,
    pub markers: crate::live_markers::Markers,
    pub trail: bool,
    pub(crate) motion: crate::live_motion::Motion,
    sample_time: f64,
    pub selected: Option<String>,
    pub(crate) paste: Option<crate::live_clipboard::Paste>,
    pub(crate) removal: Option<crate::live_delete::Confirmation>,
    pub(crate) properties: crate::live_props::Properties,
    pub(crate) group: crate::live_group::Group,
    pub aim: Option<Aim>,
    pub(crate) drag: Option<crate::live_drag::Drag>,
    pub(crate) trigger_from: Option<glam::Vec3>,
    pub(crate) place_aim: Option<crate::live_aim::Place>,
    pub(crate) teleport: Option<crate::live_teleport::Draft>,
    pub(crate) teleports: Vec<crate::live_teleport::Pending>,
    pub say: Option<(String, String)>,
    pub(crate) player_dialog: Option<crate::live_player::Dialog>,
    pub script: Option<ScriptDraft>,
    pub destinations: Vec<Destination>,
    player_looks: HashMap<String, (Option<i32>, serde_json::Value, Option<LiveLook>)>,
    bad_looks: HashSet<String>,
    looks: HashMap<(ObjectKind, String, Option<u16>), Option<LiveLook>>,
    revision: Option<u64>,
    effect_rows: HashMap<i32, Effect>,
    particle_boot: String,
}

impl LiveView {
    #[cfg(feature = "measure")]
    pub(crate) fn measure_effect_models(&self) -> usize {
        self.cached.as_ref().map_or(0, |f| f.extras.iter().map(|e| e.effect_models).sum())
    }
    #[cfg(feature = "measure")]
    pub(crate) fn measure_memory(&self) -> serde_json::Value {
        serde_json::json!({"looks": self.looks.len(), "player_looks": self.player_looks.len(),
            "effect_rows": self.effect_rows.len(), "notes":self.notes.len(),"bad_looks":self.bad_looks.len(),
            "extras": self.cached.as_ref().map_or(0, |f| f.extras.len()),
            "effect_models": self.measure_effect_models()})
    }

    /// Read the bounded status stamp; clone a report only when the server advances.
    /// Keep this UI cache separate from the store owned by the live service.
    pub(crate) fn report(
        &mut self,
        store: &std::sync::Arc<crate::live::LiveStore>,
        area: mg_core::ResRef,
    ) -> (Option<std::sync::Arc<AreaLive>>, bool) {
        #[cfg(feature = "measure")]
        let _measure = crate::measure::span("live.report");
        store.expire(now_ms());
        let stamp = store.revision();
        let source = std::sync::Arc::downgrade(store);
        let new_source = !self.report_source.ptr_eq(&source);
        let changed = new_source || self.report_stamp.as_ref() != Some(&stamp);
        if new_source || self.report_stamp.as_ref().is_some_and(|old| old.0 != stamp.0) {
            self.drag = None;
            self.trigger_from = None;
            self.player_looks.clear();
            self.bad_looks.clear();
        }
        if new_source {
            self.cached = None;
            self.motion = Default::default();
            self.cues = Default::default();
            self.patrol = Default::default();
        }
        if changed {
            self.report = store.area(&area.to_string()).map(std::sync::Arc::new);
            self.report_reads += 1;
            self.report_source = source;
            self.report_stamp = Some(stamp);
        }
        let note_revision = store.assistant_revision().0;
        if changed || self.note_revision != Some(note_revision) {
            self.notes = store.area_notes(&area.to_string());
            self.note_revision = Some(note_revision);
        }
        (self.report.clone(), changed)
    }

    pub fn selection_ids(&self) -> impl Iterator<Item = &str> {
        self.group.ids(&self.selected)
    }
    pub fn is_selected(&self, id: &str) -> bool {
        self.group.contains(&self.selected, id)
    }
    pub fn select(&mut self, id: Option<String>, toggle: bool) {
        self.group.select(&mut self.selected, id, toggle);
    }
    /// The preview currently displayed for a reported object, for inspection.
    pub fn preview(&self, id: &str) -> Option<&mg_preview::Preview> {
        self.cached.as_ref()?.extras.iter().find(|e| e.id == id)?.object.preview.as_ref()
    }
}

#[derive(Debug)]
pub(crate) struct LiveFrame {
    live: AreaLive,
    pub hidden: Vec<usize>,
    pub extras: Vec<LiveExtra>,
    pub moving: bool,
    pub key: LiveDrawn,
    markers: HashSet<String>,
    palette: Palette,
    effects: HashMap<String, Vec<Effect>>,
}

impl LiveFrame {
    pub(crate) fn allows_cue(&self, id: &str) -> bool {
        self.markers.contains(id)
    }
}

#[derive(Debug)]
pub(crate) struct LiveExtra {
    pub id: String,
    pub object: AreaObject,
    pub dead: bool,
    pub combat: bool,
    size: mg_preview::vfx::Size,
    pub effect_models: usize,
    source: usize,
    pub animates: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LiveDrawn {
    source: usize,
    boot: String,
    seq: u64,
    received_ms: u64,
    note_revision: Option<u64>,
    moving: bool,
    filter: crate::live_panel::LiveFilter,
    selected: Option<String>,
    effects: EffectsMode,
    cues: (u64, usize),
    edit: Option<(String, glam::Vec3, f32)>,
    revision: Option<u64>,
    colors: Option<std::collections::BTreeMap<u32, [u8; 3]>>,
    eye: Option<glam::Vec3>,
}

pub(crate) use mg_live::now_ms;

pub(crate) fn toolbar(app: &mut Moonglow, ui: &mut egui::Ui, view: &mut AreaView) {
    ui.add_enabled_ui(app.live_service.is_some(), |ui| {
        ui.toggle_value(&mut view.live.on, "📡 Live")
            .on_disabled_hover_text("Turn on live mode in Options › Live")
            .on_hover_text("Show the running game; actions send commands to the server");
    });
    crate::live_agents::toolbar(app, ui);
    if view.live.on {
        // Keep the Live controls usable with the palette docked beside a narrow view.
        ui.end_row();
        egui::ComboBox::from_id_salt(("live-labels", view.area))
            .selected_text(format!("Labels: {}", view.live.labels.name()))
            .show_ui(ui, |ui| {
                for mode in LabelMode::ALL {
                    ui.selectable_value(&mut view.live.labels, mode, mode.name());
                }
            });
        egui::ComboBox::from_id_salt(("live-colors", view.area))
            .selected_text(format!("Colours: {}", view.live.colors.name()))
            .show_ui(ui, |ui| {
                for mode in ColorMode::ALL {
                    ui.selectable_value(&mut view.live.colors, mode, mode.name());
                }
            })
            .response
            .on_hover_text("Faction colours, hostility to the selected player, or object kind");
        if ui.button("Live Panel").clicked() {
            app.actions.push(crate::Action::OpenTab(crate::Tab::LivePanel));
        }
        if ui
            .button(format!("{} selected", view.live.selection_ids().count()))
            .on_hover_text("Open Selection. Drag empty ground for a box; Shift adds. Ctrl+click toggles; Ctrl+drag pans.")
            .clicked()
        {
            app.actions.push(crate::Action::OpenTab(crate::Tab::LiveSelection));
        }
        if ui.available_size_before_wrap().x < 370. {
            ui.end_row();
        }
        if ui
            .button("Live Properties")
            .on_hover_text("Numeric properties of the selected live placeable or door")
            .clicked()
        {
            let suitable = view.live.selection_ids().count() == 1
                && view
                    .live
                    .selected
                    .as_ref()
                    .and_then(|id| app.live.as_ref()?.locate(id))
                    .is_some_and(|(_, kind, _)| {
                        matches!(kind, ObjectKind::Placeable | ObjectKind::Door)
                    });
            if suitable {
                view.live.properties.open = true;
            } else {
                view.notice =
                    Some("Select one live placeable or door to open Live Properties.".into());
            }
        }
        if ui
            .button("Legend")
            .on_hover_text("Marker visibility for this area, in the Legend window.")
            .clicked()
        {
            app.palette.area = Some(view.area);
            app.actions.push(crate::Action::OpenTab(crate::Tab::LiveLegend));
        }
        ui.toggle_value(&mut view.live.trail, "Trail")
            .on_hover_text("Show the selected object's last five seconds of movement");
        egui::ComboBox::from_id_salt(("live-effects", view.area))
            .selected_text(format!("Effects: {}", view.live.effects.name()))
            .show_ui(ui, |ui| {
                for mode in EffectsMode::ALL {
                    ui.selectable_value(&mut view.live.effects, mode, mode.name());
                }
            })
            .response
            .on_hover_text("Show effect markers, or their models and particles with Full");
        if view.live.effects == EffectsMode::Markers
            && view.live.cached.as_ref().is_some_and(|frame| !frame.effects.is_empty())
        {
            ui.end_row();
            ui.weak("Markers hides effect models and particles.");
            if ui.button("Show Full effects").clicked() {
                view.live.effects = EffectsMode::Full;
            }
        }
    }
}

pub(crate) fn frame(app: &mut Moonglow, view: &mut AreaView, now: f64) -> Option<LiveFrame> {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("live.frame");
    if view.live.layout_on != view.live.on {
        view.live.layout_on = view.live.on;
        if view.live.on {
            crate::live_layout::activate(app);
        }
    }
    if !view.live.on {
        crate::live_aim::clear(view);
        view.live.cached = None;
        view.live.report_stamp = None;
        view.live.patrol = Default::default();
        view.live.patrol_paths = 0;
        view.live.motion = Default::default();
        view.live.cues = Default::default();
        view.live.transient_effects = 0;
        view.live.speech_bubbles = 0;
        view.live_particles.clear();
        view.effect_models_shown = 0;
        view.live_particles_shown = 0;
        return None;
    }
    crate::live_teleport::results(app, view);
    let store = app.live.as_ref()?;
    let (report, changed) = view.live.report(store, view.area);
    if changed {
        view.live_tiles.observe(
            store,
            &view.area.to_string(),
            view.model.as_ref().map_or((0, 0), |m| (m.width, m.height)),
        );
    }
    let report = report?;
    let boot = store.boot();
    if changed {
        view.live.group.sync(&mut view.live.selected, &report, &boot);
        view.live.patrol.update(store, &view.area.to_string(), &report);
        view.live.patrol_paths = view.live.patrol.count();
    }
    view.live.properties.sync(store, &report, &boot);
    if view.live.particle_boot != boot {
        view.live_particles.clear();
        view.live.particle_boot.clone_from(&boot);
    }
    view.live.motion.observe(&report, &boot, now);
    view.live.sample_time = now;
    let moving = view.live.motion.moving(now);
    let mut key = LiveDrawn {
        source: std::sync::Arc::as_ptr(store) as usize,
        boot,
        seq: report.seq,
        received_ms: report.received_ms,
        note_revision: view.live.note_revision,
        moving,
        filter: view.live.filter.clone(),
        selected: view.live.selected.clone(),
        effects: view.live.effects,
        cues: (0, 0),
        edit: view.live.properties.key(),
        revision: app.ws.as_ref().map(|ws| ws.revision()),
        colors: app.module_path().and_then(|p| app.settings.live_faction_colors.get(&p).cloned()),
        eye: (view.live.effects == EffectsMode::Full)
            .then(|| view.camera().map_or(glam::Vec3::ZERO, |c| c.eye)),
    };
    // Judge arrival against the raw engine report, before applying drawn poses.
    view.live.destinations.retain(|d| {
        now_ms().saturating_sub(d.started_ms) < 30_000
            && report.find(&d.target).is_some_and(|(_, o)| o.position().distance(d.at) >= 1.)
            && !store.result(&d.command).is_some_and(|r| !r.ok)
    });
    #[cfg(feature = "measure")]
    let cues_measure = crate::measure::span("live.cues");
    if changed {
        view.live.cues.update(
            store,
            &view.area.to_string(),
            &report,
            now_ms(),
            now,
            view.live.effects != EffectsMode::Off,
        );
    } else {
        view.live.cues.advance(&report, now, view.live.effects != EffectsMode::Off);
    }
    view.live.cues.resolve_names(app.game.as_deref()?);
    #[cfg(feature = "measure")]
    drop(cues_measure);
    view.live.transient_effects = view.live.cues.len();
    key.cues = view.live.cues.stamp();
    view.live.speech_bubbles = view.live.cues.speech_len();
    if let Some(mut cached) = view.live.cached.take() {
        let poses_changed = moving || cached.moving || cached.key.edit != key.edit;
        // Motion/dragging changes poses, not report reconciliation or model looks.
        cached.key.moving = key.moving;
        cached.key.edit.clone_from(&key.edit);
        if cached.key == key {
            cached.moving = moving;
            if poses_changed {
                apply_poses(&view.live, &mut cached, now);
            }
            return Some(cached);
        }
    }
    looks::rebuild(app, view, &report, moving, key, now)
}

fn apply_poses(view: &LiveView, frame: &mut LiveFrame, now: f64) {
    for o in frame
        .live
        .creatures
        .iter_mut()
        .chain(&mut frame.live.placeables)
        .chain(&mut frame.live.doors)
        .chain(&mut frame.live.items)
    {
        view.motion.apply(o, now);
        if let Some((at, facing)) = view.properties.pose(&o.id) {
            [o.x, o.y, o.z] = at.to_array();
            o.facing = facing;
        }
    }
    for extra in &mut frame.extras {
        let objects = match extra.object.kind {
            ObjectKind::Creature => &frame.live.creatures,
            ObjectKind::Placeable => &frame.live.placeables,
            ObjectKind::Door => &frame.live.doors,
            ObjectKind::Item => &frame.live.items,
            _ => continue,
        };
        let o = &objects[extra.source];
        extra.object.position = o.position();
        extra.object.rotation = o.facing.to_radians() - FRAC_PI_2;
    }
}

/// Parse the engine's GFF JSON without narrowing the appearance fields.
pub fn player_look(
    game: &mg_rules::GameData,
    look: &serde_json::Value,
) -> Result<AreaObject, String> {
    looks::read_player_look(game, look).map(|look| look.object)
}

pub(crate) fn screen(view: &AreaView, o: &LiveObject) -> Option<Pos2> {
    let position = view
        .live
        .properties
        .pose(&o.id)
        .map(|(at, _)| at)
        .or_else(|| view.live.motion.position(&o.id, view.live.sample_time))
        .unwrap_or(o.position());
    view.screen_pos(position + glam::Vec3::Z * 0.3)
}

pub(crate) fn visible(view: &AreaView, live: &AreaLive, kind: ObjectKind, o: &LiveObject) -> bool {
    let selected = view.live.selected.as_ref().and_then(|id| live.find(id)).map(|(_, o)| o);
    view.show[kind.index()]
        && view.live.filter.matches_with_note(
            kind,
            o,
            selected,
            view.live.notes.get(&o.id).map_or("", String::as_str),
        )
}

fn label_priority(selected: bool, o: &LiveObject) -> u8 {
    if selected {
        0
    } else if o.pc {
        1
    } else if o.combat {
        2
    } else if o.own {
        3
    } else {
        4
    }
}

fn label_opacity(distance: f32, selected: bool) -> f32 {
    if selected { 1. } else { ((80. - distance) / 40.).clamp(0., 1.) }
}

/// Stable priority order; crowded or clipped names give way, never their markers.
fn place_labels(candidates: &[(Rect, u8)], markers: &[Rect], visible: Rect) -> Vec<usize> {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("overlay.label_placement");
    let mut order: Vec<_> = (0..candidates.len()).collect();
    order.sort_by_key(|&i| candidates[i].1);
    let mut placed: Vec<usize> = Vec::new();
    for i in order {
        let rect = candidates[i].0;
        if visible.contains_rect(rect)
            && markers.iter().all(|marker| !rect.intersects(*marker))
            && placed.iter().all(|&j| !rect.expand(3.).intersects(candidates[j].0))
        {
            placed.push(i);
        }
    }
    placed
}

/// Stale targets and filtered objects must not leak lines into the view.
fn combat_links(
    live: &AreaLive,
    visible: impl Fn(ObjectKind, &LiveObject) -> bool,
) -> Vec<(&LiveObject, &LiveObject)> {
    live.creatures
        .iter()
        .filter(|o| o.combat && !o.dead && visible(ObjectKind::Creature, o))
        .filter_map(|o| {
            let (kind, target) = live.find(o.target.as_deref()?)?;
            (o.id != target.id && !target.dead && visible(kind, target)).then_some((o, target))
        })
        .collect()
}

fn dead_marker(painter: &egui::Painter, at: Pos2) {
    for sign in [-1., 1.] {
        let diagonal = egui::vec2(5., 5. * sign);
        painter.line_segment([at - diagonal, at + diagonal], Stroke::new(3., Color32::GRAY));
    }
}

fn filter_notice(ui: &mut egui::Ui, view: &mut AreaView, live: &AreaLive) -> Option<Rect> {
    let (hidden, total) = live.all().fold((0, 0), |(hidden, total), (kind, o)| {
        (hidden + usize::from(!visible(view, live, kind, o)), total + 1)
    });
    if hidden == 0 {
        return None;
    }
    let rect =
        Rect::from_min_size(view.rect.left_top() + egui::vec2(10., 30.), egui::vec2(270., 28.));
    Some(
        ui.scope_builder(
            egui::UiBuilder::new().id_salt("live-filter-notice").max_rect(rect),
            |ui| {
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(220))
                    .inner_margin(4.)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("{hidden} of {total} hidden by filters"));
                            if ui.button("Show all").clicked() {
                                view.live.filter = Default::default();
                                for kind in ObjectKind::ALL {
                                    view.show[kind.index()] = true;
                                }
                            }
                        });
                    })
                    .response
                    .rect
            },
        )
        .inner,
    )
}

pub(crate) fn overlay(
    ui: &mut egui::Ui,
    view: &mut AreaView,
    frame: Option<&LiveFrame>,
    pending: &[String],
) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("live.overlay");
    if !view.live.on {
        return;
    }
    let painter = ui.painter_at(view.rect);
    let now = now_ms();
    let (age, color) = match frame {
        Some(f) => {
            let age = now.saturating_sub(f.live.received_ms);
            (
                format!("Live: {:.1} s", age as f32 / 1000.),
                if age > 3000 { Color32::LIGHT_RED } else { Color32::LIGHT_GREEN },
            )
        }
        None => ("Live: no report for this area".into(), Color32::LIGHT_RED),
    };
    painter.text(
        view.rect.left_top() + egui::vec2(10., 10.),
        egui::Align2::LEFT_TOP,
        age,
        egui::FontId::proportional(14.),
        color,
    );
    let Some(frame) = frame else { return };
    let filter_notice = filter_notice(ui, view, &frame.live);
    crate::live_aim::paint(ui, view);
    crate::live_clipboard::paint(ui, view);
    crate::live_group::paint(ui, view);
    crate::live_patrol::paint(ui, view);
    crate::live_teleport::paint(ui, view);
    crate::live_props::paint(ui, view, &frame.live);
    crate::live_drag::paint(ui, view);
    view.live.cues.paint(ui, view, view.live.sample_time, |id| {
        frame.live.find(id).is_some_and(|(kind, o)| visible(view, &frame.live, kind, o))
    });
    if view.live.trail
        && let Some(id) = &view.live.selected
        && frame.live.find(id).is_some_and(|(kind, o)| visible(view, &frame.live, kind, o))
    {
        let points: Vec<_> = view
            .live
            .motion
            .trail(id, ui.input(|i| i.time))
            .into_iter()
            .filter_map(|p| view.screen_pos(p + glam::Vec3::Z * 0.05))
            .collect();
        if points.len() > 1 {
            painter.add(egui::Shape::line(points, Stroke::new(2., Color32::LIGHT_BLUE)));
        }
    }
    for (attacker, target) in combat_links(&frame.live, |kind, o| {
        view.live.markers.shows("Attack lines")
            && view.live.markers.object(kind, o)
            && visible(view, &frame.live, kind, o)
    }) {
        if let (Some(from), Some(to)) = (screen(view, attacker), screen(view, target)) {
            let direction = (to - from).normalized();
            if from.distance(to) > 30. {
                let from = from + direction * 14.;
                let to = to - direction * 14.;
                painter.extend(egui::Shape::dashed_line(
                    &[from, to],
                    Stroke::new(2., Color32::LIGHT_RED),
                    6.,
                    4.,
                ));
                // A short arrowhead makes the reported attacker→target direction clear.
                let side = egui::vec2(-direction.y, direction.x) * 4.;
                for end in [to - direction * 7. + side, to - direction * 7. - side] {
                    painter.line_segment([to, end], Stroke::new(2., Color32::LIGHT_RED));
                }
            }
        }
    }
    for d in &view.live.destinations {
        if let Some(at) = view.screen_pos(d.at) {
            painter.line_segment(
                [at, at - egui::vec2(0., 26.)],
                Stroke::new(2., Color32::LIGHT_GREEN),
            );
            painter.add(egui::Shape::convex_polygon(
                vec![
                    at - egui::vec2(0., 26.),
                    at + egui::vec2(14., -21.),
                    at - egui::vec2(0., 16.),
                ],
                Color32::LIGHT_GREEN,
                Stroke::NONE,
            ));
        }
    }
    let hovered = ui.input(|i| i.pointer.hover_pos()).and_then(|p| picked(view, &frame.live, p));
    let camera = view.camera();
    let focus = view
        .live
        .selected
        .as_ref()
        .and_then(|id| frame.live.find(id))
        .map(|(_, o)| o.position())
        .or_else(|| camera.map(|c| c.target))
        .unwrap_or_default();
    let mut labels = Vec::new();
    let mut candidates = Vec::new();
    let mut marker_rects: Vec<_> = filter_notice.into_iter().collect();
    for (kind, o) in frame.live.all().filter(|(kind, o)| visible(view, &frame.live, *kind, o)) {
        let Some(at) = screen(view, o) else { continue };
        let selected =
            view.live.selected.as_deref().and_then(|id| frame.live.find(id)).map(|(_, o)| o);
        let color = frame.palette.color(view.live.colors, kind, o, selected);
        if frame.markers.contains(&o.id) && view.live.markers.object(kind, o) {
            let fighting = view.live.markers.shows("Combat rings")
                && kind == ObjectKind::Creature
                && o.combat
                && !o.dead;
            let effects = frame.effects.get(&o.id).map_or(&[][..], Vec::as_slice);
            let radius = if !effects.is_empty() {
                17. + effects.len().min(3) as f32 * 3.
            } else if fighting {
                14.
            } else {
                8.
            };
            for (i, effect) in effects.iter().take(3).enumerate() {
                painter.circle_stroke(at, 17. + i as f32 * 3., Stroke::new(2., effect.color));
            }
            marker_rects.push(Rect::from_center_size(at, egui::Vec2::splat(radius * 2.)));
            if let Some(note) = view.live.notes.get(&o.id) {
                let icon = Rect::from_center_size(at + egui::vec2(14., -14.), egui::vec2(14., 16.));
                painter.rect_filled(icon, 2., Color32::from_rgb(70, 57, 18));
                painter.text(
                    icon.center(),
                    egui::Align2::CENTER_CENTER,
                    "N",
                    egui::FontId::proportional(11.),
                    Color32::LIGHT_YELLOW,
                );
                ui.interact(icon, ui.id().with(("dm-note", &o.id)), egui::Sense::hover())
                    .on_hover_text(format!("DM note: {note}"));
                marker_rects.push(icon);
            }
            if o.dead {
                dead_marker(&painter, at);
            } else {
                painter.circle_filled(at, 6., color);
            }
            if fighting {
                painter.circle_stroke(at, 12., Stroke::new(2., Color32::LIGHT_RED));
            }
            let selected = view.live.is_selected(&o.id);
            let show_name = view.live.labels.shows(
                selected,
                hovered.as_deref() == Some(&o.id),
                o.position().distance(focus),
            );
            if view.live.markers.shows("Labels") && (show_name || !effects.is_empty()) {
                let opacity =
                    label_opacity(camera.map_or(0., |c| c.eye.distance(o.position())), selected);
                if opacity > 0. {
                    let mut name = if show_name {
                        if o.name.is_empty() { o.resref.clone() } else { o.name.clone() }
                    } else {
                        String::new()
                    };
                    if !effects.is_empty() {
                        if !name.is_empty() {
                            name.push('\n');
                        }
                        name.push_str("* ");
                        name.push_str(
                            &effects
                                .iter()
                                .take(2)
                                .map(|e| e.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                        );
                        if effects.len() > 2 {
                            name.push_str(&format!(" +{}", effects.len() - 2));
                        }
                    }
                    let color = color.gamma_multiply(opacity);
                    let galley =
                        painter.layout(name.clone(), egui::FontId::proportional(13.), color, 180.);
                    let rect = Rect::from_min_size(
                        at - egui::vec2(galley.size().x * 0.5, galley.size().y + radius + 2.),
                        galley.size(),
                    );
                    candidates.push((rect, label_priority(selected, o)));
                    labels.push((galley, color));
                }
            }
            if let Some([hp, max]) = o.hp {
                let bar = egui::Rect::from_min_size(
                    at + egui::vec2(-18., radius + 2.),
                    egui::vec2(36., 4.),
                );
                painter.rect_filled(bar, 1., Color32::from_black_alpha(220));
                let ratio = (hp as f32 / max.max(1) as f32).clamp(0., 1.);
                painter.rect_filled(
                    egui::Rect::from_min_size(bar.min, egui::vec2(36. * ratio, 4.)),
                    1.,
                    color,
                );
            }
        }
        if view.live.markers.object(kind, o)
            && view.live.markers.shows("Labels")
            && matches!(kind, ObjectKind::Placeable | ObjectKind::Door)
        {
            let states: Vec<_> = [(o.open == Some(true), "open"), (o.lock == Some(true), "locked")]
                .into_iter()
                .filter_map(|(on, text)| on.then_some(text))
                .collect();
            if !states.is_empty() {
                painter.text(
                    at + egui::vec2(0., 18.),
                    egui::Align2::CENTER_TOP,
                    states.join(", "),
                    egui::FontId::proportional(12.),
                    Color32::WHITE,
                );
            }
        }
        if view.live.is_selected(&o.id) {
            let radius = if o.combat && !o.dead { 15. } else { 10. };
            painter.circle_stroke(at, radius, Stroke::new(2., Color32::WHITE));
        }
        if pending.contains(&o.id) {
            painter.text(
                at + egui::vec2(12., 0.),
                egui::Align2::LEFT_CENTER,
                "⏳",
                egui::FontId::proportional(14.),
                Color32::WHITE,
            );
        }
    }
    let visible = view.rect.intersect(painter.clip_rect());
    view.live.cues.paint_speech(ui, view, view.live.sample_time, &mut marker_rects, |id| {
        frame.live.find(id).and_then(|(kind, o)| {
            self::visible(view, &frame.live, kind, o).then(|| screen(view, o)).flatten()
        })
    });
    for i in place_labels(&candidates, &marker_rects, visible) {
        let (galley, color) = &labels[i];
        painter.galley(candidates[i].0.min, galley.clone(), *color);
    }
    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()).filter(|p| view.rect.contains(*p))
        && let Some(aim) = &view.live.aim
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        let (actor, end, color): (&str, Option<Pos2>, Color32) = match aim {
            Aim::Move(id) | Aim::Run(id) => {
                let ground = view.ground_at(pos, 0.);
                if let Some(at) = ground {
                    let points: Vec<_> = (0..=32)
                        .filter_map(|n| {
                            let angle = n as f32 * std::f32::consts::TAU / 32.;
                            view.screen_pos(at + glam::vec3(angle.cos(), angle.sin(), 0.02) * 0.5)
                        })
                        .collect();
                    painter.add(egui::Shape::line(points, Stroke::new(2., Color32::LIGHT_GREEN)));
                }
                (id, ground.and_then(|p| view.screen_pos(p)), Color32::LIGHT_GREEN)
            }
            Aim::Attack(id) => {
                let victim = attack_target(view, &frame.live, id, pos);
                let end = victim.and_then(|o| screen(view, o));
                if let Some(at) = end {
                    painter.circle_stroke(at, 13., Stroke::new(3., Color32::LIGHT_RED));
                }
                (id, end, Color32::LIGHT_RED)
            }
            Aim::Spawn(key) => {
                if key.restype != ResType::UTT
                    && let Some(at) = view.ground_at(pos, 0.)
                {
                    crate::live_aim::circle(ui, view, view.snapped(at), 0.5, Color32::LIGHT_GREEN);
                }
                ("", None, Color32::LIGHT_GREEN)
            }
        };
        if let (Some(from), Some(end)) =
            (frame.live.find(actor).and_then(|(_, o)| screen(view, o)), end)
        {
            painter.extend(egui::Shape::dashed_line(&[from, end], Stroke::new(2., color), 7., 5.));
        }
        if let Some(text) = aim_text(view, Some(&frame.live), Some(pos))
            .filter(|_| !matches!(view.live.aim,Some(Aim::Spawn(key)) if key.restype==ResType::UTT))
        {
            let visible = view.rect.intersect(painter.clip_rect());
            let galley = painter.layout(
                text,
                egui::FontId::proportional(14.),
                Color32::WHITE,
                (visible.width() - 32.).max(1.),
            );
            let size = galley.size() + egui::vec2(12., 8.);
            let at = egui::pos2(
                (pos.x + 18.).min(visible.right() - size.x - 4.).max(visible.left() + 4.),
                (pos.y + 22.).min(visible.bottom() - size.y - 4.).max(visible.top() + 4.),
            );
            painter.rect_filled(
                egui::Rect::from_min_size(at, size),
                4.,
                Color32::from_black_alpha(230),
            );
            painter.galley(at + egui::vec2(6., 4.), galley, Color32::WHITE);
        }
    }
}

fn attack_target<'a>(
    view: &AreaView,
    live: &'a AreaLive,
    attacker: &str,
    pos: Pos2,
) -> Option<&'a LiveObject> {
    if !view.show[ObjectKind::Creature.index()] {
        return None;
    }
    live.creatures
        .iter()
        .filter(|o| o.id != attacker && !o.dead && visible(view, live, ObjectKind::Creature, o))
        .filter_map(|o| {
            let d = screen(view, o)?.distance(pos);
            (d <= 12.).then_some((d, o))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, o)| o)
}

pub(crate) fn aim_text(
    view: &AreaView,
    live: Option<&AreaLive>,
    pos: Option<Pos2>,
) -> Option<String> {
    Some(match view.live.aim.as_ref()? {
        Aim::Run(_) => "Run here: left click destination; right click / Esc: cancel".into(),
        Aim::Move(_) => {
            "Walk here — left click: go (Shift: run) · right click / Esc: cancel".into()
        }
        Aim::Attack(id) => {
            let victim = live.and_then(|a| attack_target(view, a, id, pos?));
            let name = victim
                .map(|o| if o.name.is_empty() { &o.resref } else { &o.name })
                .map(String::as_str)
                .unwrap_or("a creature");
            format!("Attack {name} — left click: attack · right click / Esc: cancel")
        }
        Aim::Spawn(key) if key.restype == ResType::UTT => {
            format!("Spawn {}: drag a rectangle on the map; right click / Esc cancels", key.resref)
        }
        Aim::Spawn(key) => format!(
            "Spawn {} — left click: place (Shift: keep spawning) · right click / Esc: cancel",
            key.resref
        ),
    })
}

pub(crate) fn picked(view: &AreaView, live: &AreaLive, pos: Pos2) -> Option<String> {
    live.all()
        .filter(|(kind, o)| visible(view, live, *kind, o))
        .filter_map(|(_, o)| {
            let distance = screen(view, o)?.distance(pos);
            (distance <= 12.).then_some((distance, &o.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id.clone())
}

pub(crate) fn queue(app: &mut Moonglow, view: &mut AreaView, op: Op) -> Option<String> {
    queue_in_area(app, view, view.area.to_string(), op)
}

pub(crate) fn queue_in_area(
    app: &mut Moonglow,
    view: &mut AreaView,
    area: String,
    op: Op,
) -> Option<String> {
    if app.live_service.is_none() {
        view.notice = Some("Turn on live mode in Options › Live".into());
        return None;
    }
    let command = Command { id: String::new(), area, op };
    match crate::live_scene::enqueue(app, command) {
        Ok(id) => Some(id),
        Err(e) => {
            view.notice = Some(e.to_string());
            app.log.warn(format!("Live: {e}"));
            None
        }
    }
}

/// Poll even when the area tab or Live view has been closed, and log each result once.
pub(crate) fn results(app: &mut Moonglow) {
    if app.live_commands.is_empty() {
        return;
    }
    let Some(store) = &app.live else { return };
    let status = store.status(now_ms());
    let Some(results) = status["results"].as_array() else { return };
    for item in results {
        let result = &item["result"];
        let Some(id) = result["id"].as_str() else { continue };
        if !app.live_commands.remove(id) {
            continue;
        }
        let error = result["err"].as_str().unwrap_or_default();
        if result["ok"].as_bool() == Some(true) {
            let detail = if error.is_empty() { String::new() } else { format!(": {error}") };
            app.log.info(format!("Live: {id} ok{detail}"));
        } else {
            app.log.warn(format!("Live: {id} failed: {error}"));
        }
    }
    // A long UI pause can outlast the store's bounded result history. Do not
    // retain such ids forever or invent a successful outcome for them.
    if let Some(queued) = status["queued"].as_array() {
        app.live_commands.retain(|id| {
            if queued.iter().any(|command| command["id"].as_str() == Some(id)) {
                true
            } else {
                app.log.warn(format!("Live: {id} result is no longer available"));
                false
            }
        });
    }
}

fn spawn(app: &mut Moonglow, view: &mut AreaView, key: ResKey, pos: Pos2) -> bool {
    let Some(kind) = crate::live_spawn::kind(key.restype) else {
        view.notice = Some("Choose a supported Live blueprint from the palette.".into());
        return false;
    };
    let Some(at) = view.ground_at(pos, 0.).map(|p| view.snapped(p)) else { return false };
    queue(
        app,
        view,
        Op::Spawn {
            kind,
            r: key.resref.to_string(),
            x: at.x,
            y: at.y,
            z: at.z,
            f: (view.ghost_turn.to_degrees() + 90.).rem_euclid(360.),
            count: 1,
            faction: None,
        },
    )
    .is_some()
}

pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
) {
    if crate::live_delete::window(app, view, ui.ctx()) {
        return;
    }
    if crate::live_clipboard::input(app, ui, view, response) {
        return;
    }
    if crate::live_aim::input(app, ui, view, response) {
        return;
    }
    if crate::live_teleport::input(app, ui, view, response) {
        return;
    }
    if crate::live_patrol::input(app, ui, view, response) {
        return;
    }
    let live = view.live.report.clone();
    crate::live_props::window(app, view, ui.ctx(), live.as_deref());
    if let Some(d) = response.dnd_release_payload::<crate::palette_view::Dragged>() {
        crate::live_aim::clear(view);
        view.live.aim = Some(Aim::Spawn(d.0));
        app.palette.selected = None;
        return;
    }
    if let Some(key) = area_view::brush(app) {
        app.palette.selected = None;
        crate::live_aim::clear(view);
        if crate::live_spawn::kind(key.restype).is_some() {
            view.live.aim = Some(Aim::Spawn(key));
            view.notice = None;
        } else {
            view.notice = Some("Choose a creature, placeable, item, waypoint, sound, encounter, store or trigger blueprint".into());
        }
    }
    // Aiming owns this gesture, including cancellation. Never run selection,
    // palette placement or context_menu for a gesture that began while aiming.
    if let Some(aim) = view.live.aim.clone() {
        if let Aim::Spawn(key) = aim
            && key.restype == ResType::UTT
        {
            crate::live_spawn::trigger(app, ui, view, response, key);
            return;
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if matches!(aim, Aim::Spawn(_)) {
                for (cmd, step) in area_view::turn_keys(view) {
                    if ui.input(|i| app.keymap.pressed(i, cmd)) {
                        view.ghost_turn += step;
                    }
                }
            }
        }
        let gesture = crate::live_aim::gesture(ui, response);
        if matches!(gesture, crate::live_aim::Gesture::Cancel) {
            view.live.aim = None;
        } else if let crate::live_aim::Gesture::Click(pos) = gesture {
            match aim {
                Aim::Move(ref target) | Aim::Run(ref target) => {
                    let run = matches!(aim, Aim::Run(_)) || ui.input(|i| i.modifiers.shift);
                    let target = target.clone();
                    if let Some(at) = view.ground_at(pos, 0.) {
                        view.live.aim = None;
                        if let Some(command) = queue(
                            app,
                            view,
                            Op::NpcMove {
                                target: target.clone(),
                                x: at.x,
                                y: at.y,
                                z: at.z,
                                run,
                                hold: false,
                                timeout: 30,
                            },
                        ) {
                            view.live.destinations.retain(|d| d.target != target);
                            view.live.destinations.push(Destination {
                                target,
                                command,
                                at,
                                started_ms: now_ms(),
                            });
                        }
                    }
                }
                Aim::Attack(target) => {
                    if let Some(victim) = live
                        .as_ref()
                        .and_then(|a| attack_target(view, a, &target, pos))
                        .map(|o| o.id.clone())
                    {
                        view.live.aim = None;
                        queue(
                            app,
                            view,
                            Op::NpcAttack {
                                target,
                                victim,
                                hold: true,
                                timeout: 30,
                                faction: None,
                            },
                        );
                    }
                }
                Aim::Spawn(key) => {
                    if spawn(app, view, key, pos) && !ui.input(|i| i.modifiers.shift) {
                        view.live.aim = None;
                    }
                }
            }
        }
        return;
    }
    if crate::live_group::input(app, ui, view, response, live.as_deref()) {
        return;
    }
    if area_view::brush(app).is_none()
        && crate::live_props::input(app, ui, view, response, live.as_deref())
    {
        return;
    }
    if crate::live_drag::input(app, ui, view, response, live.as_deref()) {
        return;
    }
    let keys = response.hovered() && !ui.ctx().egui_wants_keyboard_input();
    if keys && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        view.live.aim = None;
        view.live.say = None;
        app.palette.selected = None;
    }
    if keys && ui.input(|i| i.key_pressed(egui::Key::Delete)) {
        crate::live_delete::request(app, view);
    }
    // Turn only the spawn preview, never a selected module instance.
    if keys && area_view::brush(app).is_some() {
        for (cmd, step) in area_view::turn_keys(view) {
            if ui.input(|i| app.keymap.pressed(i, cmd)) {
                view.ghost_turn += step;
            }
        }
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let clicked = live.as_deref().and_then(|a| picked(view, a, pos));
        if let Some(key) = area_view::brush(app) {
            app.palette.selected = None;
            if crate::live_spawn::kind(key.restype).is_some() {
                view.notice = None;
                view.live.aim = Some(Aim::Spawn(key));
            } else {
                view.notice = Some("Choose a creature, placeable, item, waypoint, sound, encounter, store or trigger blueprint".into());
            }
        } else {
            let toggle = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
            if !toggle && ui.input(|i| i.modifiers.shift) {
                if let Some(id) = clicked
                    && !view.live.is_selected(&id)
                {
                    view.live.select(Some(id), true);
                }
            } else {
                view.live.select(clicked, toggle);
            }
            app.live_panel.follow = None;
        }
    }

    if response.secondary_clicked()
        && let (Some(live), Some(pos)) = (&live, response.interact_pointer_pos())
    {
        if view.live.selection_ids().count() <= 1 {
            view.live.select(picked(view, live, pos), false);
        }
        app.live_panel.follow = None;
    }
    response.context_menu(|ui| menu(app, view, ui, live.as_deref()));
    crate::live_player::window(app, view, ui.ctx());
    say_window(app, view, ui.ctx());
    script_window(app, view, ui.ctx());
}

fn menu(app: &mut Moonglow, view: &mut AreaView, ui: &mut egui::Ui, live: Option<&AreaLive>) {
    if app.live_service.is_none() {
        ui.label("Turn on live mode in Options › Live");
        return;
    }
    if let Some(live) =
        live.filter(|live| view.live.selection_ids().any(|id| live.find(id).is_some()))
    {
        crate::live_context::menu(app, view, ui, live);
        return;
    }
    if let Some(store) = &app.live {
        let policy = store.policy();
        for (allowed, chunk, label) in
            [(policy.scripts, false, "Run Script…"), (policy.chunks, true, "Run NWScript…")]
        {
            if allowed && ui.button(label).clicked() {
                view.live.script = Some(ScriptDraft {
                    targets: Vec::new(),
                    target: None,
                    text: String::new(),
                    chunk,
                    wrap: true,
                });
                ui.close();
            }
        }
        if policy.scripts || policy.chunks {
            ui.separator();
        }
    }
    {
        ui.label("Scripts run on this area");
        let players = app.live.as_ref().is_some_and(|s| s.policy().players);
        if ui
            .add_enabled(players, egui::Button::new("Jump All Players…"))
            .on_disabled_hover_text("Enable Allow actions on players and the server player switch.")
            .clicked()
        {
            crate::live_aim::start(
                app,
                view,
                Op::JumpAllPlayers { x: 0., y: 0., z: 0. },
                "Jump All Players",
                crate::live_aim::Source::Menu,
            );
            ui.close();
        }
        if ui.button("Stop Fighting in Radius…").clicked() {
            crate::live_aim::start(
                app,
                view,
                Op::StopCombat { target: None, x: None, y: None, radius: Some(10.) },
                "Stop Fighting in radius",
                crate::live_aim::Source::Menu,
            );
            ui.close();
        }
        let mut time = None;
        crate::live_menu::values(ui, "Set Time", |ui| {
            let id = ui.make_persistent_id((view.area, "live-time"));
            let mut values: [u8; 2] = ui.data(|d| d.get_temp(id)).unwrap_or([12, 0]);
            ui.horizontal(|ui| {
                let label = ui.label("Hour");
                ui.add(egui::DragValue::new(&mut values[0]).range(0..=23)).labelled_by(label.id);
                ui.label(":");
                let label = ui.label("Minute");
                ui.add(egui::DragValue::new(&mut values[1]).range(0..=59)).labelled_by(label.id);
            });
            ui.data_mut(|d| d.insert_temp(id, values));
            if ui.button("Set Server Time").clicked() {
                time = Some((values[0], values[1]));
            }
            ui.separator();
            for hour in [0, 6, 12, 18] {
                if ui.button(format!("{hour:02}:00")).clicked() {
                    time = Some((hour, 0));
                }
            }
        });
        if let Some((hour, minute)) = time {
            queue(app, view, Op::SetTime { hour, minute });
            ui.close();
        }
    };
}

fn script_window(app: &mut Moonglow, view: &mut AreaView, ctx: &egui::Context) {
    let Some(mut draft) = view.live.script.take() else { return };
    let title = if draft.chunk { "Run NWScript…" } else { "Run Script…" };
    let mut open = true;
    let mut send = false;
    let mut cancel = false;
    let allowed = app.live_service.is_some()
        && app.live.as_ref().is_some_and(|store| {
            if draft.chunk { store.policy().chunks } else { store.policy().scripts }
        });
    egui::Window::new(title)
        .id(egui::Id::new(("live-script", view.area)))
        .open(&mut open)
        .collapsible(false)
        .default_width(460.)
        .show(ctx, |ui| {
            ui.label(format!("Target: {}", draft.target.as_deref().unwrap_or("this area")));
            if draft.chunk {
                ui.add(
                    egui::TextEdit::multiline(&mut draft.text)
                        .desired_rows(10)
                        .desired_width(f32::INFINITY)
                        .char_limit(8000)
                        .font(egui::FontId::monospace(app.settings.script_style.font_size as f32))
                        .hint_text("NWScript code"),
                );
                ui.checkbox(&mut draft.wrap, "Wrap in void main()");
            } else {
                ui.label("Compiled module script (resref)");
                ui.add(
                    egui::TextEdit::singleline(&mut draft.text)
                        .char_limit(16)
                        .hint_text("script_name"),
                );
            }
            ui.horizontal(|ui| {
                send = ui
                    .add_enabled(allowed && !draft.text.trim().is_empty(), egui::Button::new("OK"))
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if !allowed {
                ui.label("This command is disabled in Options › Live");
            }
        });
    if send {
        let targets = if draft.targets.is_empty() {
            vec![draft.target]
        } else {
            draft.targets.into_iter().map(Some).collect()
        };
        for target in targets {
            let op = if draft.chunk {
                Op::RunChunk {
                    target,
                    code: draft.text.clone(),
                    wrap: draft.wrap,
                    return_value: false,
                }
            } else {
                Op::RunScript { target, script: draft.text.clone() }
            };
            queue(app, view, op);
        }
    } else if open && !cancel {
        view.live.script = Some(draft);
    }
}

fn say_window(app: &mut Moonglow, view: &mut AreaView, ctx: &egui::Context) {
    let Some((target, mut text)) = view.live.say.take() else { return };
    let volume_id = egui::Id::new(("live-say-volume", view.area));
    let mut volume: crate::live::SpeechVolume =
        ctx.data(|d| d.get_temp(volume_id)).unwrap_or_default();
    let mut open = true;
    let mut send = false;
    egui::Window::new("Say…")
        .id(egui::Id::new(("live-say", view.area)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.text_edit_singleline(&mut text);
            ui.horizontal(|ui| {
                use crate::live::SpeechVolume as V;
                for (label, v) in [("Talk", V::Talk), ("Whisper", V::Whisper), ("Shout", V::Shout)]
                {
                    ui.radio_value(&mut volume, v, label);
                }
            });
            send = ui.button("OK").clicked();
        });
    ctx.data_mut(|d| d.insert_temp(volume_id, volume));
    if send {
        queue(app, view, Op::NpcSay { target, text, volume });
    } else if open {
        view.live.say = Some((target, text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_links_require_a_living_visible_pair_and_current_combat() {
        let attacker = LiveObject {
            id: "1".into(),
            combat: true,
            target: Some("2".into()),
            ..Default::default()
        };
        let target = LiveObject { id: "2".into(), ..Default::default() };
        let mut live = AreaLive { creatures: vec![attacker, target], ..Default::default() };
        assert_eq!(combat_links(&live, |_, _| true).len(), 1);
        assert!(combat_links(&live, |_, o| o.id == "1").is_empty());
        assert!(combat_links(&live, |_, o| o.id == "2").is_empty());
        live.creatures[1].dead = true;
        assert!(combat_links(&live, |_, _| true).is_empty());
        live.creatures[1].dead = false;
        live.creatures[0].dead = true;
        assert!(combat_links(&live, |_, _| true).is_empty());
        live.creatures[0].dead = false;
        live.creatures[0].combat = false;
        assert!(combat_links(&live, |_, _| true).is_empty());
        live.creatures[0].combat = true;
        for target in [None, Some("1".into()), Some("3".into())] {
            live.creatures[0].target = target;
            assert!(combat_links(&live, |_, _| true).is_empty());
        }
    }

    #[test]
    fn labels_keep_priority_and_never_overlap() {
        let mut candidates = Vec::new();
        for group in 0..4 {
            // Reverse report order must not give an ordinary NPC priority.
            for priority in (0..5).rev() {
                let at = egui::pos2(10. + group as f32 * 110., 10.);
                candidates.push((Rect::from_min_size(at, egui::vec2(100., 20.)), priority));
            }
        }
        let visible = Rect::from_min_size(Pos2::ZERO, egui::vec2(500., 100.));
        let placed = place_labels(&candidates, &[], visible);
        assert_eq!(placed, [4, 9, 14, 19]);
        for (n, &i) in placed.iter().enumerate() {
            for &j in &placed[n + 1..] {
                assert!(!candidates[i].0.intersects(candidates[j].0));
            }
        }
        let mut object: LiveObject = serde_json::from_value(serde_json::json!({"id":"1"})).unwrap();
        assert_eq!(label_priority(false, &object), 4);
        object.own = true;
        assert_eq!(label_priority(false, &object), 3);
        object.combat = true;
        assert_eq!(label_priority(false, &object), 2);
        object.pc = true;
        assert_eq!(label_priority(false, &object), 1);
        assert_eq!(label_priority(true, &object), 0);
    }

    #[test]
    fn labels_reserve_a_gap_and_stay_inside_the_view() {
        let r = |x| Rect::from_min_size(egui::pos2(x, 10.), egui::vec2(20., 20.));
        let visible = Rect::from_min_size(Pos2::ZERO, egui::vec2(100., 100.));
        assert_eq!(
            place_labels(
                &[(r(-1.), 0), (r(5.), 1), (r(26.), 2), (r(29.), 3), (r(85.), 0)],
                &[],
                visible
            ),
            [1, 3]
        );
    }

    #[test]
    fn labels_leave_other_objects_markers_clear() {
        let label = Rect::from_min_size(egui::pos2(20., 10.), egui::vec2(80., 20.));
        let marker = Rect::from_center_size(egui::pos2(60., 22.), egui::vec2(16., 16.));
        let visible = Rect::from_min_size(Pos2::ZERO, egui::vec2(200., 200.));
        assert!(place_labels(&[(label, 0)], &[marker], visible).is_empty());
        assert_eq!(
            place_labels(&[(label, 0)], &[marker.translate(egui::vec2(0., 40.))], visible),
            [0]
        );
    }

    #[test]
    fn label_modes_and_distance_fade() {
        assert_eq!(LabelMode::default(), LabelMode::NearSelected);
        assert!(LabelMode::NearSelected.shows(false, false, 15.));
        assert!(!LabelMode::NearSelected.shows(false, true, 15.01));
        assert!(LabelMode::NearSelected.shows(true, false, 100.));
        assert!(LabelMode::All.shows(false, false, 100.));
        assert!(!LabelMode::Hover.shows(true, false, 0.));
        assert!(LabelMode::Hover.shows(false, true, 100.));
        assert!(!LabelMode::None.shows(true, true, 0.));
        assert_eq!(label_opacity(40., false), 1.);
        assert_eq!(label_opacity(60., false), 0.5);
        assert_eq!(label_opacity(80., false), 0.);
        assert_eq!(label_opacity(100., true), 1.);
    }
}
