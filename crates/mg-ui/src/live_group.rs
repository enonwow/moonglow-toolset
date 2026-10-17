//! Live selection and group orders. The editable area's selection is separate.
use std::collections::BTreeSet;

use egui::{Color32, Pos2, Rect, Stroke};
use mg_area::ObjectKind;

use crate::Moonglow;
use crate::area_view::AreaView;
use crate::live::{AreaLive, Op};
use crate::live_targets::creature;
use crate::live_view::{Destination, now_ms, picked, queue, screen, visible};

#[derive(Debug, Clone)]
enum Aim {
    Move { ids: Vec<String>, formation: bool, run: bool },
    Attack(Vec<String>),
}

#[derive(Debug, Default)]
pub(crate) struct Group {
    anchor: Option<String>,
    others: BTreeSet<String>,
    boot: String,
    box_from: Option<Pos2>,
    aim: Option<Aim>,
    saved: [BTreeSet<String>; 5],
    last_recall: Option<(usize, f64)>,
}

impl Group {
    pub(crate) fn focus(&mut self, primary: &mut Option<String>, id: &str) {
        let mut ids: BTreeSet<_> = self.ids(primary).map(str::to_owned).collect();
        ids.remove(id);
        *primary = Some(id.into());
        self.anchor.clone_from(primary);
        self.others = ids;
    }

    pub(crate) fn cancel_aim(&mut self) {
        self.aim = None;
    }
    pub(crate) fn ids<'a>(&'a self, primary: &'a Option<String>) -> impl Iterator<Item = &'a str> {
        self.others
            .iter()
            .filter(move |_| self.anchor == *primary)
            .map(String::as_str)
            .chain(primary.as_deref())
    }

    pub(crate) fn contains(&self, primary: &Option<String>, id: &str) -> bool {
        primary.as_deref() == Some(id) || (self.anchor == *primary && self.others.contains(id))
    }

    pub(crate) fn replace(&mut self, primary: &mut Option<String>, mut ids: BTreeSet<String>) {
        *primary = ids.pop_first();
        self.anchor.clone_from(primary);
        self.others = ids;
    }

    pub(crate) fn select(
        &mut self,
        primary: &mut Option<String>,
        id: Option<String>,
        toggle: bool,
    ) {
        let mut ids =
            if toggle { self.ids(primary).map(str::to_owned).collect() } else { BTreeSet::new() };
        if let Some(id) = id
            && !ids.remove(&id)
        {
            ids.insert(id);
        }
        self.replace(primary, ids);
    }

    pub(crate) fn sync(&mut self, primary: &mut Option<String>, live: &AreaLive, boot: &str) {
        let ids =
            self.ids(primary).filter(|id| live.find(id).is_some()).map(str::to_owned).collect();
        if !self.boot.is_empty() && self.boot != boot {
            self.replace(primary, BTreeSet::new());
            self.aim = None;
            self.box_from = None;
            self.saved = Default::default();
            self.last_recall = None;
        } else {
            self.replace(primary, ids);
        }
        self.boot = boot.into();
    }
}

/// Stable two-metre grid, centred on the clicked destination.
fn formation_offset(index: usize, count: usize) -> glam::Vec3 {
    let columns = (count as f32).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    glam::vec3(
        (index % columns) as f32 - (columns - 1) as f32 / 2.,
        (index / columns) as f32 - (rows - 1) as f32 / 2.,
        0.,
    ) * 2.
}

pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
    live: Option<&AreaLive>,
) -> bool {
    let players = app.live.as_ref().is_some_and(|s| s.policy().players);
    if response.hovered()
        && !ui.ctx().egui_wants_keyboard_input()
        && let Some(live) = live
    {
        if crate::live_scene::hotkey(app, ui) {
            view.notice.clone_from(&app.live_scene.notice);
            return true;
        }
        for (index, (save, recall)) in control_keys().into_iter().enumerate() {
            if ui.input(|i| app.keymap.pressed(i, save)) {
                view.live.group.saved[index] =
                    view.live.selection_ids().map(str::to_owned).collect();
            }
            if ui.input(|i| app.keymap.pressed(i, recall)) {
                let now = ui.input(|i| i.time);
                let double =
                    view.live.group.last_recall.is_some_and(|(n, t)| n == index && now - t < 0.4);
                let ids = view.live.group.saved[index]
                    .iter()
                    .filter(|id| live.find(id).is_some())
                    .cloned()
                    .collect();
                view.live.group.replace(&mut view.live.selected, ids);
                view.live.group.last_recall = Some((index, now));
                if double {
                    let points: Vec<_> = view
                        .live
                        .selection_ids()
                        .filter_map(|id| live.find(id).map(|(_, o)| o.position()))
                        .collect();
                    if !points.is_empty()
                        && let Some(orbit) = &mut view.orbit
                    {
                        orbit.target =
                            points.iter().copied().sum::<glam::Vec3>() / points.len() as f32;
                    }
                }
                app.live_panel.follow = None;
            }
        }
    }
    if let Some(aim) = view.live.group.aim.clone() {
        let gesture = crate::live_aim::gesture(ui, response);
        if matches!(gesture, crate::live_aim::Gesture::Cancel) {
            view.live.group.aim = None;
        } else if let crate::live_aim::Gesture::Click(pos) = gesture
            && let Some(live) = live
        {
            match aim {
                Aim::Move { ids, formation, run } => {
                    if let Some(at) = view.ground_at(pos, 0.) {
                        let ids: Vec<_> = ids
                            .into_iter()
                            .filter(|id| {
                                live.find(id).is_some_and(|(k, o)| creature(k, o, players))
                            })
                            .collect();
                        for (n, target) in ids.iter().enumerate() {
                            let at = at
                                + if formation {
                                    formation_offset(n, ids.len())
                                } else {
                                    glam::Vec3::ZERO
                                };
                            if let Some(command) = queue(
                                app,
                                view,
                                Op::NpcMove {
                                    target: target.clone(),
                                    x: at.x,
                                    y: at.y,
                                    z: at.z,
                                    run: run || ui.input(|i| i.modifiers.shift),
                                    hold: true,
                                    timeout: 30,
                                },
                            ) {
                                view.live.destinations.retain(|d| d.target != *target);
                                view.live.destinations.push(Destination {
                                    target: target.clone(),
                                    command,
                                    at,
                                    started_ms: now_ms(),
                                });
                            }
                        }
                        view.live.group.aim = None;
                    }
                }
                Aim::Attack(ids) => {
                    if let Some(victim) = picked(view, live, pos).filter(|id| !ids.contains(id))
                        && live
                            .find(&victim)
                            .is_some_and(|(k, o)| k == ObjectKind::Creature && !o.dead)
                    {
                        for target in ids.into_iter().filter(|id| {
                            live.find(id).is_some_and(|(k, o)| creature(k, o, players))
                        }) {
                            queue(
                                app,
                                view,
                                Op::NpcAttack {
                                    target,
                                    victim: victim.clone(),
                                    faction: None,
                                    hold: true,
                                    timeout: 30,
                                },
                            );
                        }
                        view.live.group.aim = None;
                    }
                }
            }
        }
        return true;
    }
    // Ctrl+drag belongs to the camera, including a Ctrl press during a box.
    // A Ctrl+click is still handled by the ordinary marker toggle below us.
    if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
        view.live.group.box_from = None;
        return false;
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        let from = ui.input(|i| i.pointer.press_origin());
        if ui.input(|i| i.modifiers.shift)
            || from.is_some_and(|p| {
                live.is_none_or(|a| {
                    picked(view, a, p).is_none() && !crate::live_props::on_handle(view, a, p)
                })
            })
        {
            view.live.group.box_from = from;
        }
    }
    if let Some(from) = view.live.group.box_from {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            view.live.group.box_from = None;
        } else if response.drag_stopped_by(egui::PointerButton::Primary) {
            if let (Some(to), Some(live)) = (ui.input(|i| i.pointer.latest_pos()), live) {
                let rect = Rect::from_two_pos(from, to).intersect(view.rect);
                let mut ids: BTreeSet<_> = if ui.input(|i| i.modifiers.shift) {
                    view.live.selection_ids().map(str::to_owned).collect()
                } else {
                    BTreeSet::new()
                };
                for id in live
                    .all()
                    .filter(|(k, o)| visible(view, live, *k, o))
                    .filter(|(_, o)| screen(view, o).is_some_and(|p| rect.contains(p)))
                    .map(|(_, o)| o.id.clone())
                {
                    ids.insert(id);
                }
                view.live.group.replace(&mut view.live.selected, ids);
                app.live_panel.follow = None;
            }
            view.live.group.box_from = None;
        }
        return true;
    }
    false
}

