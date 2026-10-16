//! The area view's placeable brush (Paint, on its toolbar): placeables
//! chosen in the palette (Ctrl + click for several) are painted onto the
//! ground, as builders asked for trees, rocks and bushes.
//!
//! - Scatter: a circle on the ground (Shift + wheel sizes it). A press lays
//!   a dab, and so does a drag every so far, or the button held still every
//!   so often: each dab adds a share (Flow) of what the circle lacks of its
//!   Density, at random places on the walkmesh no nearer than Spacing to
//!   another placeable, a random pick of the chosen blueprints, each turned
//!   as Q and E turned the brush give or take Turn, scaled at random
//!   between Scale's two. The placeables of the chosen blueprints already
//!   in the circle count, so going over a painted place again fills it up
//!   and no more. Before a press the next dab shows, see-through.
//! - Row: a drag lays pieces end to end from where it began to the pointer,
//!   each facing along the row (Q and E turn them on it), one after the
//!   other by its model's length along the row, and Gap.
//!
//! What a stroke puts down shows see-through until the button is let go,
//! and goes into the area then as one command: one Undo takes it away.

use std::collections::HashMap;

use egui::{Color32, Stroke};
use glam::{Vec2, Vec3};
use mg_area::{AreaObject, ObjectKind};
use mg_core::ResType;
use mg_edit::Command;
use mg_resman::ResKey;

use crate::area_view::AreaView;
use crate::settings::PaintSettings;
use crate::{Action, Moonglow};

/// A placeable the brush puts down (or would): of which blueprint, where,
/// turned how far from as its blueprint stands (radians, anticlockwise),
/// and its scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Painted {
    pub key: ResKey,
    pub position: Vec3,
    pub turn: f32,
    pub scale: f32,
}

/// An area view's brush: the stroke going on and what it shows.
#[derive(Debug, Default)]
pub struct PaintState {
    /// What the stroke going on has put down: into the area when the
    /// button is let go.
    pub(crate) pending: Vec<Painted>,
    /// The button is down on the view, painting.
    pub(crate) painting: bool,
    /// Where and when (egui's time) the stroke's last dab was laid.
    last: Option<(Vec3, f64)>,
    /// Chooses the next dab, the one shown before it is laid.
    seed: u64,
    /// Each blueprint as an unplaced object (`None`: it can't be read).
    templates: HashMap<ResKey, Option<AreaObject>>,
    /// What the last frame showed see-through: the stroke's placeables
    /// and the next dab.
    pub shown: usize,
}

/// The circle's color, a dab's green.
const CIRCLE: Color32 = Color32::from_rgb(110, 230, 110);

/// How far the pointer goes on the ground, as a share of the radius,
/// between a drag's dabs.
const DAB_STEP: f32 = 0.4;

/// How often a button held still lays a dab, in seconds.
const DAB_EVERY: f64 = 0.2;

/// How many places a dab tries for each placeable it lays, before it
/// gives up on the rest (the circle is as full as Spacing lets it be).
const TRIES: usize = 30;

/// The radius's bounds, in centimeters.
const RADIUS: (u16, u16) = (25, 5000);

/// The placeables chosen in the palette to paint with: the one clicked and
/// those chosen with Ctrl + click.
pub(crate) fn keys(app: &Moonglow) -> Vec<ResKey> {
    let p = &app.palette;
    if p.tiles || p.prefabs {
        return Vec::new();
    }
    let mut keys: Vec<ResKey> = p.selected.into_iter().chain(p.chosen.iter().copied()).collect();
    keys.retain(|k| k.restype == ResType::UTP);
    keys.dedup();
    keys
}

/// The blueprints the brush paints with, when Paint is on and placeables
/// are chosen.
pub(crate) fn active(app: &Moonglow) -> Option<Vec<ResKey>> {
    let keys = keys(app);
    (app.paint && !keys.is_empty()).then_some(keys)
}

