//! Shared map gesture and preview primitives for every Live location action.
use crate::{
    Moonglow,
    area_view::AreaView,
    live::{Op, TransformVector},
};
use egui::{Color32, Pos2, Stroke};
use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Gesture {
    Idle,
    Cancel,
    Click(Pos2),
}

pub(crate) fn gesture(ui: &egui::Ui, response: &egui::Response) -> Gesture {
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        Gesture::Cancel
    } else if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        Gesture::Click(p)
    } else {
        Gesture::Idle
    }
}

pub(crate) fn cursor(ui: &egui::Ui, view: &AreaView) -> Option<Vec3> {
    let p = ui.input(|i| i.pointer.hover_pos()).filter(|p| view.rect.contains(*p))?;
    if ui
        .ctx()
        .layer_id_at(p)
        .is_some_and(|l| l != ui.layer_id() && l.order != egui::Order::Tooltip)
    {
        return None;
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    view.ground_at(p, 0.)
}

pub(crate) fn circle(ui: &egui::Ui, view: &AreaView, at: Vec3, radius: f32, color: Color32) {
    let points: Vec<_> = (0..=64)
        .filter_map(|n| {
            let a = n as f32 * std::f32::consts::TAU / 64.;
            view.screen_pos(at + Vec3::new(a.cos() * radius, a.sin() * radius, 0.08))
        })
        .collect();
    ui.painter_at(view.rect).add(egui::Shape::line(points, Stroke::new(2., color)));
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Source {
    Menu,
    Skill,
    Vfx,
    Library,
}

#[derive(Debug)]
pub(crate) struct Place {
    pub op: Op,
    pub label: String,
    pub source: Source,
    pub preview: Option<mg_area::AreaObject>,
    boot: String,
    module: Option<std::path::PathBuf>,
}

pub(crate) fn clear(view: &mut AreaView) {
    view.live.drag = None;
    view.live.trigger_from = None;
    view.live.paste = None;
    view.live.aim = None;
    view.live.place_aim = None;
    view.live.teleport = None;
    view.live.group.cancel_aim();
    view.live.patrol.cancel_aim();
}

pub(crate) fn start(app: &Moonglow, view: &mut AreaView, op: Op, label: &str, source: Source) {
    let preview = if let Op::SpawnJson { json, .. } = &op {
        app.game.as_deref().and_then(|g| crate::live_view::player_look(g, json).ok())
    } else {
        None
    };
    clear(view);
    view.ghost_turn = 0.;
    view.live.place_aim = Some(Place {
        op,
        label: label.into(),
        source,
        preview,
        boot: app.live.as_ref().map(|s| s.boot()).unwrap_or_default(),
        module: app.module_path(),
    });
    view.notice = None;
}

pub(crate) fn arm(
    app: &mut Moonglow,
    area: mg_core::ResRef,
    op: Op,
    label: &str,
    source: Source,
) -> Result<(), String> {
    let Some(mut view) = app.area_views.remove(&area) else {
        return Err("Open the Live area view first.".into());
    };
    crate::live_tiles::stop(app, &mut view);
    start(app, &mut view, op, label, source);
    app.area_views.insert(area, view);
    app.palette.selected = None;
    Ok(())
}

pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
) -> bool {
    let Some(mut draft) = view.live.place_aim.take() else { return false };
    if draft.module != app.module_path() || app.live.as_ref().is_none_or(|s| s.boot() != draft.boot)
    {
        view.notice = Some("Aim cancelled: session changed".into());
        return true;
    }
    let event = gesture(ui, response);
    if matches!(event, Gesture::Cancel) {
        return true;
    }
    if let Op::SkillCheck {
        target,
        x,
        y,
        radius,
        skill,
        dc,
        bonus,
        show_dc,
        timeout,
        success_text,
        fail_text,
    } = &mut draft.op
    {
        *target = None;
        *x = None;
        *y = None;
        *radius = Some(app.live_skill.radius);
        *skill = app.live_skill.skill;
        *dc = app.live_skill.dc;
        *bonus = app.live_skill.bonus;
        *show_dc = app.live_skill.show_dc;
        *timeout = app.live_skill.timeout;
        success_text.clone_from(&app.live_skill.success_text);
        fail_text.clone_from(&app.live_skill.fail_text);
    }
    if let Op::StopCombat { radius: Some(radius), .. } = &mut draft.op {
        let mut open = true;
        egui::Window::new("Stop Fighting radius")
            .id(egui::Id::new(("stop-radius", view.area)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(view.rect.left_top() + egui::vec2(12., 44.))
            .show(ui.ctx(), |ui| {
                let label = ui.label("Radius");
                ui.add(egui::DragValue::new(radius).range(0.1..=50.).suffix(" m"))
                    .labelled_by(label.id);
                ui.label("Click the map to stop nearby NPCs. Esc/right click cancels.");
            });
        if !open {
            return true;
        }
    }
    if response.hovered() && !ui.ctx().egui_wants_keyboard_input() {
        for (cmd, by) in crate::area_view::turn_keys(view) {
            if ui.input(|i| app.keymap.pressed(i, cmd)) {
                view.ghost_turn += by;
            }
        }
    }
    if let Gesture::Click(pos) = event
        && let Some(at) = view.ground_at(pos, 0.)
    {
        match &mut draft.op {
            Op::StopCombat { x, y, .. } | Op::SkillCheck { x, y, .. } => {
                *x = Some(at.x);
                *y = Some(at.y);
            }
            Op::JumpAllPlayers { x, y, z } => {
                *x = at.x;
                *y = at.y;
                *z = at.z;
            }
            Op::SpawnJson { x, y, z, f, .. } => {
                *x = at.x;
                *y = at.y;
                *z = at.z;
                *f = (view.ghost_turn.to_degrees() + 90.).rem_euclid(360.);
            }
            Op::ApplyVfx { location, .. } => {
                *location = Some(TransformVector { x: at.x, y: at.y, z: at.z });
            }
            _ => unreachable!("only place operations arm this draft"),
        }
        if let Some(id) = crate::live_view::queue(app, view, draft.op.clone()) {
            view.live.cues.ground_command(id.clone(), &draft.op);
            match draft.source {
                Source::Skill => app.live_skill.aim_sent(id, draft.boot, draft.module),
                Source::Library => app.live_library.aim_sent(id, draft.boot),
                Source::Vfx => {
                    app.live_vfx.pending.push(id);
                    app.live_vfx.notice = Some("Effect queued at the clicked point.".into());
                }
                Source::Menu => {}
            }
        }
    } else {
        view.live.place_aim = Some(draft);
    }
    true
}

pub(crate) fn paint(ui: &egui::Ui, view: &AreaView) {
    let Some(draft) = &view.live.place_aim else { return };
    ui.painter_at(view.rect).text(
        view.rect.center_bottom() - egui::vec2(0., 28.),
        egui::Align2::CENTER_BOTTOM,
        format!("{} · click map to send · Esc / right click: cancel", draft.label),
        egui::FontId::proportional(14.),
        Color32::WHITE,
    );
    if let Some(at) = cursor(ui, view) {
        let radius = match draft.op {
            Op::StopCombat { radius, .. } | Op::SkillCheck { radius, .. } => radius.unwrap_or(0.5),
            _ => 0.6,
        };
        circle(ui, view, at, radius, Color32::LIGHT_GREEN);
    }
}

pub(crate) fn ghost(ui: &egui::Ui, view: &AreaView) -> Option<mg_area::AreaObject> {
    let mut object = view.live.place_aim.as_ref()?.preview.clone()?;
    object.position = cursor(ui, view)?;
    object.rotation = view.ghost_turn;
    Some(object)
}
