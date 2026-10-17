//! Numeric properties and release-to-send manipulation of reported objects.
use crate::Moonglow;
use crate::area_view::{self, AreaView, TurnRing};
use crate::live::{AreaLive, LiveObject, Op, VisualTransform};
use crate::live_view::{now_ms, picked, queue};
use egui::{Color32, Pos2, Stroke};
use glam::Vec3;
use mg_area::ObjectKind;

#[derive(Debug, Clone)]
enum Gesture {
    Move(Vec3),
    Ring(f32),
    Turn(Pos2),
    Keys,
}

#[derive(Debug, Clone)]
struct Edit {
    id: String,
    at: Vec3,
    facing: f32,
    original_at: Vec3,
    original_facing: f32,
    gesture: Gesture,
}

#[derive(Debug)]
struct Pending {
    edit: Edit,
    command: String,
    until: u64,
}

#[derive(Debug)]
struct Draft {
    id: String,
    name: String,
    useable: bool,
    transform: VisualTransform,
}

#[derive(Debug, Default)]
pub(crate) struct Properties {
    pub open: bool,
    draft: Option<Draft>,
    edit: Option<Edit>,
    pending: Option<Pending>,
    boot: String,
}

impl Properties {
    pub(crate) fn pose(&self, id: &str) -> Option<(Vec3, f32)> {
        self.edit
            .as_ref()
            .or_else(|| self.pending.as_ref().map(|p| &p.edit))
            .filter(|e| e.id == id)
            .map(|e| (e.at, e.facing))
    }
    pub(crate) fn key(&self) -> Option<(String, Vec3, f32)> {
        self.edit
            .as_ref()
            .or_else(|| self.pending.as_ref().map(|p| &p.edit))
            .map(|e| (e.id.clone(), e.at, e.facing))
    }
    pub(crate) fn sync(&mut self, store: &crate::live::LiveStore, live: &AreaLive, boot: &str) {
        if self.boot != boot {
            self.edit = None;
            self.pending = None;
            self.draft = None;
            self.boot = boot.into();
        }
        if self.edit.as_ref().is_some_and(|e| live.find(&e.id).is_none()) {
            self.edit = None;
        }
        if self.pending.as_ref().is_some_and(|p| {
            now_ms() > p.until
                || store.result(&p.command).is_some_and(|r| !r.ok)
                || live.find(&p.edit.id).is_none_or(|(_, o)| {
                    o.position().distance(p.edit.at) < 0.02
                        && ((o.facing - p.edit.facing + 180.).rem_euclid(360.) - 180.).abs() < 0.1
                })
        }) {
            self.pending = None;
        }
    }
}

fn selected<'a>(view: &AreaView, live: &'a AreaLive) -> Option<(ObjectKind, &'a LiveObject)> {
    if view.live.selection_ids().count() != 1 {
        return None;
    }
    live.find(view.live.selected.as_deref()?)
        .filter(|(kind, _)| matches!(kind, ObjectKind::Placeable | ObjectKind::Door))
}

fn ring(view: &AreaView, at: Vec3, facing: f32) -> Option<TurnRing> {
    let camera = view.camera()?;
    view.turn_ring.then_some(TurnRing {
        pivot: at + Vec3::Z * 0.05,
        radius: ((camera.eye - at).length() * 0.0765).max(0.5),
        facing: facing.to_radians(),
    })
}

/// A visible rotation handle takes precedence over the empty-ground selection box.
pub(crate) fn on_handle(view: &AreaView, live: &AreaLive, pos: egui::Pos2) -> bool {
    let Some((ObjectKind::Placeable, object)) = selected(view, live) else { return false };
    let (at, facing) =
        view.live.properties.pose(&object.id).unwrap_or((object.position(), object.facing));
    ring(view, at, facing).is_some_and(|r| view.on_ring(&r, pos))
}

pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
    live: Option<&AreaLive>,
) -> bool {
    let Some((kind, object)) = live.and_then(|l| selected(view, l)) else {
        view.live.properties.edit = None;
        return false;
    };
    let (at, facing) =
        view.live.properties.pose(&object.id).unwrap_or((object.position(), object.facing));
    let mut edit = view.live.properties.edit.take();
    let keys = response.hovered() && !ui.ctx().egui_wants_keyboard_input();
    if keys && kind == ObjectKind::Placeable {
        for (cmd, step) in area_view::turn_keys(view) {
            if ui.input(|i| app.keymap.pressed(i, cmd)) {
                let e = edit.get_or_insert_with(|| Edit {
                    id: object.id.clone(),
                    at,
                    facing,
                    original_at: at,
                    original_facing: facing,
                    gesture: Gesture::Keys,
                });
                if matches!(e.gesture, Gesture::Keys) {
                    e.facing = (e.facing + step.to_degrees()).rem_euclid(360.);
                }
            }
        }
    }
    let modifiers = ui.input(|i| i.modifiers);
    if edit.is_none()
        && !modifiers.shift
        && !modifiers.ctrl
        && !modifiers.command
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        let gesture = if kind == ObjectKind::Placeable
            && modifiers.alt
            && response.drag_started_by(egui::PointerButton::Secondary)
        {
            Some(Gesture::Turn(origin))
        } else if response.drag_started_by(egui::PointerButton::Primary) {
            if kind == ObjectKind::Placeable
                && let Some(ring) = ring(view, at, facing).filter(|r| view.on_ring(r, origin))
            {
                view.angle_about(ring.pivot, origin).map(Gesture::Ring)
            } else if live.is_some_and(|l| picked(view, l, origin).as_deref() == Some(&object.id)) {
                view.ground_at(origin, at.z).map(|p| Gesture::Move(p - at))
            } else {
                None
            }
        } else {
            None
        };
        edit = gesture.map(|gesture| Edit {
            id: object.id.clone(),
            at,
            facing,
            original_at: at,
            original_facing: facing,
            gesture,
        });
    }
    let Some(mut edit) = edit else {
        return false;
    };
    if edit.id != object.id
        || response.secondary_clicked()
        || ui.input(|i| i.key_pressed(egui::Key::Escape))
    {
        return true;
    }
    let pointer = ui.input(|i| i.pointer.latest_pos());
    match edit.gesture {
        Gesture::Move(grip) => {
            if let Some(at) = pointer.and_then(|p| view.ground_at(p, edit.original_at.z)) {
                edit.at = view.snapped(at - grip);
            }
        }
        Gesture::Ring(grip) => {
            if let Some(angle) =
                pointer.and_then(|p| view.angle_about(edit.original_at + Vec3::Z * 0.05, p))
            {
                edit.facing = mg_area::arrange::snap_rotation(
                    edit.original_facing.to_radians() + angle - grip,
                    view.snap.1,
                )
                .to_degrees()
                .rem_euclid(360.);
            }
        }
        Gesture::Turn(origin) => {
            if let Some(p) = pointer {
                edit.facing = mg_area::arrange::snap_rotation(
                    edit.original_facing.to_radians() + (p.x - origin.x) * 0.01,
                    view.snap.1,
                )
                .to_degrees()
                .rem_euclid(360.);
            }
        }
        Gesture::Keys => {}
    }
    let released = match edit.gesture {
        Gesture::Keys => area_view::turn_keys(view)
            .iter()
            .all(|(cmd, _)| !ui.input(|i| app.keymap.held_but(i, *cmd, false, false))),
        Gesture::Turn(_) => ui.input(|i| i.pointer.button_released(egui::PointerButton::Secondary)),
        _ => ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)),
    };
    if released {
        let moving = matches!(edit.gesture, Gesture::Move(_));
        let changed =
            if moving { edit.at != edit.original_at } else { edit.facing != edit.original_facing };
        if changed {
            let op = if moving {
                Op::SetPosition {
                    target: edit.id.clone(),
                    x: edit.at.x,
                    y: edit.at.y,
                    z: edit.at.z,
                }
            } else {
                Op::SetFacing { target: edit.id.clone(), f: edit.facing }
            };
            if let Some(command) = queue(app, view, op) {
                view.live.properties.pending =
                    Some(Pending { edit, command, until: now_ms() + 30_000 });
            }
        }
    } else {
        view.live.properties.edit = Some(edit);
    }
    true
}

pub(crate) fn paint(ui: &egui::Ui, view: &AreaView, live: &AreaLive) {
    let Some((ObjectKind::Placeable, o)) = selected(view, live) else {
        return;
    };
    let (at, facing) = view.live.properties.pose(&o.id).unwrap_or((o.position(), o.facing));
    let Some(ring) = ring(view, at, facing) else {
        return;
    };
    let painter = ui.painter_at(view.rect);
    let active = view.live.properties.edit.is_some()
        || ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| view.on_ring(&ring, p));
    let color =
        if active { Color32::from_rgb(255, 225, 150) } else { Color32::from_rgb(255, 170, 60) };
    for k in 0..area_view::RING_SEGMENTS {
        let (a, b) = ring.segment(k);
        if let (Some(a), Some(b)) = (view.screen_pos(a), view.screen_pos(b)) {
            painter.line_segment([a, b], Stroke::new(if active { 2.5 } else { 1.5 }, color));
        }
    }
    if let (Some(a), Some(b)) =
        (view.screen_pos(ring.pivot), view.screen_pos(ring.point(ring.facing)))
    {
        painter.line_segment([a, b], Stroke::new(1., color));
        painter.circle_filled(b, 3.5, color);
        if active {
            painter.text(
                b + egui::vec2(10., -10.),
                egui::Align2::LEFT_BOTTOM,
                format!("{facing:.0}°"),
                egui::FontId::proportional(13.),
                color,
            );
        }
    }
}