/// What one dab at `center` lays, by `seed`: Flow's share of what the
/// circle lacks of its Density (counting `around`'s placeables of the
/// brush's blueprints, those marked so), each no nearer than Spacing to
/// any of `around` or to another of the dab's. `ground` is the walkmesh's
/// height at a point (`None` off it: nothing is laid there); `turn` the
/// brush's facing, `snap` the angle facings snap to (degrees).
#[allow(clippy::too_many_arguments)]
pub(crate) fn dab(
    s: &PaintSettings,
    center: Vec3,
    keys: &[ResKey],
    around: &[(Vec2, bool)],
    turn: f32,
    snap: Option<f32>,
    seed: u64,
    ground: &dyn Fn(Vec2) -> Option<f32>,
) -> Vec<Painted> {
    if keys.is_empty() {
        return Vec::new();
    }
    let radius = f32::from(s.radius_cm) / 100.0;
    let spacing = f32::from(s.spacing_cm) / 100.0;
    let middle = center.truncate();
    let inside = |p: Vec2| p.distance(middle) <= radius;
    let wanted = f32::from(s.density) * std::f32::consts::PI * radius * radius / 100.0;
    let have = around.iter().filter(|(p, counted)| *counted && inside(*p)).count();
    let lacking = (wanted.round() as usize).saturating_sub(have);
    if lacking == 0 {
        return Vec::new();
    }
    let flow = f32::from(s.flow.clamp(1, 100)) / 100.0;
    let count = ((lacking as f32 * flow).ceil() as usize).clamp(1, lacking);
    let mut rng = fastrand::Rng::with_seed(seed);
    let mut taken: Vec<Vec2> =
        around.iter().map(|(p, _)| *p).filter(|p| p.distance(middle) <= radius + spacing).collect();
    let spread = f32::from(s.turn_spread.min(360)).to_radians();
    let (low, high) = (s.scale_min.min(s.scale_max), s.scale_max.max(s.scale_min));
    let lift = f32::from(s.lift_cm) / 100.0;
    let mut out = Vec::new();
    for _ in 0..count * TRIES {
        if out.len() == count {
            break;
        }
        // Evenly over the circle.
        let (r, a) = (radius * rng.f32().sqrt(), rng.f32() * std::f32::consts::TAU);
        let p = middle + Vec2::new(a.cos(), a.sin()) * r;
        if taken.iter().any(|q| q.distance(p) < spacing) {
            continue;
        }
        let Some(z) = ground(p) else { continue };
        let key = keys[rng.usize(..keys.len())];
        let turn = mg_area::arrange::snap_rotation(turn + (rng.f32() - 0.5) * spread, snap);
        let scale = f32::from(low + rng.u16(..=high - low)) / 100.0;
        taken.push(p);
        out.push(Painted { key, position: p.extend(z + lift), turn, scale });
    }
    out
}

/// The placeables near `center` (within `reach`) a dab must keep its
/// distance from, the stroke's among them: where each stands, and whether
/// it is of the brush's blueprints (counted towards Density).
fn around(view: &AreaView, keys: &[ResKey], center: Vec3, reach: f32) -> Vec<(Vec2, bool)> {
    let middle = center.truncate();
    let near = |p: Vec3| p.truncate().distance(middle) <= reach;
    let placed = view.model.iter().flat_map(|m| &m.objects).filter(|o| {
        o.kind == ObjectKind::Placeable && view.show[o.kind.index()] && near(o.position)
    });
    let ours = |o: &AreaObject| o.template.is_some_and(|t| keys.iter().any(|k| k.resref == t));
    placed
        .map(|o| (o.position.truncate(), ours(o)))
        .chain(
            view.paint
                .pending
                .iter()
                .filter(|p| near(p.position))
                .map(|p| (p.position.truncate(), true)),
        )
        .collect()
}

/// The ground's height at `p` near `center`'s: on the walkmesh's walkable
/// faces (not on a roof, in water, inside a wall), `None` off them; the
/// plane at `center` where the area has no walkmesh read.
fn ground(view: &AreaView, center: Vec3) -> impl Fn(Vec2) -> Option<f32> + '_ {
    move |p| match view.ground() {
        Some(g) => g.height_on(p, center.z, |m| view.walked_on(m)),
        None => Some(center.z),
    }
}

/// The dab `view`'s brush would lay at `center` now, by its seed.
fn next_dab(app: &Moonglow, view: &AreaView, keys: &[ResKey], center: Vec3) -> Vec<Painted> {
    let s = &app.settings.paint;
    let reach = f32::from(s.radius_cm) / 100.0 + f32::from(s.spacing_cm) / 100.0;
    let around = around(view, keys, center, reach);
    let ground = ground(view, center);
    let (turn, snap) = (view.ghost_turn(), view.snap_angle());
    dab(s, center, keys, &around, turn, snap, view.paint.seed, &ground)
}

