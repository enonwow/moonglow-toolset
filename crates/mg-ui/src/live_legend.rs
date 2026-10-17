//! Marker visibility belongs to each area view; the legend never covers its map.
use crate::{Moonglow, live_colors::Palette};
use egui::Color32;
use mg_area::ObjectKind;
pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("window.legend");
    ui.heading("Legend");
    crate::live_areas::active(app, ui);
    let Some(area) = app.palette.area else {
        ui.weak("Open an area to choose its marker visibility.");
        return;
    };
    let palette = Palette::for_app(app);
    let Some(view) = app.area_views.get_mut(&area) else { return };
    ui.weak(format!("Marker visibility for {area}. Other area views keep their own settings."));
    egui::ScrollArea::vertical().show(ui, |ui| {
        for label in [
            "Players",
            "DM / possessed",
            "NPCs",
            "Placeables",
            "Doors",
            "Items",
            "Dead",
            "Combat rings",
            "Attack lines",
            "Labels",
        ] {
            let color = match label {
                "Placeables" => crate::live_colors::world_color(ObjectKind::Placeable),
                "Doors" => crate::live_colors::world_color(ObjectKind::Door),
                "Items" => crate::live_colors::world_color(ObjectKind::Item),
                _ => Color32::WHITE,
            };
            view.live.markers.checkbox(ui, label, label, color);
        }
        ui.collapsing("NPC factions", |ui| {
            ui.weak("Creatures only");
            for (id, (name, color)) in palette.0.iter().enumerate() {
                view.live.markers.checkbox(
                    ui,
                    &format!("faction:{id}"),
                    &format!("{id}: {name}"),
                    *color,
                );
            }
        });
    });
}