pub(crate) fn window(
    app: &mut Moonglow,
    view: &mut AreaView,
    ctx: &egui::Context,
    live: Option<&AreaLive>,
) {
    if !view.live.properties.open {
        return;
    }
    let Some((kind, object)) = live.and_then(|l| selected(view, l)) else {
        return;
    };
    let mut draft =
        view.live.properties.draft.take().filter(|d| d.id == object.id).unwrap_or_else(|| Draft {
            id: object.id.clone(),
            name: object.name.clone(),
            useable: object.useable.unwrap_or(true),
            transform: object.vt.clone().unwrap_or_default(),
        });
    let mut open = true;
    let mut op = None;
    egui::Window::new("Live Properties")
        .id(egui::Id::new(("live-properties", view.area)))
        .open(&mut open)
        .default_width(330.)
        .default_pos(egui::pos2(280., 230.))
        .show(ctx, |ui| {
            ui.strong(format!("{} · {}", crate::live_panel::name(object), object.id));
            ui.weak(format!("Position {:.2}, {:.2}, {:.2} · facing {:.1}°",
                object.x, object.y, object.z, object.facing));
            if kind == ObjectKind::Placeable {
                ui.weak("Drag marker to move; Q/E, ring or Alt+right drag to turn. Release sends one command.");
                if object.is_static == Some(true) {
                    ui.colored_label(Color32::YELLOW, "Static: VFX and visual transforms are unavailable.");
                    if ui.button("Make Dynamic").clicked() {
                        op = Some(Op::SetStatic { target: draft.id.clone(), r#static: false });
                    }
                    ui.weak("Game clients must re-enter the area after changing Static.");
                }
            }
            ui.horizontal(|ui| {
                let label = ui.label("Name");
                ui.text_edit_singleline(&mut draft.name).labelled_by(label.id);
                if ui.button("Set Name").clicked() {
                    op = Some(Op::SetName { target: draft.id.clone(), name: draft.name.clone() });
                }
            });
            if ui.checkbox(&mut draft.useable, "Useable").changed() {
                op = Some(Op::SetUseable { target: draft.id.clone(), useable: draft.useable });
            }
            ui.separator();
            ui.strong("Visual Transform");
            ui.add_enabled_ui(object.is_static != Some(true), |ui| {
            egui::Grid::new("live-transform-fields").show(ui, |ui| {
                for (name, value, min, max, speed) in [
                    ("Scale", &mut draft.transform.scale, 0.001, 10., 0.01),
                    ("Rotate X", &mut draft.transform.rotate.x, -360., 360., 1.),
                    ("Rotate Y", &mut draft.transform.rotate.y, -360., 360., 1.),
                    ("Rotate Z", &mut draft.transform.rotate.z, -360., 360., 1.),
                    ("Translate X", &mut draft.transform.translate.x, -1000., 1000., 0.05),
                    ("Translate Y", &mut draft.transform.translate.y, -1000., 1000., 0.05),
                    ("Translate Z", &mut draft.transform.translate.z, -1000., 1000., 0.05),
                ] {
                    let label = ui.label(name);
                    let units = if name.starts_with("Rotate") { " deg" } else if name.starts_with("Translate") { " m" } else { "x" };
                    ui.add(egui::DragValue::new(value).range(min..=max).speed(speed).suffix(units)).labelled_by(label.id)
                        .on_disabled_hover_text("Make this static placeable Dynamic before applying a visual transform.");
                    ui.end_row();
                }
            });
            ui.weak("Rotation in degrees; translation in metres. Changes are sent on Apply.");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Apply Transform").on_disabled_hover_text("Static placeables cannot use visual transforms.").on_hover_text("Send these visual-transform values to the running server.").clicked() {
                    op = Some(Op::SetVisualTransform {
                        target: draft.id.clone(), scale: Some(draft.transform.scale),
                        rotate: Some(draft.transform.rotate), translate: Some(draft.transform.translate), reset: false,
                    });
                }
                if ui.button("Reset Transform").on_disabled_hover_text("Static placeables cannot use visual transforms.").on_hover_text("Remove the visual transform on the running server.").clicked() {
                    draft.transform = VisualTransform::default();
                    op = Some(Op::SetVisualTransform { target: draft.id.clone(), scale: None, rotate: None, translate: None, reset: true });
                }
            });
            });
                if ui.button("Reload Report").on_hover_text("Discard these unsent edits and reload the latest reported values. Sends no command.").clicked() {
                    draft.transform = object.vt.clone().unwrap_or_default();
                    draft.name = object.name.clone();
                    draft.useable = object.useable.unwrap_or(true);
                }
        });
    if let Some(op) = op {
        queue(app, view, op);
    }
    view.live.properties.open = open;
    view.live.properties.draft = Some(draft);
}
