//! Single-cell live tile replacement. The workspace remains the authoring source;
//! only the view model receives tiles observed in the bounded event log.
use crate::live::{LiveStore, Op};
use crate::{Moonglow, area_view::AreaView};
use egui::{Color32, Stroke};
use mg_area::AreaTile;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};

type Cell = (u16, u16);
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Tile {
    id: u16,
    orientation: u8,
    height: u8,
}
impl Tile {
    fn read(v: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: v["tile_id"].as_u64()?.try_into().ok()?,
            orientation: v["orientation"].as_u64()?.try_into().ok().filter(|v| *v < 4)?,
            height: v["height"].as_u64()?.try_into().ok()?,
        })
    }
    fn shown(t: &AreaTile) -> Option<Self> {
        Some(Self {
            id: t.id.try_into().ok()?,
            orientation: t.orientation,
            height: t.height.try_into().ok()?,
        })
    }
    fn op(self, (x, y): Cell) -> Op {
        Op::SetTile { x, y, tile_id: self.id, orientation: self.orientation, height: self.height }
    }
}
#[derive(Debug, Default)]
pub(crate) struct Tiles {
    source: Weak<LiveStore>,
    boot: String,
    last: u64,
    observed: BTreeMap<Cell, Tile>,
    before: BTreeMap<Cell, Tile>,
    originals: BTreeMap<usize, AreaTile>,
    dirty: bool,
    showing: bool,
    cell: Option<Cell>,
    draft: Tile,
    paint: bool,
    palette_brush: bool,
    search: String,
    hover: Option<Cell>,
    stroke: BTreeSet<Cell>,
    confirmed: bool,
    confirm: Option<(Cell, Tile)>,
    sent: BTreeMap<Cell, (String, Tile)>,
    message: String,
}
impl Tiles {
    pub(crate) fn rebase(&mut self) {
        self.originals.clear();
        self.dirty = true;
    }
    pub(crate) fn observe(&mut self, store: &Arc<LiveStore>, area: &str, bounds: (u32, u32)) {
        let boot = store.boot();
        let source = Arc::downgrade(store);
        if self.boot != boot || !self.source.ptr_eq(&source) {
            self.observed.clear();
            self.before.clear();
            self.sent.clear();
            // The event log survives boots and does not tag entries with a boot.
            // Start at its cursor, never replay ambiguous historical terrain.
            self.last = store.events(&mg_live::events::Query { limit: 1, ..Default::default() }).1;
            self.confirmed = false;
            self.confirm = None;
            self.paint = false;
            self.cell = None;
            self.boot = boot;
            self.source = source;
            self.dirty = true;
            self.message.clear();
        }
        self.sent.retain(|_, (id, _)| {
            if let Some(r) = store.result(id) {
                self.message = if r.ok {
                    "Accepted; waiting for the tile event".into()
                } else {
                    format!("Tile command failed: {}", r.err)
                };
                false
            } else {
                true
            }
        });
        let (events, last) = store.events(&mg_live::events::Query {
            since: self.last,
            area: Some(area.into()),
            kinds: vec!["tile".into()],
            ..Default::default()
        });
        self.last = last;
        for e in events {
            let v = &e.data;
            let Some(cell) = v["x"]
                .as_u64()
                .and_then(|x| u16::try_from(x).ok())
                .zip(v["y"].as_u64().and_then(|y| u16::try_from(y).ok()))
            else {
                continue;
            };
            if u32::from(cell.0) >= bounds.0 || u32::from(cell.1) >= bounds.1 {
                continue;
            }
            let Some(tile) = Tile::read(v) else { continue };
            if let Some(before) = Tile::read(&v["before"]) {
                self.before.entry(cell).or_insert(before);
            }
            self.observed.insert(cell, tile);
            self.dirty = true;
            self.message = format!("Reported tile {}, {}: {}", cell.0, cell.1, tile.id);
        }
    }
}
/// Apply known tiles to presentation data only. Return whether GPU/ground caches need rebuilding.
pub(crate) fn apply(view: &mut AreaView) -> bool {
    let state = &mut view.live_tiles;
    if !state.dirty && state.showing == view.live.on {
        return false;
    }
    let Some(model) = &mut view.model else { return false };
    let mut changed = false;
    for (i, old) in std::mem::take(&mut state.originals) {
        if let Some(t) = model.tiles.get_mut(i) {
            *t = old;
            changed = true;
        }
    }
    if view.live.on {
        if let Some(tools) = &view.terrain {
            for (&(x, y), v) in &state.observed {
                if u32::from(x) >= model.width || u32::from(y) >= model.height {
                    continue;
                }
                let Some(def) =
                    tools.set.tiles.get(usize::from(v.id)).filter(|t| !t.model.is_empty())
                else {
                    continue;
                };
                let i = usize::from(y) * model.width as usize + usize::from(x);
                let Some(tile) = model.tiles.get_mut(i) else { continue };
                state.originals.insert(i, tile.clone());
                tile.id = v.id.into();
                tile.orientation = v.orientation;
                tile.height = v.height.into();
                tile.position.z = f32::from(v.height) * model.height_step;
                tile.model = Some(def.model.to_ascii_lowercase());
                tile.replace_texture = None;
                changed = true;
            }
        }
    } else {
        state.paint = false;
        state.confirm = None;
        state.stroke.clear();
    }
    state.showing = view.live.on;
    state.dirty = false;
    changed
}
fn at(view: &AreaView, cell: Cell) -> Option<&AreaTile> {
    let model = view.model.as_ref()?;
    if u32::from(cell.0) >= model.width || u32::from(cell.1) >= model.height {
        return None;
    }
    model.tiles.get(usize::from(cell.1) * model.width as usize + usize::from(cell.0))
}
pub(crate) fn select(view: &mut AreaView, cell: Cell) {
    let value = at(view, cell).and_then(Tile::shown);
    view.live_tiles.cell = Some(cell);
    view.tile_selection = vec![(cell.0.into(), cell.1.into())];
    if let Some(value) = value {
        view.live_tiles.draft = value;
    }
}
fn request(app: &mut Moonglow, view: &mut AreaView, cell: Cell, tile: Tile) {
    if at(view, cell).is_none()
        || view.terrain.as_ref().is_none_or(|t| {
            t.set.tiles.get(usize::from(tile.id)).is_none_or(|t| t.model.is_empty())
        })
    {
        return;
    }
    if view.live_tiles.sent.get(&cell).is_some_and(|(_, v)| *v == tile)
        || (!view.live_tiles.sent.contains_key(&cell)
            && view.live_tiles.observed.get(&cell) == Some(&tile))
    {
        return;
    }
    if !view.live_tiles.confirmed {
        view.live_tiles.confirm = Some((cell, tile));
        return;
    }
    if let Some(id) = crate::live_view::queue(app, view, tile.op(cell)) {
        view.live_tiles.sent.insert(cell, (id, tile));
        view.live_tiles.message = "Queued; waiting for the server tile event".into();
    }
}
/// Leave Live painting completely, including pending confirmation and palette choice.
pub(crate) fn stop(app: &mut Moonglow, view: &mut AreaView) {
    view.tile_mode = false;
    app.palette.selected = None;
    app.palette.tile_brush = None;
    let state = &mut view.live_tiles;
    state.paint = false;
    state.palette_brush = false;
    state.confirm = None;
    state.hover = None;
    state.cell = None;
    state.stroke.clear();
    view.tile_selection.clear();
}

