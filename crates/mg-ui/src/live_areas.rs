//! One area-tab pattern for Live histories and tools. History filters are local;
//! command tools share the active map, so their target and area stay together.
use crate::{Action, Moonglow, Tab};
use mg_core::ResRef;

#[derive(Clone)]
pub(crate) struct Area {
    pub id: String,
    pub name: String,
}
pub(crate) fn choices<'a>(app: &mut Moonglow, ids: impl IntoIterator<Item = &'a str>) -> Vec<Area> {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("live.area_choices");
    let mut ids: Vec<_> = ids.into_iter().filter(|s| !s.is_empty()).collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .map(|id| {
            let name = id
                .parse::<ResRef>()
                .ok()
                .and_then(|area| {
                    app.ws
                        .as_ref()
                        .map(|ws| app.area_names.info(ws, app.game.as_deref(), area).name)
                })
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| id.to_owned());
            Area { id: id.to_owned(), name }
        })
        .collect()
}
/// Returns the area requested as an independent window, if detaching is enabled.
pub(crate) fn tabs(
    ui: &mut egui::Ui,
    selected: &mut Option<String>,
    areas: &[Area],
    all: bool,
    detach: bool,
) -> Option<String> {
    let mut open = None;
    ui.push_id("live-area-tabs", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.weak("Area:");
            if all {
                ui.selectable_value(selected, None, "All");
            }
            for area in areas {
                let label = if areas.iter().any(|a| a.id != area.id && a.name == area.name) {
                    format!("{} ({})", area.name, area.id)
                } else {
                    area.name.clone()
                };
                let r = ui
                    .selectable_value(selected, Some(area.id.clone()), label)
                    .on_hover_text(&area.id);
                if detach {
                    r.context_menu(|ui| {
                        if ui.button("Open in new window").clicked() {
                            open = Some(area.id.clone());
                            ui.close();
                        }
                    });
                }
            }
        });
    });
    open
}
/// Open-map scope for command tools. It follows map focus, including cross-area follow.
pub(crate) fn active(app: &mut Moonglow, ui: &mut egui::Ui) {
    let ids = app
        .panes_tabs
        .iter()
        .flatten()
        .filter_map(|t| match t {
            Tab::Area(a) => Some(a.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let areas = choices(app, ids.iter().map(String::as_str));
    if areas.is_empty() {
        return;
    }
    let mut selected = app.palette.area.map(|a| a.to_string());
    let old = selected.clone();
    tabs(ui, &mut selected, &areas, false, false);
    if selected != old
        && let Some(area) = selected.and_then(|s| s.parse().ok())
    {
        app.palette.area = Some(area);
        app.actions.push(Action::OpenTab(Tab::Area(area)));
    }
    ui.separator();
}
