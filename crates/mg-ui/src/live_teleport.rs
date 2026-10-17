//! One teleport flow for reported objects. Only the command queue changes the game.
use crate::live::{LiveObject, Op};
use crate::{Moonglow, area_view::AreaView};
use egui::{Color32, Stroke};
use glam::Vec3;

#[derive(Debug)]
pub(crate) struct Pending {
    command: String,
    target: String,
    area: String,
    boot: String,
    started: u64,
}

pub(crate) fn track(
    view: &mut AreaView,
    command: String,
    target: String,
    area: String,
    boot: String,
) {
    view.live.teleports.push(Pending {
        command,
        target,
        area,
        boot,
        started: crate::live_view::now_ms(),
    });
}

/// Reconstruction may change a door/scenery/item ID. Wait for its report before
/// selecting the replacement; never keep commanding the destroyed original.
pub(crate) fn results(app: &mut Moonglow, view: &mut AreaView) {
    if view.live.teleports.is_empty() {
        return;
    }
    let Some(store) = app.live.clone() else { return };
    let now = crate::live_view::now_ms();
    let status = store.status(now);
    let results = status["results"].as_array();
    let boot = store.boot();
    for pending in std::mem::take(&mut view.live.teleports) {
        if pending.boot != boot || now.saturating_sub(pending.started) > 90_000 {
            continue;
        }
        let result = results
            .into_iter()
            .flatten()
            .find(|r| r["result"]["id"] == pending.command)
            .map(|r| &r["result"]);
        let Some(result) = result else {
            view.live.teleports.push(pending);
            continue;
        };
        if result["ok"] != true {
            continue;
        }
        let id = result["obj"].as_str().unwrap_or(&pending.target);
        if store.locate(id).is_none_or(|(area, _, _)| area != pending.area) {
            view.live.teleports.push(pending);
            continue;
        }
        let ids = view
            .live
            .selection_ids()
            .filter(|old| *old != pending.target)
            .map(str::to_owned)
            .collect();
        view.live.group.replace(&mut view.live.selected, ids);
        if pending.area == view.area.to_string() {
            if !view.live.is_selected(id) {
                view.live.select(Some(id.into()), true);
            }
        } else {
            if let Ok(area) = pending.area.parse::<mg_core::ResRef>()
                && let Some(destination) = app.area_views.get_mut(&area)
                && !destination.live.is_selected(id)
            {
                destination.live.select(Some(id.into()), true);
            }
            view.notice = Some(format!(
                "Teleported to {} (object {id}); open that area's Live view to follow",
                pending.area
            ));
        }
    }
}

#[derive(Debug)]
struct Target {
    id: String,
    name: String,
    offset: Vec3,
    facing: f32,
}

#[derive(Debug)]
pub(crate) struct Draft {
    targets: Vec<Target>,
    boot: String,
    area: String,
    areas: Vec<(String, Option<[u32; 2]>)>,
    point: Vec3,
    turn: f32,
    choose_area: bool,
    origin: (String, Vec3),
    map: Option<(String, u64, Option<crate::images::Picture>)>,
}

fn eligible(o: &LiveObject, players: bool) -> bool {
    !o.dead && crate::live_targets::allowed(o, players)
}