/// The ground under the pointer, if it is over the view (not over a
/// window in front of it).
fn pointer(ui: &egui::Ui, view: &AreaView) -> Option<Vec3> {
    let pos = ui.ctx().pointer_latest_pos().filter(|p| view.rect.contains(*p))?;
    let layer = ui.ctx().layer_id_at(pos);
    if layer.is_some_and(|l| l != ui.layer_id() && l.order != egui::Order::Tooltip) {
        return None;
    }
    view.ground_at(pos, 0.0)
}

/// `p` as its blueprint's object would show it there.
fn object(template: &AreaObject, p: &Painted) -> AreaObject {
    let mut o = template.clone();
    o.position = p.position;
    o.rotation = template.rotation + p.turn;
    if p.scale != 1.0 {
        let visual = mg_area::VisualTransform { scale: Vec3::splat(p.scale), ..Default::default() };
        o.visual = Some(visual);
    }
    o
}

/// What the brush shows see-through this frame: the stroke's placeables,
/// and before a press, the next dab under the pointer.
pub(crate) fn ghosts(app: &mut Moonglow, ui: &egui::Ui, view: &mut AreaView) -> Vec<AreaObject> {
    let Some(keys) = active(app) else {
        // (Read anew when it is next on: the blueprints may have changed.)
        view.paint.templates.clear();
        view.paint.shown = 0;
        return Vec::new();
    };
    view.read_walkable(app);
    let mut shown = view.paint.pending.clone();
    if !view.paint.painting
        && let Some(at) = pointer(ui, view)
    {
        shown.extend(next_dab(app, view, &keys, at));
    }
    for key in &keys {
        if !view.paint.templates.contains_key(key) {
            let template = crate::area_view::template(app, *key);
            view.paint.templates.insert(*key, template);
        }
    }
    let templates = &view.paint.templates;
    let objects: Vec<AreaObject> = shown
        .iter()
        .filter_map(|p| templates.get(&p.key)?.as_ref().map(|t| object(t, p)))
        .collect();
    view.paint.shown = objects.len();
    objects
}

/// Shift + wheel over the view sizes the brush's circle: whether the wheel
/// is the brush's (it doesn't zoom then).
pub(crate) fn wheel(app: &mut Moonglow, ui: &egui::Ui, response: &egui::Response) -> bool {
    if active(app).is_none() || app.settings.paint.row || !response.hovered() {
        return false;
    }
    let (shift, d) = ui.input(|i| (i.modifiers.shift, i.smooth_scroll_delta));
    if !shift {
        return false;
    }
    // (egui turns the wheel sideways with Shift held.)
    let scroll = if d.y == 0.0 { d.x } else { d.y };
    if scroll != 0.0 {
        let r = &mut app.settings.paint.radius_cm;
        let to = (f32::from(*r) * (scroll * 0.003).exp()).round() as u16;
        // At least a centimeter a step, so that a small one tells.
        let to = match to.cmp(r) {
            std::cmp::Ordering::Equal if scroll > 0.0 => to + 1,
            std::cmp::Ordering::Equal => to.saturating_sub(1),
            _ => to,
        };
        *r = to.clamp(RADIUS.0, RADIUS.1);
    }
    true
}

