//! Patrol targeting and bounded routes/progress observed from the live service.
use crate::live::{AreaLive, LiveStore, Op, TransformVector};
use crate::live_view::{now_ms, queue};
use crate::{Moonglow, area_view::AreaView};
use egui::{Color32, Stroke};
use glam::Vec3;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Default)]
pub(crate) struct Patrol {
    draft: Option<Draft>,
    routes: BTreeMap<String, Route>,
    boot: String,
    last: u64,
    seen: HashSet<String>,
}
#[derive(Debug)]
struct Draft {
    ids: Vec<String>,
    points: Vec<Vec3>,
    looping: bool,
    run: bool,
    finish_on_click: bool,
}
#[derive(Debug)]
struct Route {
    points: BTreeMap<usize, Vec3>,
    looping: Option<bool>,
    generation: Option<i64>,
    progress: String,
    at_ms: u64,
}

impl Patrol {
    pub(crate) fn cancel_aim(&mut self) {
        self.draft = None;
    }
    pub(crate) fn start(&mut self, ids: Vec<String>) {
        self.draft = Some(Draft {
            ids,
            points: Vec::new(),
            looping: false,
            run: false,
            finish_on_click: false,
        });
    }
    pub(crate) fn update(&mut self, store: &LiveStore, area: &str, live: &AreaLive) {
        let boot = store.boot();
        if self.boot != boot {
            self.routes.clear();
            // History survives boot changes and carries no boot stamp. Observe
            // future arrivals/results only; old routes cannot be trusted.
            let status = store.status(now_ms());
            self.seen = status["results"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r["command"]["id"].as_str().map(str::to_owned))
                .collect();
            self.last = store.events(&mg_live::events::Query { limit: 1, ..Default::default() }).1;
            if !self.boot.is_empty() {
                self.draft = None;
            }
            self.boot = boot;
        }
        let now = now_ms();
        // Results include their exact accepted command; use them to learn full
        // routes and observe replacement orders from this UI, scenes or agents.
        let status = store.status(now);
        if let Some(results) = status["results"].as_array() {
            for item in results.iter().rev() {
                let command = &item["command"];
                let Some(id) = command["id"].as_str() else { continue };
                if !self.seen.insert(id.into())
                    || item["result"]["ok"] != true
                    || command["area"] != area
                {
                    continue;
                }
                let target = command["target"].as_str().unwrap_or_default();
                match command["op"].as_str().unwrap_or_default() {
                    "patrol" => {
                        let points = command["points"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .take(16)
                            .enumerate()
                            .filter_map(|(i, p)| Some((i, position(p)?)))
                            .collect();
                        self.routes.insert(
                            target.into(),
                            Route {
                                points,
                                looping: Some(command["loop"] == true),
                                generation: None,
                                progress: "accepted; waiting for arrival".into(),
                                at_ms: now,
                            },
                        );
                    }
                    "npc_move" | "npc_attack" | "npc_say" | "set_faction" | "force_rest"
                    | "limbo" | "unlimbo" | "freeze" | "unfreeze" | "run_script" | "run_chunk"
                    | "kill" | "destroy" | "stop_patrol" | "teleport" | "pc_teleport" => {
                        self.routes.remove(target);
                    }
                    "stop_combat" => {
                        self.routes.remove(target);
                        for id in item["result"]["affected"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|id| id.as_str())
                        {
                            self.routes.remove(id);
                        }
                    }
                    _ => {}
                }
            }
            self.seen.retain(|id| results.iter().any(|r| r["command"]["id"] == *id));
        }
        let (events, last) = store.events(&mg_live::events::Query {
            since: self.last,
            area: Some(area.into()),
            kinds: vec!["patrol".into()],
            ..Default::default()
        });
        self.last = last;
        for e in events {
            let Some(id) = e.id else { continue };
            let Some(point) =
                e.data["point"].as_u64().and_then(|p| usize::try_from(p).ok()).filter(|p| *p < 16)
            else {
                continue;
            };
            if e.data["status"] == "blocked" {
                self.routes.remove(&id);
                continue;
            }
            if e.data["status"] != "reached" {
                continue;
            }
            let generation = e.data["generation"].as_i64();
            if self
                .routes
                .get(&id)
                .is_some_and(|r| r.generation.is_some() && r.generation != generation)
            {
                self.routes.remove(&id);
            }
            let route = self.routes.entry(id.clone()).or_insert_with(|| Route {
                points: BTreeMap::new(),
                looping: None,
                generation,
                progress: String::new(),
                at_ms: e.at_ms,
            });
            route.generation = generation;
            route.at_ms = e.at_ms;
            if route.looping.is_none()
                && let Some(at) = position(&e.data["position"])
            {
                route.points.insert(point, at);
            }
            route.progress = format!(
                "reached {} · lap {}",
                point + 1,
                e.data["lap"].as_u64().unwrap_or_default() + 1
            );
            if route.looping == Some(false) && point + 1 == route.points.len() {
                self.routes.remove(&id);
            }
        }
        self.routes.retain(|id, r| {
            live.find(id).is_some_and(|(_, o)| !o.dead) && now.saturating_sub(r.at_ms) < 125_000
        });
        if let Some(draft) = &mut self.draft {
            draft.ids.retain(|id| {
                live.find(id).is_some_and(|(k, o)| crate::live_targets::creature(k, o, false))
            });
            if draft.ids.is_empty() {
                self.draft = None;
            }
        }
    }
    pub(crate) fn count(&self) -> usize {
        self.routes.values().filter(|r| now_ms().saturating_sub(r.at_ms) < 125_000).count()
    }
}
fn position(v: &serde_json::Value) -> Option<Vec3> {
    let p = Vec3::new(v["x"].as_f64()? as f32, v["y"].as_f64()? as f32, v["z"].as_f64()? as f32);
    p.is_finite().then_some(p)
}
pub(crate) fn menu(app: &mut Moonglow, view: &mut AreaView, ui: &mut egui::Ui, ids: Vec<String>) {
    if ui.add_enabled(!ids.is_empty(), egui::Button::new("Patrol…")).clicked() {
        crate::live_aim::clear(view);
        view.live.patrol.start(ids.clone());
        ui.close();
    }
    if ui.add_enabled(!ids.is_empty(), egui::Button::new("Stop Patrol")).clicked() {
        for target in ids {
            queue(app, view, Op::StopPatrol { target });
        }
        ui.close();
    }
}
fn send(app: &mut Moonglow, view: &mut AreaView) {
    let Some(draft) = view.live.patrol.draft.take() else { return };
    if draft.points.is_empty() {
        view.live.patrol.draft = Some(draft);
        return;
    }
    for target in draft.ids {
        queue(
            app,
            view,
            Op::Patrol {
                target,
                points: draft
                    .points
                    .iter()
                    .map(|p| TransformVector { x: p.x, y: p.y, z: p.z })
                    .collect(),
                r#loop: draft.looping,
                run: draft.run,
            },
        );
    }
}
pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
) -> bool {
    let clicked = response.interact_pointer_pos().and_then(|p| view.ground_at(p, 0.));
    let Some(draft) = &mut view.live.patrol.draft else { return false };
    if matches!(crate::live_aim::gesture(ui, response), crate::live_aim::Gesture::Cancel) {
        view.live.patrol.draft = None;
        return true;
    }
    if response.clicked()
        && let Some(at) = clicked
        && draft.points.len() < 16
        && draft.points.last().is_none_or(|last| last.distance(at) > 0.05)
    {
        draft.points.push(at);
    }
    let finish = response.double_clicked() || (response.clicked() && draft.finish_on_click);
    if response.hovered() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        draft.finish_on_click = true;
    }
    let mut cancel = false;
    let mut open = true;
    egui::Window::new("Patrol")
        .id(egui::Id::new(("live-patrol", view.area)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(view.rect.left_top() + egui::vec2(12., 44.))
        .show(ui.ctx(), |ui| {
            ui.label(format!("{} NPCs · {} / 16 points", draft.ids.len(), draft.points.len()));
            ui.label(if draft.finish_on_click {
                "Click the final point on the map to send."
            } else {
                "Click points; double-click the final point to send."
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut draft.looping, "Loop");
                ui.checkbox(&mut draft.run, "Run");
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !draft.points.is_empty(),
                        egui::Button::new("Finish on next click"),
                    )
                    .clicked()
                {
                    draft.finish_on_click = true;
                }
                if ui
                    .add_enabled(!draft.points.is_empty(), egui::Button::new("Remove Last Point"))
                    .clicked()
                {
                    draft.points.pop();
                }
                cancel |= ui.button("Cancel Patrol").clicked();
            });
        });
    if cancel || !open {
        view.live.patrol.draft = None;
    } else if finish {
        send(app, view);
    }
    true
}
fn path(
    ui: &egui::Ui,
    view: &AreaView,
    points: impl Iterator<Item = (usize, Vec3)>,
    looping: bool,
    color: Color32,
    caption: &str,
) {
    let painter = ui.painter_at(view.rect);
    let points: Vec<_> =
        points.filter_map(|(i, p)| Some((i, view.screen_pos(p + Vec3::Z * 0.15)?))).collect();
    for pair in points.windows(2) {
        if pair[1].0 == pair[0].0 + 1 {
            painter.line_segment([pair[0].1, pair[1].1], Stroke::new(2., color));
        }
    }
    if looping && points.len() > 1 {
        painter.line_segment([points[0].1, points.last().unwrap().1], Stroke::new(2., color));
    }
    for (i, at) in &points {
        painter.circle_filled(*at, 10., Color32::from_black_alpha(200));
        painter.circle_stroke(*at, 10., Stroke::new(2., color));
        painter.text(
            *at,
            egui::Align2::CENTER_CENTER,
            (i + 1).to_string(),
            egui::FontId::proportional(13.),
            color,
        );
    }
    if let Some((_, at)) = points.first() {
        painter.text(
            *at + egui::vec2(0., -16.),
            egui::Align2::CENTER_BOTTOM,
            caption,
            egui::FontId::proportional(13.),
            color,
        );
    }
}
pub(crate) fn paint(ui: &egui::Ui, view: &AreaView) {
    for (id, r) in &view.live.patrol.routes {
        if view.live.is_selected(id) && now_ms().saturating_sub(r.at_ms) < 125_000 {
            path(
                ui,
                view,
                r.points.iter().map(|(i, p)| (*i, *p)),
                r.looping == Some(true),
                Color32::LIGHT_GREEN,
                &format!(
                    "{}: {}",
                    if r.looping.is_some() { "Patrol" } else { "Observed patrol points" },
                    r.progress
                ),
            );
        }
    }
    if let Some(draft) = &view.live.patrol.draft {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if let Some(at) = crate::live_aim::cursor(ui, view) {
            crate::live_aim::circle(ui, view, at, 0.5, Color32::LIGHT_BLUE);
            if let (Some(last), Some(end)) =
                (draft.points.last().and_then(|p| view.screen_pos(*p)), view.screen_pos(at))
            {
                ui.painter_at(view.rect)
                    .line_segment([last, end], Stroke::new(1., Color32::LIGHT_BLUE));
            }
        }
        path(
            ui,
            view,
            draft.points.iter().copied().enumerate(),
            draft.looping,
            Color32::LIGHT_BLUE,
            "Patrol draft",
        );
    }
}
