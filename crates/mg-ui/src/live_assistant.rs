//! Saved DM guidance, NPC conversation history and object notes.
use crate::{Action, Moonglow, Tab};
use mg_live::store::{
    LiveStore,
    assistant::{
        cards::{GmCard, NpcCard},
        memory::Conversation,
    },
};
use std::{collections::HashMap, path::PathBuf, sync::Weak};

#[derive(Debug, Default)]
struct Editor {
    gm: GmCard,
    npc: NpcCard,
    loaded: bool,
    dirty: bool,
    notice: Option<String>,
}
#[derive(Debug)]
pub struct LiveAssistant {
    pub data_dir: Option<PathBuf>,
    context: Option<(PathBuf, String)>,
    source: Weak<LiveStore>,
    pub notice: Option<String>,
    editors: HashMap<(PathBuf, String), Editor>,
    memory: Option<(String, (u64, u64), Vec<Conversation>)>,
    memory_area: Option<String>,
}
impl Default for LiveAssistant {
    fn default() -> Self {
        Self {
            data_dir: crate::recovery::data_dir(),
            context: None,
            source: Weak::new(),
            notice: None,
            editors: HashMap::new(),
            memory: None,
            memory_area: None,
        }
    }
}
#[cfg(feature = "measure")]
impl LiveAssistant {
    pub(crate) fn measure_cache(&self) -> serde_json::Value {
        serde_json::json!({"editors":self.editors.len(),"memory_pairs":self.memory.as_ref().map_or(0,|(_,_,rows)|rows.len())})
    }
}
pub(crate) fn update(app: &mut Moonglow) {
    if app.live_backend().shared_reason().is_some() {
        return;
    }
    let module = app.module_path();
    let Some(store) = app.live.clone() else { return };
    let config = app.live_backend().config();
    let server = format!("{}:{}:{}", config.redis_host, config.redis_port, config.prefix);
    let context = module.map(|m| (m, server));
    let same =
        app.live_assistant.source.upgrade().is_some_and(|old| std::sync::Arc::ptr_eq(&old, &store));
    if same && app.live_assistant.context == context {
        return;
    }
    app.live_assistant.source = std::sync::Arc::downgrade(&store);
    app.live_assistant.context = context.clone();
    app.live_assistant.memory = None;
    app.live_assistant.memory_area = None;
    app.live_assistant.notice = match (context, app.live_assistant.data_dir.clone()) {
        (Some((module, server)), Some(data)) => store.assistant_context(data, module, server).err(),
        _ => {
            store.clear_assistant_context();
            Some("Save and open a module to keep assistant cards, notes and memory.".into())
        }
    };
}
fn field(ui: &mut egui::Ui, label: &str, text: &mut String, rows: usize) -> bool {
    let label = ui.label(label);
    ui.add(
        egui::TextEdit::multiline(text)
            .char_limit(4000)
            .desired_rows(rows)
            .desired_width(f32::INFINITY),
    )
    .labelled_by(label.id)
    .changed()
}
pub(crate) fn card(app: &mut Moonglow, ui: &mut egui::Ui, target: Option<&str>) {
    if app.live_backend().is_gateway() {
        crate::live_book::ui(
            app,
            ui,
            target.map_or(crate::live_book::Target::Gm, |id| {
                crate::live_book::Target::Npc(id.into())
            }),
        );
        return;
    }
    ui.heading(if target.is_some() { "NPC card" } else { "GM card" });
    ui.weak(if target.is_some(){"Tell the assistant who this NPC is and how to speak as them. Saved with this module."}else{"Private campaign guidance. The assistant reads this first; it must not quote secrets to players."});
    let Some(store) = app.live.clone() else {
        ui.label("Start Live and open a module first.");
        return;
    };
    let Some(module) = app.module_path() else {
        ui.label("Save and open a module first.");
        return;
    };
    if let Some(error) = &app.live_assistant.notice {
        ui.colored_label(ui.visuals().error_fg_color, error);
        return;
    }
    let key = (module, target.unwrap_or("@gm").into());
    let state = app.live_assistant.editors.entry(key).or_default();
    if !state.loaded {
        let loaded = if let Some(target) = target {
            store.npc_card(target).map(|c| state.npc = c)
        } else {
            store.gm_card().map(|c| state.gm = c)
        };
        state.notice = loaded.err();
        state.loaded = state.notice.is_none();
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(state.loaded && state.dirty, egui::Button::new("Save card"))
            .on_disabled_hover_text("Change a field to save it.")
            .clicked()
        {
            let result = if let Some(target) = target {
                store.save_npc_card(target, state.npc.clone())
            } else {
                store.save_gm_card(state.gm.clone())
            };
            match result {
                Ok(()) => {
                    state.dirty = false;
                    state.notice = Some("Card saved.".into());
                }
                Err(e) => state.notice = Some(e),
            }
        }
        if ui
            .button("Reload card")
            .on_hover_text("Discard this editor's unsaved changes and reload the saved card.")
            .clicked()
        {
            state.loaded = false;
            state.dirty = false;
        }
        if state.dirty {
            ui.weak("Unsaved changes");
        }
    });
    if let Some(message) = &state.notice {
        ui.label(message);
    }
    if !state.loaded {
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        if target.is_some() {
            ui.small(format!("Card: {}", state.npc.id));
            for (label, value) in [
                ("Name", &mut state.npc.name),
                ("Who they are", &mut state.npc.who),
                ("Personality", &mut state.npc.personality),
                ("What they know", &mut state.npc.knows),
                ("What they hide", &mut state.npc.hides),
                ("How they speak", &mut state.npc.speech),
                ("Language", &mut state.npc.language),
            ] {
                state.dirty |= field(ui, label, value, 2);
            }
        } else {
            for (label, value) in [
                ("Campaign tone", &mut state.gm.campaign_tone),
                ("House rules", &mut state.gm.house_rules),
                ("Never reveal to players", &mut state.gm.never_reveal),
                ("How to answer on the DM channel", &mut state.gm.dm_replies),
                ("Narration style", &mut state.gm.narration),
                ("Language", &mut state.gm.language),
            ] {
                state.dirty |= field(ui, label, value, 2);
            }
        }
    });
}
pub(crate) fn npc_button(app: &mut Moonglow, ui: &mut egui::Ui, id: &str) {
    let allowed = app.live.as_ref().is_some_and(|s| {
        s.locate(id).is_some_and(|(_, k, o)| {
            k == mg_area::ObjectKind::Creature && !o.pc && !o.dm && !o.possessed
        })
    });
    if allowed
        && ui
            .add_enabled(true, egui::Button::new("NPC card").small())
            .on_disabled_hover_text(crate::live_backend::SHARED_REASON)
            .on_hover_text(
                "Edit this NPC's identity, knowledge, secrets and voice for the assistant.",
            )
            .clicked()
    {
        open_npc_card(app, id);
    }
}
pub(crate) fn open_npc_card(app: &mut Moonglow, id: &str) {
    if app.live_backend().is_gateway() {
        app.actions.push(Action::OpenTab(Tab::LiveNpcCard(id.into())));
        return;
    }
    let Some(store) = &app.live else { return };
    match store.npc_card(id) {
        Ok(card) => app.actions.push(Action::OpenTab(Tab::LiveNpcCard(card.id))),
        Err(error) => app.live_assistant.notice = Some(error),
    }
}
pub(crate) fn memory(app: &mut Moonglow, ui: &mut egui::Ui, id: &str) {
    if app.live_backend().is_gateway() {
        ui.weak("Gateway conversation history is not available in this view yet.");
        return;
    }
    ui.weak("Saved NPC/player history. Nearby lines use reported positions; they do not prove who heard them.");
    let Some(store) = app.live.clone() else { return };
    let revision = store.assistant_revision();
    let state = &mut app.live_assistant;
    if state.memory.as_ref().is_none_or(|(old, r, _)| old != id || *r != revision) {
        match store.npc_memory(id, None, 50) {
            Ok(rows) => {
                state.memory = Some((id.into(), revision, rows));
                state.notice = None;
            }
            Err(e) => {
                ui.label(e);
                return;
            }
        }
    }
    if let Some(error) = store.assistant_error() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("History has not been saved: {error}"),
        );
    }
    let memory = state.memory.take().unwrap();
    let rows = &memory.2;
    let areas = crate::live_areas::choices(
        app,
        rows.iter().flat_map(|c| c.turns.iter().map(|t| t.area.as_str())),
    );
    crate::live_areas::tabs(ui, &mut app.live_assistant.memory_area, &areas, true, false);
    let area = app.live_assistant.memory_area.clone();
    if rows.is_empty() {
        ui.label("No saved conversation yet. Captured exchanges with players will appear here.");
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for c in rows {
            ui.collapsing(format!("{} ({} turns)", c.player_name, c.turns.len()), |ui| {
                if !c.summary.is_empty() {
                    ui.label(format!("Summary: {}", c.summary));
                }
                for t in c.turns.iter().filter(|t| area.as_ref().is_none_or(|a| a == &t.area)) {
                    let response = ui.label(format!(
                        "{}  {}: {}",
                        crate::live_log::clock(t.at_ms),
                        t.speaker,
                        t.text
                    ));
                    response.on_hover_text(format!("{} | {}", t.area, t.context));
                }
            });
        }
    });
    app.live_assistant.memory = Some(memory);
}