/// The brush's clicks and drags on the view: whether they were its (Paint
/// is on with placeables chosen), and nothing else is to take them.
pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
) -> bool {
    let Some(keys) = active(app) else {
        view.paint.pending.clear();
        view.paint.painting = false;
        return false;
    };
    view.read_walkable(app);
    let hovered = response.hovered();
    // Escape or a right click: the stroke going on is dropped, else Paint
    // goes off (the placeables stay chosen).
    let stop =
        response.secondary_clicked() || (hovered && ui.input(|i| i.key_pressed(egui::Key::Escape)));
    if stop {
        if view.paint.painting {
            view.paint.pending.clear();
            view.paint.painting = false;
        } else {
            app.paint = false;
        }
        return true;
    }
    // A press with Ctrl or Alt held is the camera's (Ctrl + drag, Alt +
    // drag move the view): the brush lays nothing with it.
    let (pressed, down, camera, now) = ui.input(|i| {
        let p = &i.pointer;
        (p.primary_pressed(), p.primary_down(), i.modifiers.command || i.modifiers.alt, i.time)
    });
    if pressed && hovered && !camera {
        view.paint.painting = true;
        view.paint.last = None;
    }
    if !view.paint.painting {
        return true;
    }
    if let Some(at) = pointer(ui, view) {
        let step = (f32::from(app.settings.paint.radius_cm) / 100.0 * DAB_STEP).max(0.25);
        let due = view.paint.last.is_none_or(|(p, t)| {
            p.truncate().distance(at.truncate()) >= step || now - t >= DAB_EVERY
        });
        if due {
            let laid = next_dab(app, view, &keys, at);
            view.paint.pending.extend(laid);
            view.paint.seed = fastrand::u64(..);
            view.paint.last = Some((at, now));
        }
    }
    if down {
        // Held still, it goes on laying dabs.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(DAB_EVERY));
    } else {
        view.paint.painting = false;
        commit(app, view);
    }
    true
}

/// Puts the stroke's placeables into the area, as one command.
fn commit(app: &mut Moonglow, view: &mut AreaView) {
    use mg_module::instances::{Placement, Placing, instance};
    let pending = std::mem::take(&mut view.paint.pending);
    if pending.is_empty() {
        return;
    }
    let git = view.git();
    let list = ObjectKind::Placeable.list();
    let (Some(game), Some(ws)) = (app.game.as_deref(), app.ws.as_mut()) else { return };
    let _ = ws.flush();
    let Some(mut index) = ws.doc(&git).ok().map(|g| g.root.list(list).map_or(0, <[_]>::len)) else {
        return;
    };
    let read = |k: ResKey| mg_module::gff_root(Some(&ws.module), game, &k);
    let items = |r| read(ResKey::new(r, ResType::UTI));
    let placing = Placing { game, item: &items };
    let mut blueprints = HashMap::new();
    let mut edits = Vec::new();
    for p in pending {
        let Some(blueprint) = blueprints.entry(p.key).or_insert_with(|| read(p.key)) else {
            continue;
        };
        let placement = Placement { position: p.position.to_array(), rotation: p.turn };
        let Some(mut item) = instance(&placing, ResType::UTP, blueprint, placement, &[]) else {
            continue;
        };
        // (A static placeable takes no scale: the game gives it none.)
        if p.scale != 1.0 && item.integer("Static").unwrap_or(0) == 0 {
            let visual =
                mg_area::VisualTransform { scale: Vec3::splat(p.scale), ..Default::default() };
            item.set("VisTransformList", mg_area::edit::visual_transform_list(visual));
        }
        edits.push(mg_edit::Edit::insert_item(git, mg_edit::GffPath::root(), list, index, item));
        index += 1;
    }
    if !edits.is_empty() {
        app.actions.push(Action::Apply(Command::new("Paint Placeables", edits)));
    }
}

/// The brush's circle, on the ground around the pointer.
pub(crate) fn overlay(app: &Moonglow, ui: &egui::Ui, view: &AreaView) {
    if active(app).is_none() || app.settings.paint.row {
        return;
    }
    let Some(at) = pointer(ui, view) else { return };
    let radius = f32::from(app.settings.paint.radius_cm) / 100.0;
    const POINTS: usize = 64;
    let circle: Vec<Vec3> = (0..POINTS)
        .map(|k| {
            let a = k as f32 / POINTS as f32 * std::f32::consts::TAU;
            at + Vec3::new(a.cos(), a.sin(), 0.0) * radius
        })
        .collect();
    // On the walkable ground nearest the pointer's level, as a dab finds
    // it (not up a tower's wall beside it); level where there is none.
    let ground = ground(view, at);
    let circle: Vec<Vec3> = circle
        .into_iter()
        .map(|p| p.truncate().extend(ground(p.truncate()).unwrap_or(at.z) + 0.05))
        .collect();
    let painter = ui.painter_at(view.rect);
    for k in 0..POINTS {
        if let Some(line) = view.screen_segment(circle[k], circle[(k + 1) % POINTS]) {
            painter.line_segment(line, Stroke::new(2.0, CIRCLE));
        }
    }
    if let Some(middle) = view.screen_pos(at) {
        painter.circle_filled(middle, 2.5, CIRCLE);
    }
}

