//! Blueprint kind mapping and rectangle aiming for running-world triggers.
use crate::{Moonglow, area_view::AreaView, live_view::queue};
use mg_core::ResType;
use mg_live::store::{Op, SpawnKind, TriggerShape};
use mg_resman::ResKey;
pub(crate) fn kind(ty: ResType) -> Option<SpawnKind> {
    Some(match ty {
        ResType::UTC => SpawnKind::Creature,
        ResType::UTP => SpawnKind::Placeable,
        ResType::UTI => SpawnKind::Item,
        ResType::UTW => SpawnKind::Waypoint,
        ResType::UTS => SpawnKind::Sound,
        ResType::UTE => SpawnKind::Encounter,
        ResType::UTM => SpawnKind::Store,
        ResType::UTT => SpawnKind::Trigger,
        _ => return None,
    })
}
pub(crate) fn trigger(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
    key: ResKey,
) {
    if matches!(crate::live_aim::gesture(ui, response), crate::live_aim::Gesture::Cancel) {
        crate::live_aim::clear(view);
        return;
    }
    if response.drag_started_by(egui::PointerButton::Primary) {
        view.live.trigger_from = ui
            .input(|i| i.pointer.press_origin())
            .filter(|p| view.rect.contains(*p))
            .and_then(|p| view.ground_at(p, 0.))
            .map(|p| view.snapped(p));
    }
    let at = crate::live_aim::cursor(ui, view).map(|p| view.snapped(p));
    if let Some(at) = at {
        if let Some(from) = view.live.trigger_from {
            let lo = from.min(at);
            let hi = from.max(at);
            let size = hi - lo;
            let valid = size.x > 0.1 && size.y > 0.1 && size.x <= 30. && size.y <= 30.;
            let points = [lo, glam::vec3(hi.x, lo.y, 0.), hi, glam::vec3(lo.x, hi.y, 0.), lo]
                .into_iter()
                .filter_map(|p| view.screen_pos(p + glam::Vec3::Z * 0.1))
                .collect();
            let color = if valid { egui::Color32::LIGHT_GREEN } else { egui::Color32::LIGHT_RED };
            ui.painter_at(view.rect).add(egui::Shape::line(points, egui::Stroke::new(2., color)));
            ui.painter_at(view.rect).text(
                view.rect.center_top() + egui::vec2(0., 32.),
                egui::Align2::CENTER_TOP,
                format!(
                    "{}: {:.1} x {:.1} m | release to spawn | maximum 30 m | Esc cancels",
                    key.resref, size.x, size.y
                ),
                egui::FontId::proportional(14.),
                color,
            );
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                view.live.trigger_from = None;
                if valid {
                    let mid = (lo + hi) * 0.5;
                    let op = Op::SpawnTrigger {
                        r: Some(key.resref.to_string()),
                        x: mid.x,
                        y: mid.y,
                        z: mid.z,
                        f: 0.,
                        shape: TriggerShape::Rectangle,
                        radius: None,
                        width: Some(size.x),
                        height: Some(size.y),
                    };
                    if queue(app, view, op).is_some() && !ui.input(|i| i.modifiers.shift) {
                        view.live.aim = None;
                    }
                } else {
                    view.notice =
                        Some("Drag a rectangle between 0.1 and 30 metres on each side.".into());
                }
            }
        } else {
            crate::live_aim::circle(ui, view, at, 0.5, egui::Color32::LIGHT_GREEN);
        }
    } else if response.drag_stopped_by(egui::PointerButton::Primary) {
        view.live.trigger_from = None;
    }
}
