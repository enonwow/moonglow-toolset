//! The Live selection is independent from the module editor's selection.
use crate::{Moonglow, area_view::AreaView};

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("window.selection");
    ui.heading("Selection");
    ui.separator();
    let Some(area) = app.palette.area else {
        ui.label("Open a Live area first.");
        return;
    };
    let Some(view) = app.area_views.get_mut(&area).filter(|v| v.live.on) else {
        ui.label("Turn on Live in the area view.");
        return;
    };
    let Some(store) = &app.live else { return };
    let (live, _) = view.live.report(store, area);
    let Some(live) = live else {
        ui.label("Waiting for the area report.");
        return;
    };
    let ids: Vec<_> = view.live.selection_ids().map(str::to_owned).collect();
    ui.strong(format!("{} selected", ids.len()));
    if ids.is_empty() {
        ui.label("Select objects on the Live map.");
        ui.weak("Click a marker or drag on empty ground. Shift adds; Ctrl+click toggles.");
        return;
    }
    ui.weak("Click a name to focus. Ctrl+1–5 stores; 1–5 recalls; double tap centres.");
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("live-selected-objects").striped(true).show(ui, |ui| {
            ui.strong("Name");
            ui.strong("HP");
            ui.strong("Kind");
            ui.end_row();
            for id in &ids {
                let Some((kind, o)) = live.find(id) else { continue };
                let clicked = ui
                    .push_id(id, |ui| {
                        ui.horizontal(|ui| {
                            let portrait = kind == mg_area::ObjectKind::Creature
                                && portrait(
                                    app,
                                    ui,
                                    o.portrait.as_deref(),
                                    o.app,
                                    crate::live_portrait::gender(o),
                                    crate::live_panel::name(o),
                                )
                                .clicked();
                            let clicked =
                                ui.button(crate::live_panel::name(o)).clicked() || portrait;
                            crate::live_assistant::npc_button(app, ui, &o.id);
                            clicked
                        })
                        .inner
                    })
                    .inner;
                if clicked {
                    if let Some(view) = app.area_views.get_mut(&area) {
                        focus(view, o.position());
                    }
                    app.live_panel.follow = None;
                }
                ui.label(o.hp.map_or_else(|| "—".into(), |[hp, max]| format!("{hp}/{max}")));
                ui.label(crate::live_panel::kind_name(kind, o));
                ui.end_row();
            }
        });
    });
    let mut note_id = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(egui::Id::new("selection-note")))
        .filter(|id| ids.contains(id))
        .unwrap_or_else(|| ids[0].clone());
    ui.separator();
    ui.strong("DM note");
    egui::ComboBox::from_id_salt("note-object")
        .selected_text(live.find(&note_id).map_or("Object", |(_, o)| crate::live_panel::name(o)))
        .show_ui(ui, |ui| {
            for id in &ids {
                if let Some((_, o)) = live.find(id) {
                    ui.selectable_value(&mut note_id, id.clone(), crate::live_panel::name(o));
                }
            }
        });
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("selection-note"), note_id.clone()));
    crate::live_notes::editor(app, ui, &note_id);
}

fn focus(view: &mut AreaView, position: glam::Vec3) {
    if let Some(orbit) = &mut view.orbit {
        orbit.target = position;
    }
}

/// Native GetPortraitResRef is already the complete base (often po_*).
/// Retain custom bases; an unset/missing portrait falls back to the reported
/// appearance (or humanoid race/gender), then a text placeholder.
/// Some stock defaults omit the underscore before the size suffix.
pub(crate) fn portrait(
    app: &mut Moonglow,
    ui: &mut egui::Ui,
    base: Option<&str>,
    appearance: Option<u16>,
    gender: Option<u8>,
    name: &str,
) -> egui::Response {
    let fallback = app
        .game
        .as_ref()
        .and_then(|game| crate::live_portrait::default_base(game, appearance, gender));
    let picture = app.loader().and_then(|mut loader| {
        crate::live_portrait::load(base, fallback.as_deref(), |name| loader.picture(ui.ctx(), name))
    });
    let size = egui::vec2(32., 50.);
    if let Some(picture) = picture {
        ui.add(
            egui::Image::new(&picture.texture)
                .uv(egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 100. / 128.)))
                .fit_to_exact_size(size)
                .maintain_aspect_ratio(false)
                .alt_text(format!("Portrait of {name}"))
                .sense(egui::Sense::click()),
        )
        .on_hover_text(format!("{name}\nReported portrait, appearance default, or the game’s neutral portrait when neither is set."))
    } else {
        ui.add_sized(size, egui::Button::new("?")).on_hover_text(format!(
            "No portrait image for the reported base ({}) or appearance default ({}).",
            base.unwrap_or("unset"),
            fallback.as_deref().unwrap_or("unset")
        ))
    }
}