/// The status line while Paint is on.
pub(crate) fn status(app: &Moonglow) -> Option<String> {
    if !app.paint {
        return None;
    }
    let keys = keys(app);
    Some(match keys.len() {
        0 => "Paint: choose placeables in the palette (Ctrl + click for several); Escape: stop"
            .to_string(),
        n => {
            let what = if n == 1 { keys[0].resref.to_string() } else { format!("{n} placeables") };
            format!(
                "Painting {what}: drag to paint, Shift + wheel: the brush's size, Q and E: turn; \
                 right click or Escape: stop"
            )
        }
    })
}

/// A length kept in centimeters, edited in meters.
fn meters(ui: &mut egui::Ui, cm: &mut i32, range: (f32, f32)) -> egui::Response {
    let mut m = *cm as f32 / 100.0;
    let r = ui.add(
        egui::DragValue::new(&mut m)
            .speed(0.05)
            .range(range.0..=range.1)
            .fixed_decimals(2)
            .suffix(" m"),
    );
    if r.changed() {
        *cm = (m * 100.0).round() as i32;
    }
    r
}

/// The strip of the brush's settings, above the view while Paint is on.
pub(crate) fn strip(app: &mut Moonglow, ui: &mut egui::Ui) {
    use crate::widgets::field_label;
    let chosen = keys(app).len();
    let s = &mut app.settings.paint;
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut s.row, false, "Scatter")
            .on_hover_text("Paint at random in a circle");
        ui.separator();
        if !s.row {
            field_label(ui, "Radius");
            let mut cm = i32::from(s.radius_cm);
            let (low, high) = (f32::from(RADIUS.0) / 100.0, f32::from(RADIUS.1) / 100.0);
            meters(ui, &mut cm, (low, high)).on_hover_text("The circle's size (Shift + wheel)");
            s.radius_cm = cm.clamp(i32::from(RADIUS.0), i32::from(RADIUS.1)) as u16;
            field_label(ui, "Density");
            ui.add(egui::DragValue::new(&mut s.density).range(1..=400).suffix(" a tile"))
                .on_hover_text(
                    "How many placeables a tile's ground (10 by 10 m) holds once painted: the \
                     brush adds until the circle has its share, counting those of these \
                     blueprints already there",
                );
            field_label(ui, "Spacing");
            let mut cm = i32::from(s.spacing_cm);
            meters(ui, &mut cm, (0.0, 20.0))
                .on_hover_text("The least distance from one placeable to the next");
            s.spacing_cm = cm.clamp(0, 2000) as u16;
            field_label(ui, "Flow");
            ui.add(egui::DragValue::new(&mut s.flow).range(1..=100).suffix("%")).on_hover_text(
                "How much of what the circle lacks each dab adds: 100% fills it at once; less \
                 fills it as you go over it, or hold the button",
            );
            field_label(ui, "Turn");
            ui.add(egui::DragValue::new(&mut s.turn_spread).range(0..=360).suffix("°"))
                .on_hover_text(
                    "How far each one's facing strays from the brush's (Q and E turn it): 360° \
                     is any way",
                );
            field_label(ui, "Scale");
            ui.add(egui::DragValue::new(&mut s.scale_min).range(10..=500).suffix("%"));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut s.scale_max).range(10..=500).suffix("%"))
                .on_hover_text(
                    "Each one's scale, picked between these. Static placeables take none (the \
                     game gives them none)",
                );
        }
        field_label(ui, "Height");
        let mut cm = i32::from(s.lift_cm);
        meters(ui, &mut cm, (-10.0, 10.0))
            .on_hover_text("How far above the ground they stand (below it, less than 0)");
        s.lift_cm = cm.clamp(-1000, 1000) as i16;
        ui.separator();
        if chosen == 0 {
            ui.weak("Choose placeables in the palette (Ctrl + click for several)");
        } else {
            ui.weak(format!("{chosen} chosen"))
                .on_hover_text("Ctrl + click in the palette for more, or fewer");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> ResKey {
        ResKey::parse(name, ResType::UTP).unwrap()
    }

    fn settings() -> PaintSettings {
        PaintSettings {
            radius_cm: 500,
            density: 10,
            spacing_cm: 100,
            flow: 100,
            ..Default::default()
        }
    }

    fn flat(_: Vec2) -> Option<f32> {
        Some(2.0)
    }

    #[test]
    fn a_full_dab_fills_the_circle_to_its_density() {
        let s = settings();
        let keys = [key("plc_tree"), key("plc_shrub")];
        let at = Vec3::new(20.0, 20.0, 0.0);
        let laid = dab(&s, at, &keys, &[], 0.0, None, 1, &flat);
        // 10 a tile on 78.5 m²: 8.
        assert_eq!(laid.len(), 8);
        for (i, p) in laid.iter().enumerate() {
            assert!(p.position.truncate().distance(at.truncate()) <= 5.0, "inside the circle");
            assert_eq!(p.position.z, 2.0, "on the ground");
            assert!(keys.contains(&p.key));
            for q in &laid[i + 1..] {
                assert!(p.position.distance(q.position) >= 1.0, "kept apart");
            }
        }
        // The same seed, the same dab: what was shown is what is laid.
        assert_eq!(dab(&s, at, &keys, &[], 0.0, None, 1, &flat), laid);
    }

    #[test]
    fn a_dab_counts_what_is_there_and_keeps_its_distance() {
        let s = settings();
        let keys = [key("plc_tree")];
        let at = Vec3::new(20.0, 20.0, 0.0);
        // Five of the brush's already there, and a chest it isn't painting.
        let mut around: Vec<(Vec2, bool)> =
            (0..5).map(|k| (Vec2::new(18.0 + k as f32, 20.0), true)).collect();
        around.push((Vec2::new(20.0, 22.0), false));
        let laid = dab(&s, at, &keys, &around, 0.0, None, 3, &flat);
        assert_eq!(laid.len(), 3, "8 wanted, 5 there");
        for p in &laid {
            for (q, _) in &around {
                assert!(p.position.truncate().distance(*q) >= 1.0);
            }
        }
        // Full: nothing more.
        let mut full = around.clone();
        full.extend(laid.iter().map(|p| (p.position.truncate(), true)));
        assert!(dab(&s, at, &keys, &full, 0.0, None, 4, &flat).is_empty());
    }

    #[test]
    fn flow_lays_a_share_and_nothing_goes_off_the_walkmesh() {
        let s = PaintSettings { flow: 25, ..settings() };
        let keys = [key("plc_tree")];
        let at = Vec3::new(20.0, 20.0, 0.0);
        assert_eq!(dab(&s, at, &keys, &[], 0.0, None, 1, &flat).len(), 2, "a quarter of 8");
        // Walkmesh only east of x = 20.
        let half = |p: Vec2| (p.x >= 20.0).then_some(0.0);
        let s = settings();
        let laid = dab(&s, at, &keys, &[], 0.0, None, 5, &half);
        assert!(!laid.is_empty() && laid.iter().all(|p| p.position.x >= 20.0));
    }

    #[test]
    fn facings_stray_by_turn_and_scales_stay_between_scale() {
        let s = PaintSettings {
            turn_spread: 90,
            scale_min: 80,
            scale_max: 120,
            lift_cm: -30,
            ..settings()
        };
        let keys = [key("plc_tree")];
        let laid = dab(&s, Vec3::new(20.0, 20.0, 0.0), &keys, &[], 1.0, None, 9, &flat);
        assert!(!laid.is_empty());
        for p in &laid {
            assert!((p.turn - 1.0).abs() <= 45f32.to_radians() + 1e-4, "{}", p.turn);
            assert!((0.8..=1.2).contains(&p.scale));
            assert!((p.position.z - 1.7).abs() < 1e-4, "lowered by Height");
        }
        // Snapped to 45°: each facing a multiple of it.
        let s = PaintSettings { turn_spread: 360, ..settings() };
        for p in dab(&s, Vec3::new(20.0, 20.0, 0.0), &keys, &[], 0.0, Some(45.0), 2, &flat) {
            let steps = p.turn / 45f32.to_radians();
            assert!((steps - steps.round()).abs() < 1e-3, "{}", p.turn);
        }
    }
}