pub(crate) fn input(
    app: &mut Moonglow,
    ui: &egui::Ui,
    view: &mut AreaView,
    response: &egui::Response,
) -> bool {
    let terrain = crate::terrain_mode::active(app, view).is_some();
    if !view.tile_mode && !terrain {
        return false;
    }
    if response.secondary_clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        stop(app, view);
        return true;
    }
    let mut leave = false;
    egui::Area::new(egui::Id::new(("live-paint-banner", view.area)))
        .order(egui::Order::Foreground)
        .fixed_pos(view.rect.left_top() + egui::vec2(10., 28.))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width((view.rect.width() - 260.).max(260.));
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(Color32::YELLOW, "Live: painting the running server");
                    leave = ui.button("Stop painting").clicked();
                    ui.weak("Q/E rotates; Esc leaves painting");
                });
            });
        });
    if leave {
        stop(app, view);
        return true;
    }
    if response.hovered() && !ui.ctx().egui_wants_keyboard_input() {
        for (cmd, step) in crate::area_view::turn_keys(view) {
            if ui.input(|i| app.keymap.pressed(i, cmd)) {
                let turn = if step > 0. { 1 } else { 3 };
                view.live_tiles.draft.orientation = (view.live_tiles.draft.orientation + turn) % 4;
            }
        }
    }
    let pointer = response
        .interact_pointer_pos()
        .and_then(|p| crate::tile_select::tile_at(view, p))
        .and_then(|(x, y)| Some((x.try_into().ok()?, y.try_into().ok()?)));
    view.live_tiles.hover = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|_| response.hovered())
        .and_then(|p| crate::tile_select::tile_at(view, p))
        .and_then(|(x, y)| Some((x.try_into().ok()?, y.try_into().ok()?)));
    if !ui.input(|i| i.pointer.primary_down()) {
        view.live_tiles.stroke.clear();
    }
    if view.live_tiles.confirm.is_none()
        && (response.clicked()
            || (response.dragged_by(egui::PointerButton::Primary) && view.live_tiles.paint))
        && let Some(cell) = pointer
    {
        if view.live_tiles.paint {
            if view.live_tiles.stroke.insert(cell) {
                let mut tile = view.live_tiles.draft;
                if view.live_tiles.palette_brush {
                    tile.height = at(view, cell).and_then(Tile::shown).map_or(0, |t| t.height);
                }
                request(app, view, cell, tile);
            }
        } else {
            select(view, cell);
        }
    }
    if !(view.live_tiles.paint
        && view.live_tiles.confirm.is_none()
        && view.live_tiles.palette_brush)
    {
        window(app, ui, view, terrain);
    }
    true
}
fn window(app: &mut Moonglow, ui: &egui::Ui, view: &mut AreaView, terrain: bool) {
    let mut apply = false;
    let mut restore = false;
    let mut confirm = false;
    let mut cancel = false;
    let mut open = true;
    egui::Window::new("Live Tile Properties")
        .id(egui::Id::new(("live-tiles", view.area)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(view.rect.left_top() + egui::vec2(12., 86.))
        .show(ui.ctx(), |ui| {
            ui.set_width(390.);
            ui.colored_label(Color32::YELLOW, "Live: painting the running server");
            ui.label("Single-tile replacement; the module stays unchanged.");
            ui.weak("Shows changes observed here; earlier server terrain is unknown.");
            ui.label("Pick a tile in the Tiles palette, then click or drag in the area.");
            if terrain {
                ui.label("Terrain transitions are unavailable in Live. Select a tile below.");
            }
            let s = &mut view.live_tiles;
            if let Some((x, y)) = s.cell {
                ui.label(format!("Tile {x}, {y} (grid coordinates)"));
                ui.add_enabled_ui(s.confirm.is_none(), |ui| {
                    egui::CollapsingHeader::new("Advanced").show(ui, |ui| {
                    egui::Grid::new("live-tile-fields").show(ui, |ui| {
                        let label = ui.label("Tile ID");
                        ui.add(egui::DragValue::new(&mut s.draft.id).range(0..=u16::MAX))
                            .labelled_by(label.id);
                        ui.end_row();
                        let label = ui.label("Orientation");
                        ui.add(egui::DragValue::new(&mut s.draft.orientation).range(0..=3))
                            .labelled_by(label.id);
                        ui.label("quarter turns CCW");
                        ui.end_row();
                        let label = ui.label("Tile height");
                        ui.add(egui::DragValue::new(&mut s.draft.height).range(0..=255))
                            .labelled_by(label.id);
                        ui.label("native tile units");
                        ui.end_row();
                    });
                    let model = view.terrain.as_ref()
                        .and_then(|t| t.set.tiles.get(usize::from(s.draft.id)))
                        .filter(|t| !t.model.is_empty()).map(|t| t.model.as_str());
                    ui.label(model.unwrap_or("Tile ID is absent from this tileset"));
                    ui.add_enabled_ui(model.is_some(), |ui| {
                        apply = ui.button("Apply Live Tile")
                            .on_hover_text("Send these values for the selected tile; the first change asks for confirmation.")
                            .on_disabled_hover_text("Choose a tile ID available in this tileset.").clicked();
                        if ui.checkbox(&mut s.paint, "Paint single tiles").on_hover_text(
                            "Click or drag across cells to use these values. Right-click or Escape stops painting.").changed() {
                            s.palette_brush = false;
                        }
                    });
                    });
                    restore = ui.add_enabled(s.before.contains_key(&(x,y)),
                        egui::Button::new("Restore First Observed Tile"))
                        .on_disabled_hover_text("No earlier tile was observed here in this Live session.")
                        .on_hover_text("Restore the first server tile observed here, not necessarily the module tile.").clicked();
                });
            } else {
                ui.label("Click a tile in the area to select it.");
            }
            if !s.message.is_empty() { ui.label(&s.message); }
            if let Some((cell, t)) = s.confirm {
                ui.separator();
                ui.strong("Confirm live tile painting");
                ui.label(format!("Area {}: tile {}, {} -> ID {}, orientation {}, height {}",
                    view.area, cell.0, cell.1, t.id, t.orientation, t.height));
                ui.label("Changes affect the running server. Neighbouring tiles and doors are not adjusted.");
                ui.horizontal(|ui| {
                    confirm = ui.button("Confirm Live Painting").clicked();
                    cancel = ui.button("Cancel Tile Change").clicked();
                });
            }
        });
    if !open {
        stop(app, view);
        return;
    }
    if cancel {
        view.live_tiles.confirm = None;
        view.live_tiles.paint = false;
    }
    if confirm && let Some((cell, t)) = view.live_tiles.confirm.take() {
        view.live_tiles.confirmed = true;
        request(app, view, cell, t);
    }
    if let Some(cell) = view.live_tiles.cell {
        if apply {
            let t = view.live_tiles.draft;
            request(app, view, cell, t);
        }
        if restore && let Some(t) = view.live_tiles.before.get(&cell).copied() {
            request(app, view, cell, t);
        }
    }
}

/// Live exposes native single-cell replacements, never the authoring transition brush.
pub(crate) fn palette(app: &mut Moonglow, ui: &mut egui::Ui, area: mg_core::ResRef) {
    let Some(view) = app.area_views.get_mut(&area) else { return };
    let Some(tools) = &view.terrain else { return };
    let set = tools.set.clone();
    ui.colored_label(Color32::YELLOW, "Live: painting the running server");
    ui.label("Single tiles · Q/E rotates · click/drag to paint");
    ui.weak("Keeps each cell's height. No terrain transitions.");
    ui.add(egui::TextEdit::singleline(&mut view.live_tiles.search).hint_text("Find tile model"));
    let search = view.live_tiles.search.to_ascii_lowercase();
    let current = view.live_tiles.paint.then_some(view.live_tiles.draft.id);
    let orientation = view.live_tiles.draft.orientation;
    let tiles: Vec<_> = set
        .tiles
        .iter()
        .enumerate()
        .filter(|(id, t)| {
            *id <= u16::MAX as usize
                && !t.model.is_empty()
                && t.model.to_ascii_lowercase().contains(&search)
        })
        .collect();
    let columns = (ui.available_width() / 98.).floor().max(1.) as usize;
    let mut chosen = None;
    egui::ScrollArea::vertical().show_rows(ui, 112., tiles.len().div_ceil(columns), |ui, rows| {
        for row in rows {
            ui.horizontal(|ui| {
                for &(id, tile) in tiles.iter().skip(row * columns).take(columns) {
                    let placement = mg_tiles::Placement { tile: id as u32, orientation, height: 0 };
                    if crate::tile_select::variant_picture(
                        app,
                        ui,
                        placement,
                        &tile.model,
                        current == Some(id as u16),
                    ) {
                        chosen = Some(id as u16);
                    }
                }
            });
        }
    });
    if let Some(id) = chosen {
        let view = app.area_views.get_mut(&area).expect("open area");
        view.live_tiles.draft.id = id;
        view.live_tiles.paint = true;
        view.live_tiles.palette_brush = true;
        view.live_tiles.confirm = None;
        view.tile_mode = true;
        app.palette.selected = None;
        app.palette.tile_brush = None;
    }
}

pub(crate) fn preview(view: &AreaView) -> Option<crate::terrain_mode::Preview> {
    let s = &view.live_tiles;
    if !s.paint || !view.tile_mode || s.confirm.is_some() {
        return None;
    }
    let cell = s.hover?;
    let mut tile = at(view, cell)?.clone();
    let def = view.terrain.as_ref()?.set.tiles.get(usize::from(s.draft.id))?;
    tile.id = i64::from(s.draft.id);
    tile.orientation = s.draft.orientation;
    tile.model = Some(def.model.to_ascii_lowercase());
    tile.replace_texture = None;
    if !s.palette_brush {
        tile.height = s.draft.height.into();
        tile.position.z = f32::from(s.draft.height) * view.model.as_ref()?.height_step;
    }
    let index = usize::from(cell.1) * view.model.as_ref()?.width as usize + usize::from(cell.0);
    Some((vec![tile], vec![index]))
}
pub(crate) fn paint(ui: &egui::Ui, view: &AreaView) {
    if !view.live.on {
        return;
    }
    let Some(cell) = view.live_tiles.cell else { return };
    let Some(t) = at(view, cell) else { return };
    let corners = [(-5., -5.), (5., -5.), (5., 5.), (-5., 5.), (-5., -5.)];
    let points: Vec<_> = corners
        .into_iter()
        .filter_map(|(x, y)| view.screen_pos(t.position + glam::vec3(x, y, 0.05)))
        .collect();
    if points.len() == 5 {
        ui.painter_at(view.rect).add(egui::Shape::line(points, Stroke::new(2., Color32::YELLOW)));
    }
}
