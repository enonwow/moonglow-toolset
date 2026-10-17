//! Drag live objects as a group; only release submits orders to the game.
use crate::{
    Moonglow,
    area_view::AreaView,
    live::{AreaLive, LiveObject, Op},
};
use egui::{Color32, Stroke};
use glam::Vec3;
use mg_area::ObjectKind;

#[derive(Debug)]
struct Target {
    object: LiveObject,
    kind: ObjectKind,
}
#[derive(Debug)]
pub(crate) struct Drag {
    targets: Vec<Target>,
    boot: String,
    grip: Vec3,
    anchor: Vec3,
    delta: Vec3,
}
fn eligible(kind: ObjectKind, o: &LiveObject, players: bool) -> bool {
    matches!(
        kind,
        ObjectKind::Creature | ObjectKind::Placeable | ObjectKind::Door | ObjectKind::Item
    ) && !o.dead
        && crate::live_targets::allowed(o, players)
}
pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
    live: Option<&AreaLive>,
) -> bool {
    let Some(live) = live else {
        view.live.drag = None;
        return false;
    };
    let Some(store) = app.live.clone() else { return false };
    let modifiers = ui.input(|i| i.modifiers);
    let players = store.policy().players;
    let mut draft = view.live.drag.take();
    if draft.is_none()
        && !modifiers.shift
        && !modifiers.ctrl
        && !modifiers.command
        && !modifiers.alt
        && response.drag_started_by(egui::PointerButton::Primary)
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        && let Some(id) = crate::live_view::picked(view, live, origin)
        && let Some((kind, object)) = live.find(&id)
    {
        if !eligible(kind, object, players) {
            view.notice = Some(
                "This object cannot be moved: player permission, DM protection or death state."
                    .into(),
            );
            return true;
        }
        if !view.live.is_selected(&id) {
            view.live.select(Some(id.clone()), false);
        }
        let targets: Vec<_> = view
            .live
            .selection_ids()
            .filter_map(|id| live.find(id))
            .filter(|(k, o)| eligible(*k, o, players))
            .map(|(kind, o)| Target { object: o.clone(), kind })
            .collect();
        let anchor = object.position();
        if let Some(grip) = view.ground_at(origin, anchor.z) {
            draft = Some(Drag { targets, boot: store.boot(), grip, anchor, delta: Vec3::ZERO });
        }
    }
    let Some(mut draft) = draft else { return false };
    if modifiers.ctrl
        || modifiers.command
        || ui.input(|i| i.key_pressed(egui::Key::Escape) || i.pointer.secondary_clicked())
        || draft.boot != store.boot()
    {
        return true;
    }
    if let Some(point) =
        ui.input(|i| i.pointer.latest_pos()).and_then(|p| view.ground_at(p, draft.anchor.z))
    {
        draft.delta = view.snapped(draft.anchor + point - draft.grip) - draft.anchor;
    }
    let valid = draft.targets.iter().all(|t| {
        let at = t.object.position() + draft.delta;
        live.find(&t.object.id).is_some_and(|(k, o)| {
            k == t.kind
                && o.tag == t.object.tag
                && o.resref == t.object.resref
                && eligible(k, o, players)
        }) && live
            .size
            .is_none_or(|[w, h]| at.x >= 0. && at.y >= 0. && at.x < w as f32 && at.y < h as f32)
    });
    ui.ctx().set_cursor_icon(if valid {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::NotAllowed
    });
    if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
        if !valid {
            view.notice = Some(
                "Move cancelled: keep every object inside the area, with current permissions."
                    .into(),
            );
            return true;
        }
        if draft.delta.length_squared() > 0.0001 {
            for target in draft.targets {
                let at = target.object.position() + draft.delta;
                let op = match target.kind {
                    ObjectKind::Placeable | ObjectKind::Door => Op::SetPosition {
                        target: target.object.id.clone(),
                        x: at.x,
                        y: at.y,
                        z: at.z,
                    },
                    _ => Op::Teleport {
                        target: target.object.id.clone(),
                        x: at.x,
                        y: at.y,
                        z: at.z,
                        f: Some(target.object.facing),
                    },
                };
                if let Some(command) = crate::live_view::queue(app, view, op) {
                    let area = view.area.to_string();
                    crate::live_teleport::track(
                        view,
                        command,
                        target.object.id,
                        area,
                        draft.boot.clone(),
                    );
                }
            }
        }
    } else {
        view.live.drag = Some(draft);
    }
    true
}
pub(crate) fn paint(ui: &egui::Ui, view: &AreaView) {
    let Some(draft) = &view.live.drag else { return };
    let painter = ui.painter_at(view.rect);
    for (index, target) in draft.targets.iter().enumerate() {
        let at = target.object.position() + draft.delta;
        if let Some(point) = view.screen_pos(at + Vec3::Z * 0.3) {
            painter.circle_filled(point, 9., Color32::from_rgba_unmultiplied(100, 210, 255, 80));
            painter.circle_stroke(point, 11., Stroke::new(2., Color32::LIGHT_BLUE));
            if let Some(origin) = view.screen_pos(target.object.position() + Vec3::Z * 0.3) {
                painter.line_segment([origin, point], Stroke::new(1., Color32::LIGHT_BLUE));
            }
            painter.text(
                point,
                egui::Align2::CENTER_CENTER,
                (index + 1).to_string(),
                egui::FontId::proportional(13.),
                Color32::WHITE,
            );
        }
    }
    painter.text(
        view.rect.left_bottom() + egui::vec2(12., -36.),
        egui::Align2::LEFT_BOTTOM,
        format!("Move {} objects | release to send | Esc/right click cancels", draft.targets.len()),
        egui::FontId::proportional(14.),
        Color32::LIGHT_BLUE,
    );
}
