//! Live capture clipboard: no module edits, cut only after every capture succeeds.
use crate::{
    Moonglow,
    area_view::AreaView,
    live::{Command, Op},
};
use glam::Vec3;
use mg_area::{AreaObject, ObjectKind};
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct Source {
    id: String,
    area: String,
    kind: ObjectKind,
    at: Vec3,
    facing: f32,
}
#[derive(Debug, Clone)]
struct Entry {
    json: serde_json::Value,
    offset: Vec3,
    facing: f32,
    preview: AreaObject,
}
#[derive(Debug)]
struct Capture {
    sources: Vec<Source>,
    commands: Vec<String>,
    cut: bool,
    started: u64,
}
#[derive(Debug, Default)]
pub struct Clipboard {
    module: Option<PathBuf>,
    boot: String,
    entries: Vec<Entry>,
    capture: Option<Capture>,
    confirm: Option<Vec<Source>>,
    pub notice: Option<String>,
}
#[derive(Debug)]
pub(crate) struct Paste {
    entries: Vec<Entry>,
    boot: String,
    module: Option<PathBuf>,
}

fn selected(app: &Moonglow, view: &AreaView) -> Vec<Source> {
    let Some(store) = &app.live else { return Vec::new() };
    view.live
        .selection_ids()
        .filter_map(|id| {
            let (area, kind, o) = store.locate(id)?;
            (matches!(kind, ObjectKind::Creature | ObjectKind::Placeable)
                && !o.pc
                && !o.dm
                && !o.possessed)
                .then(|| Source {
                    id: o.id.clone(),
                    area,
                    kind,
                    at: o.position(),
                    facing: o.facing,
                })
        })
        .collect()
}
fn send(app: &mut Moonglow, area: String, op: Op) -> Result<String, String> {
    crate::live_scene::enqueue(app, Command { id: String::new(), area, op })
        .map_err(|e| e.to_string())
}
fn capture(app: &mut Moonglow, state: &mut Clipboard, sources: Vec<Source>, cut: bool) {
    let commands: Result<Vec<_>, _> = sources
        .iter()
        .map(|s| send(app, s.area.clone(), Op::CaptureObject { target: s.id.clone() }))
        .collect();
    match commands {
        Ok(commands) => {
            state.capture = Some(Capture { sources, commands, cut, started: mg_live::now_ms() });
            state.notice =
                Some("Capturing selection; originals stay until every capture succeeds.".into());
        }
        Err(e) => state.notice = Some(e),
    }
}
pub(crate) fn update(app: &mut Moonglow, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.live_clipboard);
    let module = app.module_path();
    let boot = app.live.as_ref().map(|s| s.boot()).unwrap_or_default();
    if state.module != module || state.boot != boot {
        state = Clipboard { module, boot, ..Default::default() };
        for v in app.area_views.values_mut() {
            v.live.paste = None;
        }
    }
    if let Some(p) = state.capture.take() {
        let results: Vec<_> =
            p.commands.iter().map(|id| app.live.as_ref().and_then(|s| s.result(id))).collect();
        let failed = results.iter().flatten().find(|r| !r.ok).map(|r| r.err.clone());
        if let Some(err) = failed {
            state.notice = Some(format!("Copy failed; originals retained: {err}"));
        } else if results.iter().all(Option::is_some) {
            let copied = (|| {
                let game = app.game.as_ref().ok_or("Game resources unavailable")?;
                let origin = p.sources[0].at;
                p.sources
                    .iter()
                    .zip(results)
                    .map(|(source, r)| {
                        let json = r.unwrap().object_json.ok_or("Missing capture snapshot")?;
                        let gff =
                            mg_gff::from_json(&json, game.codepage()).map_err(|e| e.to_string())?;
                        let expected =
                            if source.kind == ObjectKind::Creature { "UTC " } else { "UTP " };
                        if json["__data_type"] != expected {
                            return Err("Capture type changed; originals retained".into());
                        }
                        let command = Command {
                            id: String::new(),
                            area: source.area.clone(),
                            op: Op::SpawnJson { json: json.clone(), x: 0., y: 0., z: 0., f: 0. },
                        };
                        command.validate()?;
                        Ok(Entry {
                            json,
                            offset: source.at - origin,
                            facing: source.facing,
                            preview: AreaObject::read(game, source.kind, usize::MAX, &gff.root),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })();
            match copied {
                Err(e) => state.notice = Some(format!("Copy failed; originals retained: {e}")),
                Ok(entries) => {
                    state.entries = entries;
                    state.notice = Some(format!(
                        "Copied {} objects. Ctrl+V, then click the map to paste.",
                        state.entries.len()
                    ));
                    if p.cut {
                        // Refuse a changed session/identity rather than deleting an unrelated target.
                        let valid = app.live.as_ref().is_some_and(|s| {
                            p.sources.iter().all(|o| {
                                s.locate(&o.id).is_some_and(|(a, k, v)| {
                                    a == o.area && k == o.kind && !v.pc && !v.dm && !v.possessed
                                })
                            })
                        });
                        if valid {
                            for o in p.sources {
                                if let Err(e) = send(app, o.area, Op::Destroy { target: o.id }) {
                                    state.notice =
                                        Some(format!("Copy retained; removal failed: {e}"));
                                    break;
                                }
                            }
                        } else {
                            state.notice =
                                Some("Copied; removal cancelled because targets changed.".into());
                        }
                    }
                }
            }
        } else if mg_live::now_ms().saturating_sub(p.started) > 120_000 {
            state.notice = Some("Capture timed out; originals retained.".into());
        } else {
            state.capture = Some(p);
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    if let Some(sources) = state.confirm.take() {
        let mut open = true;
        let mut yes = false;
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Cut Live selection").open(&mut open).collapsible(false).resizable(false).show(ctx,|ui|{
            ui.label(format!("Capture and remove {} objects?",sources.len()));
            ui.label("Originals are removed only after all captures succeed. The clipboard lasts until this session ends.");
            ui.horizontal(|ui|{cancel|=ui.button("Cancel").clicked();yes=ui.button(format!("Cut {} objects",sources.len())).clicked();});
        });
        if yes {
            capture(app, &mut state, sources, true);
        } else if open && !cancel {
            state.confirm = Some(sources);
        }
    }
    for view in app.area_views.values_mut().filter(|v| v.live.on) {
        if state.notice.is_some() {
            view.notice.clone_from(&state.notice);
        }
    }
    app.live_clipboard = state;
}

pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
) -> bool {
    if let Some(paste) = view.live.paste.take() {
        if app.module_path() != paste.module
            || app.live.as_ref().is_none_or(|s| s.boot() != paste.boot)
        {
            return true;
        }
        if response.hovered() && !ui.ctx().egui_wants_keyboard_input() {
            for (cmd, by) in crate::area_view::turn_keys(view) {
                if ui.input(|i| app.keymap.pressed(i, cmd)) {
                    view.ghost_turn += by;
                }
            }
        }
        match crate::live_aim::gesture(ui, response) {
            crate::live_aim::Gesture::Cancel => {}
            crate::live_aim::Gesture::Click(p) => {
                if let Some(at) = view.ground_at(p, 0.) {
                    for e in paste.entries {
                        let pos = at + glam::Quat::from_rotation_z(view.ghost_turn) * e.offset;
                        if crate::live_view::queue(
                            app,
                            view,
                            Op::SpawnJson {
                                json: e.json,
                                x: pos.x,
                                y: pos.y,
                                z: pos.z,
                                f: (e.facing + view.ghost_turn.to_degrees()).rem_euclid(360.),
                            },
                        )
                        .is_none()
                        {
                            break;
                        }
                    }
                }
            }
            crate::live_aim::Gesture::Idle => view.live.paste = Some(paste),
        }
        return true;
    }
    if !response.hovered() || ui.ctx().egui_wants_keyboard_input() {
        return false;
    }
    let (copy, cut, paste) = crate::widgets::copy_cut_paste(ui);
    if !(copy || cut || paste) {
        return false;
    }
    let mut state = std::mem::take(&mut app.live_clipboard);
    if state.capture.is_some() || state.confirm.is_some() {
        state.notice = Some("Finish the pending capture first.".into());
    } else if copy || cut {
        let sources = selected(app, view);
        if sources.is_empty() {
            state.notice =
                Some("Select NPCs or placeables; players and DMs cannot be copied.".into());
        } else if cut && app.live.as_ref().is_none_or(|s| !s.policy().scripts) {
            state.notice = Some(
                "Enable module scripts in Live Options before cutting: pasting requires them."
                    .into(),
            );
        } else if cut {
            state.confirm = Some(sources);
        } else {
            capture(app, &mut state, sources, false);
        }
    } else if state.entries.is_empty() {
        state.notice = Some("Copy a Live NPC or placeable first.".into());
    } else if app.live.as_ref().is_none_or(|s| !s.policy().scripts) {
        state.notice =
            Some("Enable module scripts in Live Options to paste captured objects.".into());
    } else {
        crate::live_aim::clear(view);
        crate::live_tiles::stop(app, view);
        app.palette.selected = None;
        view.ghost_turn = 0.;
        view.live.paste = Some(Paste {
            entries: state.entries.clone(),
            boot: state.boot.clone(),
            module: state.module.clone(),
        });
        state.notice = None;
    }
    view.notice.clone_from(&state.notice);
    app.live_clipboard = state;
    true
}
pub(crate) fn ghosts(ui: &egui::Ui, view: &AreaView) -> Vec<AreaObject> {
    let Some(paste) = &view.live.paste else { return Vec::new() };
    let Some(at) = crate::live_aim::cursor(ui, view) else { return Vec::new() };
    paste
        .entries
        .iter()
        .map(|e| {
            let mut o = e.preview.clone();
            o.position = at + glam::Quat::from_rotation_z(view.ghost_turn) * e.offset;
            o.rotation = (e.facing - 90.).to_radians() + view.ghost_turn;
            o
        })
        .collect()
}
pub(crate) fn paint(ui: &egui::Ui, view: &AreaView) {
    let label = if let Some(p) = &view.live.paste {
        Some(format!(
            "Paste {} objects; click map; Q/E turn; Esc/right click cancel",
            p.entries.len()
        ))
    } else {
        view.notice.clone()
    };
    if let Some(label) = label {
        ui.painter_at(view.rect).text(
            view.rect.center_bottom() - egui::vec2(0., 45.),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(14.),
            egui::Color32::WHITE,
        );
    }
}