pub(crate) fn menu(
    app: &mut Moonglow,
    view: &mut AreaView,
    ui: &mut egui::Ui,
    objects: &[&LiveObject],
) {
    let players = app.live.as_ref().is_some_and(|s| s.policy().players);
    let enabled = objects.iter().any(|o| eligible(o, players));
    for (label, choose_area) in [("Teleport…", false), ("Teleport to Area…", true)] {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            let objects: Vec<_> = objects.iter().filter(|o| eligible(o, players)).collect();
            let origin = Vec3::new(objects[0].x, objects[0].y, objects[0].z);
            let store = app.live.as_ref().expect("live menu");
            let mut areas: Vec<_> = store.status(crate::live_view::now_ms())["areas"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a["area"].as_str())
                .map(|a| (a.to_owned(), store.area(a).and_then(|a| a.size)))
                .collect();
            // The server need not have visited/reported an area to teleport into it.
            if let Some(ws) = &mut app.ws {
                // Match the module tree, including areas not yet written into
                // the module-info document by an editor change.
                let module_areas: Vec<_> =
                    ws.module.keys_of(mg_core::ResType::ARE).map(|key| key.resref).collect();
                for area in module_areas {
                    let name = area.to_string();
                    let dimensions = ws
                        .doc(&mg_resman::ResKey::new(area, mg_core::ResType::ARE))
                        .ok()
                        .and_then(|g| {
                            Some([
                                u32::try_from(g.root.integer("Width")?).ok()? * 10,
                                u32::try_from(g.root.integer("Height")?).ok()? * 10,
                            ])
                        });
                    if let Some((_, size)) = areas.iter_mut().find(|(a, _)| *a == name) {
                        if size.is_none() {
                            *size = dimensions;
                        }
                    } else {
                        areas.push((name, dimensions));
                    }
                }
            }
            areas.sort_by(|a, b| a.0.cmp(&b.0));
            let mut draft = Draft {
                targets: objects
                    .iter()
                    .map(|o| Target {
                        id: o.id.clone(),
                        name: crate::live_panel::name(o).into(),
                        offset: Vec3::new(o.x, o.y, o.z) - origin,
                        facing: o.facing,
                    })
                    .collect(),
                boot: store.boot(),
                area: view.area.to_string(),
                areas,
                point: origin,
                turn: 0.,
                choose_area,
                origin: (view.area.to_string(), origin),
                map: None,
            };
            if choose_area {
                draft.centre();
            }
            crate::live_aim::clear(view);
            view.live.teleport = Some(draft);
            view.notice = None;
            ui.close();
        }
    }
    if objects.iter().any(|o| !eligible(o, players)) {
        ui.weak("Teleport excludes dead, DM/possessed and disabled player targets.");
    }
}

impl Draft {
    fn size(&self) -> Option<[u32; 2]> {
        self.areas
            .iter()
            .find(|a| a.0 == self.area)
            .and_then(|a| a.1)
            .filter(|[w, h]| *w > 0 && *h > 0)
    }
    fn centre(&mut self) {
        if let Some([w, h]) = self.size() {
            self.point = Vec3::new(w as f32 * 0.5, h as f32 * 0.5, 0.);
        }
    }
    fn map(&mut self, app: &mut Moonglow, ctx: &egui::Context) -> Option<crate::images::Picture> {
        let revision = app.ws.as_ref().map_or(0, |w| w.revision());
        if let Some((area, old_revision, picture)) = &self.map
            && *area == self.area
            && *old_revision == revision
        {
            return picture.clone();
        }
        let picture = (|| {
            let area = self.area.parse().ok()?;
            let are = app
                .ws
                .as_mut()?
                .doc(&mg_resman::ResKey::new(area, mg_core::ResType::ARE))
                .ok()?
                .root
                .clone();
            let game = app.game.as_ref()?;
            let set = mg_module::minimap::tileset(&game.resman, &are).ok()?;
            let pixels =
                mg_module::minimap::minimap(&game.resman, &are, &set, Some(32)).ok()?.top_down();
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [pixels.width as usize, pixels.height as usize],
                &pixels.data,
            );
            Some(crate::images::Picture {
                texture: ctx.load_texture(
                    format!("live-destination-{}", self.area),
                    image,
                    egui::TextureOptions::LINEAR,
                ),
                size: egui::vec2(pixels.width as f32, pixels.height as f32),
                drawn: egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            })
        })();
        self.map = Some((self.area.clone(), revision, picture.clone()));
        picture
    }

    fn valid(&self, point: Vec3) -> bool {
        point.is_finite()
            && self.turn.is_finite()
            && self.targets.iter().all(|o| {
                let p = point + o.offset;
                p.x >= 0.
                    && p.y >= 0.
                    && self.size().is_none_or(|[w, h]| p.x < w as f32 && p.y < h as f32)
            })
    }
}

pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
) -> bool {
    let Some(mut draft) = view.live.teleport.take() else { return false };
    if app.live.as_ref().is_none_or(|s| s.boot() != draft.boot) {
        view.notice = Some("Teleport cancelled: server changed".into());
        return true;
    }
    if matches!(crate::live_aim::gesture(ui, response), crate::live_aim::Gesture::Cancel) {
        return true;
    }
    if response.hovered() && !ui.ctx().egui_wants_keyboard_input() {
        for (cmd, step) in crate::area_view::turn_keys(view) {
            if ui.input(|i| app.keymap.pressed(i, cmd)) {
                draft.turn += step.to_degrees();
            }
        }
    }
    let mut send = false;
    let mut open = true;
    let mut cancel = false;
    if draft.choose_area {
        egui::Window::new("Teleport to Area")
            .id(egui::Id::new(("live-teleport-area", view.area)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(view.rect.left_top() + egui::vec2(12., 44.))
            .show(ui.ctx(), |ui| {
                ui.label(format!("{} objects · relative offsets kept", draft.targets.len()));
                let old = draft.area.clone();
                egui::ComboBox::from_label("Destination area").selected_text(&draft.area).show_ui(
                    ui,
                    |ui| {
                        for (area, _) in &draft.areas {
                            ui.selectable_value(&mut draft.area, area.clone(), area);
                        }
                    },
                );
                if draft.area != old {
                    draft.centre();
                }
                if let Some([w, h]) = draft.size() {
                    ui.label(format!("{w} × {h} m · click the map to teleport"));
                    let size = egui::vec2(w as f32, h as f32);
                    let (rect, map) = ui.allocate_exact_size(
                        size * (240. / size.max_elem().max(1.)),
                        egui::Sense::click_and_drag(),
                    );
                    ui.painter().rect_filled(rect, 2., Color32::from_gray(25));
                    if let Some(picture) = draft.map(app, ui.ctx()) {
                        ui.painter().image(
                            picture.texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                            Color32::WHITE,
                        );
                    }
                    let at = |p: Vec3| {
                        egui::pos2(
                            rect.left() + p.x / w as f32 * rect.width(),
                            rect.bottom() - p.y / h as f32 * rect.height(),
                        )
                    };
                    let painter = ui.painter_at(rect);
                    for x in (0..=w).step_by(10) {
                        painter.line_segment(
                            [
                                at(Vec3::new(x as f32, 0., 0.)),
                                at(Vec3::new(x as f32, h as f32, 0.)),
                            ],
                            Stroke::new(0.5, Color32::from_white_alpha(65)),
                        );
                    }
                    for y in (0..=h).step_by(10) {
                        painter.line_segment(
                            [
                                at(Vec3::new(0., y as f32, 0.)),
                                at(Vec3::new(w as f32, y as f32, 0.)),
                            ],
                            Stroke::new(0.5, Color32::from_white_alpha(65)),
                        );
                    }
                    painter.text(
                        rect.left_bottom() - egui::vec2(-3., 3.),
                        egui::Align2::LEFT_BOTTOM,
                        "0, 0",
                        egui::FontId::proportional(12.),
                        Color32::WHITE,
                    );
                    painter.text(
                        rect.right_top() + egui::vec2(-3., 3.),
                        egui::Align2::RIGHT_TOP,
                        format!("{w}, {h}"),
                        egui::FontId::proportional(12.),
                        Color32::WHITE,
                    );
                    if draft.origin.0 == draft.area {
                        for target in &draft.targets {
                            painter.circle_filled(
                                at(draft.origin.1 + target.offset),
                                3.,
                                Color32::LIGHT_BLUE,
                            );
                        }
                    }
                    if map.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                    }
                    map.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Destination map")
                    });
                    if matches!(
                        crate::live_aim::gesture(ui, &map),
                        crate::live_aim::Gesture::Cancel
                    ) {
                        cancel = true;
                    }
                    if map.hovered()
                        && let Some(p) = ui.input(|i| i.pointer.hover_pos())
                    {
                        draft.point.x = ((p.x - rect.left()) / rect.width() * w as f32)
                            .clamp(0., w as f32 - 0.01);
                        draft.point.y = ((rect.bottom() - p.y) / rect.height() * h as f32)
                            .clamp(0., h as f32 - 0.01);
                    }
                    send = map.clicked() && draft.valid(draft.point);
                    for o in &draft.targets {
                        let p = draft.point + o.offset;
                        let at = egui::pos2(
                            rect.left() + p.x / w as f32 * rect.width(),
                            rect.bottom() - p.y / h as f32 * rect.height(),
                        );
                        ui.painter_at(rect).circle_stroke(
                            at,
                            4.,
                            Stroke::new(1.5, Color32::LIGHT_GREEN),
                        );
                    }
                    ui.weak("Blue: current position · green: destination · grid: 10 m");
                    ui.weak("Tile map from the open module; runtime tile edits may differ.");
                    ui.weak(format!(
                        "Current: {} ({:.1}, {:.1})",
                        draft.origin.0, draft.origin.1.x, draft.origin.1.y
                    ));
                    if ui.button("Area Centre").clicked() {
                        draft.centre();
                    }
                } else {
                    ui.weak("Waiting for area dimensions to show its destination map.");
                }
                for (label, value) in [
                    ("Destination X", &mut draft.point.x),
                    ("Destination Y", &mut draft.point.y),
                    ("Destination Z", &mut draft.point.z),
                    ("Facing offset", &mut draft.turn),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.label(label);
                        ui.add(egui::DragValue::new(value).speed(0.1)).labelled_by(label.id);
                    });
                }
                ui.weak(
                    "The game checks the destination; doors/static objects may refuse movement.",
                );
                let valid = draft.valid(draft.point);
                if !valid {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        "Keep every landing point inside the area.",
                    );
                }
                ui.horizontal(|ui| {
                    ui.weak("Only a click on the destination map teleports.");
                    cancel = ui.button("Cancel Teleport").clicked();
                });
            });
    } else if response.clicked()
        && let Some(p) = response.interact_pointer_pos().and_then(|p| view.ground_at(p, 0.))
    {
        draft.point = p;
        send = draft.valid(p);
        if !send {
            view.notice = Some("Keep every teleport landing point inside the area".into());
        }
    }
    if !open || cancel {
        return true;
    }
    if send {
        for target in draft.targets {
            let p = draft.point + target.offset;
            let id = target.id;
            if let Some(command) = crate::live_view::queue_in_area(
                app,
                view,
                draft.area.clone(),
                Op::Teleport {
                    target: id.clone(),
                    x: p.x,
                    y: p.y,
                    z: p.z,
                    f: Some((target.facing + draft.turn).rem_euclid(360.)),
                },
            ) {
                view.live.teleports.push(Pending {
                    command,
                    target: id,
                    area: draft.area.clone(),
                    boot: draft.boot.clone(),
                    started: crate::live_view::now_ms(),
                });
            }
        }
    } else {
        view.live.teleport = Some(draft);
    }
    true
}