pub(crate) fn control_keys() -> [(crate::keys::Cmd, crate::keys::Cmd); 5] {
    use crate::keys::Cmd::*;
    [
        (LiveStoreGroup1, LiveRecallGroup1),
        (LiveStoreGroup2, LiveRecallGroup2),
        (LiveStoreGroup3, LiveRecallGroup3),
        (LiveStoreGroup4, LiveRecallGroup4),
        (LiveStoreGroup5, LiveRecallGroup5),
    ]
}

pub(crate) fn control_held(input: &egui::InputState, keys: &crate::keys::Keymap) -> bool {
    control_keys()
        .into_iter()
        .flat_map(|(a, b)| [a, b])
        .any(|cmd| keys.held_but(input, cmd, false, false))
}

pub(crate) fn paint(ui: &egui::Ui, view: &AreaView) {
    let painter = ui.painter_at(view.rect);
    if let (Some(from), Some(to)) = (view.live.group.box_from, ui.input(|i| i.pointer.latest_pos()))
    {
        let rect = Rect::from_two_pos(from, to).intersect(view.rect);
        painter.rect_filled(rect, 0., Color32::from_rgba_unmultiplied(80, 180, 255, 25));
        painter.rect_stroke(
            rect,
            0.,
            Stroke::new(1., Color32::LIGHT_BLUE),
            egui::StrokeKind::Inside,
        );
    }
    if let Some(aim) = &view.live.group.aim {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if let Some(at) = crate::live_aim::cursor(ui, view) {
            match aim {
                Aim::Move { ids, formation, .. } => {
                    for n in 0..ids.len() {
                        let p = at
                            + if *formation {
                                formation_offset(n, ids.len())
                            } else {
                                glam::Vec3::ZERO
                            };
                        crate::live_aim::circle(ui, view, p, 0.5, Color32::LIGHT_GREEN);
                    }
                }
                Aim::Attack(_) => crate::live_aim::circle(ui, view, at, 0.6, Color32::LIGHT_RED),
            }
        }
        let text = match aim {
            Aim::Move { formation: true, run: true, .. } => {
                "Group run in formation: click destination"
            }
            Aim::Move { run: true, .. } => "Group run to one point: click destination",
            Aim::Move { formation: true, .. } => {
                "Group walk in formation: click destination (Shift: run)"
            }
            Aim::Move { .. } => "Group walk to one point: click destination (Shift: run)",
            Aim::Attack(_) => "Group attack: click a creature outside the selection",
        };
        painter.text(
            view.rect.center_top() + egui::vec2(0., 12.),
            egui::Align2::CENTER_TOP,
            format!("{text} · Esc / right click: cancel"),
            egui::FontId::proportional(14.),
            Color32::WHITE,
        );
    }
}

pub(crate) fn move_to(view: &mut AreaView, ids: Vec<String>, formation: bool, run: bool) {
    crate::live_aim::clear(view);
    view.notice = None;
    if ids.len() == 1 {
        view.live.aim = Some(if run {
            crate::live_view::Aim::Run(ids[0].clone())
        } else {
            crate::live_view::Aim::Move(ids[0].clone())
        });
    } else {
        view.live.group.aim = Some(Aim::Move { ids, formation, run });
    }
}
pub(crate) fn attack(view: &mut AreaView, ids: Vec<String>) {
    crate::live_aim::clear(view);
    view.notice = None;
    if ids.len() == 1 {
        view.live.aim = Some(crate::live_view::Aim::Attack(ids[0].clone()));
    } else {
        view.live.group.aim = Some(Aim::Attack(ids));
    }
}
