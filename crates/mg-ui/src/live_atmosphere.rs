//! Commands for the running area's atmosphere; no module edits or inferred state.
use crate::{Moonglow, live_targets};
use mg_live::store::{AudioAction, DayNight, DayNightBoth, MusicSlot, Op, Weather};
use std::{collections::HashMap, sync::Arc};
#[derive(Clone)]
struct Draft {
    weather: Weather,
    all: bool,
    fog_period: DayNightBoth,
    fog: [u8; 3],
    amount: u8,
    fade: f32,
    light_period: DayNight,
    ambient: [u8; 3],
    diffuse: [u8; 3],
    skybox: u32,
    music_slot: MusicSlot,
    music: u32,
    sound_period: DayNight,
    sound: u32,
    volume: u8,
    pending: Option<String>,
    notice: Option<String>,
    readback: Option<String>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            weather: Weather::Default,
            all: false,
            fog_period: DayNightBoth::Both,
            fog: [128; 3],
            amount: 0,
            fade: 0.,
            light_period: DayNight::Day,
            ambient: [128; 3],
            diffuse: [255; 3],
            skybox: 0,
            music_slot: MusicSlot::Day,
            music: 0,
            sound_period: DayNight::Day,
            sound: 0,
            volume: 64,
            pending: None,
            notice: None,
            readback: None,
        }
    }
}
#[derive(Clone, Default)]
struct State {
    area: Option<String>,
    stamp: Option<(usize, String, u64)>,
    areas: Vec<String>,
    drafts: HashMap<String, Draft>,
    catalogue: Option<(String, Arc<Catalogue>)>,
}
type Catalogue = HashMap<&'static str, Vec<(u32, String)>>;
fn choice<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    egui::ComboBox::from_label(label)
        .selected_text(options.iter().find(|(v, _)| v == value).map_or("", |(_, s)| *s))
        .show_ui(ui, |ui| {
            for (v, s) in options {
                ui.selectable_value(value, *v, *s);
            }
        });
}
fn track(ui: &mut egui::Ui, label: &str, value: &mut u32, rows: &[(u32, String)]) {
    egui::ComboBox::from_label(label)
        .selected_text(
            rows.iter()
                .find(|(v, _)| v == value)
                .map_or_else(|| format!("Row {value}"), |(_, s)| s.clone()),
        )
        .show_ui(ui, |ui| {
            for (v, s) in rows {
                ui.selectable_value(value, *v, s);
            }
        });
    ui.collapsing(format!("Advanced {label}"), |ui| {
        ui.add(egui::DragValue::new(value).range(0..=65535));
    });
}
fn rgb(c: [u8; 3]) -> u32 {
    (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2])
}
pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    ui.heading("Atmosphere");
    ui.weak("Change the running server. Values below are proposed settings; Apply returns the server's settings. The module stays unchanged.");
    let Some(store) = app.live.clone().filter(|_| live_targets::enabled(app)) else {
        ui.label("Connect to a gateway to control an area's atmosphere.");
        return;
    };
    let id = egui::Id::new("live-atmosphere-state");
    let mut state = ui.ctx().data_mut(|d| d.get_temp::<State>(id)).unwrap_or_default();
    let (boot, seq) = store.revision();
    let stamp = (Arc::as_ptr(&store) as usize, boot, seq);
    if state.stamp.as_ref() != Some(&stamp) {
        if state.stamp.as_ref().is_some_and(|old| old.0 != stamp.0 || old.1 != stamp.1) {
            state.drafts.clear();
        }
        state.areas = store.status(mg_live::now_ms())["areas"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["area"].as_str().map(str::to_owned))
            .collect();
        state.stamp = Some(stamp);
    }
    let areas = crate::live_areas::choices(app, state.areas.iter().map(String::as_str));
    if state.area.as_ref().is_none_or(|a| !state.areas.contains(a)) {
        state.area = app
            .palette
            .area
            .map(|a| a.to_string())
            .filter(|a| state.areas.contains(a))
            .or_else(|| state.areas.first().cloned());
    }
    crate::live_areas::tabs(ui, &mut state.area, &areas, false, false);
    let cache_key = format!(
        "{:?}:{:?}:{:?}",
        app.module_path(),
        app.ws.as_ref().map(|w| w.revision()),
        app.game.as_ref().map(Arc::as_ptr)
    );
    if state.catalogue.as_ref().is_none_or(|(key, _)| *key != cache_key) {
        let rows = ["skyboxes", "ambientmusic", "ambientsound"]
            .into_iter()
            .map(|name| {
                let rows = app
                    .game
                    .as_ref()
                    .and_then(|g| g.table(name).ok())
                    .map(|t| {
                        (0..t.rows.len().min(65536))
                            .map(|i| {
                                (
                                    i as u32,
                                    format!(
                                        "{}: {}",
                                        i,
                                        t.get(i, "Label")
                                            .or_else(|| t.get(i, "Name"))
                                            .unwrap_or("unnamed")
                                    ),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (name, rows)
            })
            .collect();
        state.catalogue = Some((cache_key, Arc::new(rows)));
    }
    let Some(area) = state.area.clone() else {
        ui.label("Waiting for reported areas.");
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
        return;
    };
    let tables = &state.catalogue.as_ref().unwrap().1;
    let draft = state.drafts.entry(area.clone()).or_default();
    if let Some(id) = draft.pending.clone() {
        if let Some(result) = store.result(&id) {
            draft.notice = Some(if result.ok {
                "Server accepted atmosphere change".into()
            } else {
                format!("Server refused: {}", result.err)
            });
            draft.readback =
                result.value.as_ref().and_then(|v| serde_json::to_string_pretty(v).ok());
            draft.pending = None;
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
    let mut send = None;
    ui.add_enabled_ui(draft.pending.is_none(), |ui| {
        ui.collapsing("Weather", |ui| {
            choice(
                ui,
                "Weather type",
                &mut draft.weather,
                &[
                    (Weather::Default, "Area default"),
                    (Weather::Clear, "Clear"),
                    (Weather::Rain, "Rain"),
                    (Weather::Snow, "Snow"),
                ],
            );
            ui.checkbox(&mut draft.all, "All outdoor areas");
            ui.weak("Indoor areas may refuse weather changes.");
            if ui.button("Apply weather").clicked() {
                send = Some(Op::SetWeather { weather: draft.weather, all: draft.all });
            }
        });
        ui.collapsing("Fog", |ui| {
            choice(
                ui,
                "Fog period",
                &mut draft.fog_period,
                &[
                    (DayNightBoth::Both, "Day and night"),
                    (DayNightBoth::Day, "Day"),
                    (DayNightBoth::Night, "Night"),
                ],
            );
            ui.horizontal(|ui| {
                ui.label("Fog colour");
                ui.color_edit_button_srgb(&mut draft.fog);
            });
            ui.add(egui::Slider::new(&mut draft.amount, 0..=100).text("Fog amount"));
            ui.add(egui::Slider::new(&mut draft.fade, 0.0..=30.0).text("Fade seconds"));
            if ui.button("Apply fog").clicked() {
                send = Some(Op::SetFog {
                    period: draft.fog_period,
                    color: Some(rgb(draft.fog)),
                    amount: Some(draft.amount),
                    fade: draft.fade,
                });
            }
        });
        ui.collapsing("Lighting", |ui| {
            choice(
                ui,
                "Light period",
                &mut draft.light_period,
                &[(DayNight::Day, "Day"), (DayNight::Night, "Night")],
            );
            ui.horizontal(|ui| {
                ui.label("Ambient colour");
                ui.color_edit_button_srgb(&mut draft.ambient);
                ui.label("Diffuse colour");
                ui.color_edit_button_srgb(&mut draft.diffuse);
            });
            if ui.button("Apply light").clicked() {
                send = Some(Op::SetAreaLight {
                    period: draft.light_period,
                    ambient: Some(rgb(draft.ambient)),
                    diffuse: Some(rgb(draft.diffuse)),
                    fade: draft.fade,
                });
            }
        });
        ui.collapsing("Skybox", |ui| {
            track(ui, "Skybox row", &mut draft.skybox, &tables["skyboxes"]);
            if ui.button("Apply skybox").clicked() {
                send = Some(Op::SetSkybox { row: draft.skybox });
            }
        });
        ui.collapsing("Music", |ui| {
            choice(
                ui,
                "Music slot",
                &mut draft.music_slot,
                &[
                    (MusicSlot::Day, "Day"),
                    (MusicSlot::Night, "Night"),
                    (MusicSlot::Battle, "Battle"),
                ],
            );
            track(ui, "Music track", &mut draft.music, &tables["ambientmusic"]);
            ui.horizontal(|ui| {
                for (label, action, track) in [
                    ("Apply music", None, Some(draft.music)),
                    ("Play music", Some(AudioAction::Play), None),
                    ("Stop music", Some(AudioAction::Stop), None),
                ] {
                    if ui.button(label).clicked() {
                        send = Some(Op::SetMusic { slot: draft.music_slot, track, action });
                    }
                }
            });
        });
        ui.collapsing("Ambient sound", |ui| {
            choice(
                ui,
                "Ambient period",
                &mut draft.sound_period,
                &[(DayNight::Day, "Day"), (DayNight::Night, "Night")],
            );
            track(ui, "Ambient track", &mut draft.sound, &tables["ambientsound"]);
            ui.add(egui::Slider::new(&mut draft.volume, 0..=127).text("Ambient volume"));
            ui.horizontal(|ui| {
                for (label, action, apply) in [
                    ("Apply ambient", None, true),
                    ("Play ambient", Some(AudioAction::Play), false),
                    ("Stop ambient", Some(AudioAction::Stop), false),
                ] {
                    if ui.button(label).clicked() {
                        send = Some(Op::SetAmbientSound {
                            period: draft.sound_period,
                            track: apply.then_some(draft.sound),
                            volume: apply.then_some(draft.volume),
                            action,
                        });
                    }
                }
            });
        });
    });
    if let Some(op) = send {
        match area
            .parse()
            .map_err(|e| format!("{e}"))
            .and_then(|area| live_targets::send(app, area, op))
        {
            Ok(id) => {
                draft.pending = Some(id);
                draft.notice = Some("Waiting for the server...".into());
            }
            Err(e) => draft.notice = Some(e),
        }
    }
    if let Some(text) = &draft.notice {
        ui.label(text);
    }
    if let Some(text) = &draft.readback {
        ui.collapsing("Server settings from last reply", |ui| {
            ui.monospace(text);
        });
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, state));
}