pub(crate) fn paint(ui: &egui::Ui, view: &AreaView) {
    let Some(draft) = &view.live.teleport else { return };
    if draft.choose_area {
        return;
    }
    let painter = ui.painter_at(view.rect);
    painter.text(
        view.rect.center_bottom() - egui::vec2(0., 28.),
        egui::Align2::CENTER_BOTTOM,
        format!(
            "Teleport {} · click destination · turn keys: facing · Esc / right click: cancel",
            draft.targets.len()
        ),
        egui::FontId::proportional(14.),
        Color32::WHITE,
    );
    let Some(at) = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| view.rect.contains(*p))
        .and_then(|p| view.ground_at(p, 0.))
    else {
        return;
    };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    let color = if draft.valid(at) { Color32::LIGHT_GREEN } else { Color32::LIGHT_RED };
    for target in &draft.targets {
        let p = at + target.offset + Vec3::Z * 0.1;
        let Some(screen) = view.screen_pos(p) else { continue };
        painter.circle_filled(screen, 9., Color32::from_black_alpha(100));
        painter.circle_stroke(screen, 9., Stroke::new(2., color));
        let f = (target.facing + draft.turn).to_radians();
        if let Some(tip) = view.screen_pos(p + Vec3::new(f.cos(), f.sin(), 0.) * 0.8) {
            painter.arrow(screen, tip - screen, Stroke::new(2., color));
        }
        painter.text(
            screen + egui::vec2(12., -12.),
            egui::Align2::LEFT_BOTTOM,
            &target.name,
            egui::FontId::proportional(13.),
            color,
        );
    }
}
