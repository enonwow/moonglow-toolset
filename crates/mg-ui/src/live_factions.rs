//! Session-only labels and native reputation commands in the Factions editor.
use crate::{
    Moonglow,
    live_targets::{selection, send},
};
use mg_live::store::Op;
use mg_module::factions::Factions;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveFactions {
    context: Option<(Option<PathBuf>, String)>,
    labels: BTreeMap<u32, String>,
    pub faction: u32,
    pub label: String,
    pub reputation: u8,
    pub notice: Option<String>,
    editing: Option<u32>,
    pending: Option<(String, String, u64)>,
}
impl LiveFactions {
    pub fn reset_context(&mut self, module: Option<PathBuf>, boot: String) {
        let context = Some((module, boot));
        if self.context != context {
            *self = Self { context, reputation: 50, faction: 1, ..Self::default() };
        }
    }
    /// Labels never change native names/IDs and expire with module path or boot.
    pub fn label_for(&self, faction: u32, native: &str) -> String {
        self.labels
            .get(&faction)
            .map_or_else(|| native.to_string(), |label| format!("{label} ({native})"))
    }
    fn poll(&mut self, app: &Moonglow) {
        let Some((id, description, started)) = &self.pending else { return };
        if let Some(result) = app.live.as_ref().and_then(|store| store.result(id)) {
            self.notice = Some(if result.ok {
                format!(
                    "{description}: {}",
                    result.value.map_or_else(|| "OK".into(), |v| v.to_string())
                )
            } else {
                format!(
                    "{description}: {} {}",
                    result.err,
                    result.value.map_or_else(String::new, |v| v.to_string())
                )
            });
            self.pending = None;
        } else if mg_live::now_ms().saturating_sub(*started) > 15000 {
            self.notice =
                Some(format!("{description}: no result received; check Live Log before retrying."));
            self.pending = None;
        }
    }
}
pub(crate) fn ui(
    app: &mut Moonglow,
    ui: &mut egui::Ui,
    state: &mut LiveFactions,
    factions: &Factions,
) {
    state
        .reset_context(app.module_path(), app.live.as_ref().map_or_else(String::new, |s| s.boot()));
    state.poll(app);
    ui.heading("Session factions");
    ui.label("Use factions already loaded by the server. Reserve spare factions in Basic before the session.");
    let server = app.live.as_ref().and_then(|store| store.factions());
    let rows = server.as_deref().unwrap_or_default();
    let available = rows.iter().any(|f| f.id == state.faction && f.id != 0);
    if server.is_none() {
        ui.weak("Server faction list unavailable. Update/restart the Live server scripts before using this pane.");
    }
    for (id, editor) in factions.factions.iter().enumerate().skip(1) {
        if !rows.iter().any(|native| native.id == id as u32 && native.name == editor.name) {
            ui.weak(format!(
                "#{} {}: not on the server: save, rebuild and restart the module to use it",
                id, editor.name
            ));
        }
    }
    egui::ComboBox::from_label("Acting faction")
        .selected_text(rows.iter().find(|f| f.id == state.faction).map_or_else(
            || format!("Unavailable #{}", state.faction),
            |f| state.label_for(f.id, &f.name),
        ))
        .show_ui(ui, |ui| {
            for f in rows.iter().filter(|f| f.id != 0) {
                let text = format!("#{}: {}", f.id, state.label_for(f.id, &f.name));
                ui.selectable_value(&mut state.faction, f.id, text);
            }
        });
    if state.editing != Some(state.faction) {
        state.label = state.labels.get(&state.faction).cloned().unwrap_or_default();
        state.editing = Some(state.faction);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Session label");
        ui.add(
            egui::TextEdit::singleline(&mut state.label)
                .char_limit(64)
                .hint_text("e.g. Dock ambushers"),
        );
        if ui
            .add_enabled(available, egui::Button::new("Save label"))
            .on_disabled_hover_text("Select a faction reported by the running server.")
            .clicked()
        {
            let label = state.label.trim();
            if label.is_empty() {
                state.labels.remove(&state.faction);
            } else {
                state.labels.insert(state.faction, label.into());
            }
            state.notice = Some(
                "Label saved in Moonglow for this server boot; native faction unchanged.".into(),
            );
        }
    });
    ui.weak("Labels last until the server restarts or the open module changes. Empty label restores the native name.");
    ui.separator();
    let selected = selection(app).and_then(|s| {
        if s.objects.len() != 1 || s.objects[0].0 != mg_area::ObjectKind::Creature {
            Err("Select exactly one live creature as the target.".into())
        } else {
            Ok(s)
        }
    });
    if let Err(error) = &selected {
        ui.label(error);
    }
    if let Ok(s) = &selected {
        let object = &s.objects[0].1;
        let faction = object.faction.map_or_else(|| "not reported".into(), |id| format!("#{id}"));
        ui.strong(format!("Target: {} (#{}), reported faction {faction}", object.name, object.id));
        ui.label("Reputation direction: acting faction regards the target. For global NPC factions this affects the faction; the reverse direction is separate.");
        ui.weak("Custom factions need a non-plot representative in a loaded area. Empty factions can be assigned first. Combat/temporary personal reputation can affect native readings.");
        let idle = state.pending.is_none() && available;
        let mut op = None;
        if ui
            .add_enabled(
                idle && !object.pc && !object.dm && !object.possessed,
                egui::Button::new("Assign faction to target"),
            )
            .clicked()
        {
            op = Some(Op::SetFaction {
                target: object.id.clone(),
                faction: mg_live::store::Faction::Id(state.faction),
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Reputation");
            ui.add(egui::DragValue::new(&mut state.reputation).range(0..=100));
            if ui.add_enabled(idle, egui::Button::new("Read live reputation")).clicked() {
                op = Some(Op::GetFactionReputation {
                    target: object.id.clone(),
                    faction: mg_live::store::Faction::Id(state.faction),
                });
            }
            if ui.add_enabled(idle, egui::Button::new("Set live reputation")).clicked() {
                op = Some(Op::SetFactionReputation {
                    target: object.id.clone(),
                    faction: mg_live::store::Faction::Id(state.faction),
                    reputation: state.reputation,
                });
            }
        });
        if let Some(op) = op {
            match send(app, s.area, op) {
                Ok(id) => {
                    state.pending = Some((
                        id,
                        format!("Faction #{} / {}", state.faction, object.name),
                        mg_live::now_ms(),
                    ));
                    state.notice = Some("Waiting for native result...".into());
                }
                Err(e) => state.notice = Some(e),
            }
        }
    }
    if state.pending.is_some() {
        ui.spinner();
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    if let Some(notice) = &state.notice {
        ui.label(notice);
    }
}
