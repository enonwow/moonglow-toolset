//! Live › Live Chat: what is said in the running game, in a window of its
//! own (the DM's most used view), and saying something back: as the selected
//! NPC, to the selected player, or to every player. The lines are the Live
//! Log's chat ([`crate::live_log`]), in its colours, without the AI's shouts.

use mg_core::ResRef;
use mg_live::events::{Event, Query};
use mg_live::store::{Command, MessageMode, Op, SpeechVolume};

use crate::Moonglow;
use crate::live_event_people::{Face, People};
use crate::live_log::{Speaker, ai_shout, line};

/// Chat lines with who says them and the speaker's reported portrait and name.
type Lines = Vec<(Event, String, Option<Speaker>, Option<Face>)>;

/// Who a message goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum To {
    /// Said by the selected NPC, aloud.
    #[default]
    Npc,
    /// To the selected player only.
    Player,
    /// To every player the server reports.
    AllPlayers,
}

#[derive(Debug, Clone, Default)]
pub struct LiveChat {
    pub to: To,
    pub volume: SpeechVolume,
    pub text: String,
    pub notice: Option<String>,
    pub area: Option<String>,
    pub to_dm: bool,
    pub dm_target: Option<String>,
    pub windows: std::collections::HashMap<ResRef, LiveChat>,
    /// The lines last shown and the newest event they were read up to.
    cache: Option<((String, u64), Lines)>,
}

#[cfg(feature = "measure")]
impl LiveChat {
    pub(crate) fn measure_cache(&self) -> serde_json::Value {
        serde_json::json!({"rows":self.cache.as_ref().map_or(0,|(_,rows)|rows.len()),
            "detached":self.windows.iter().map(|(area,state)|(area.to_string(),state.measure_cache())).collect::<std::collections::BTreeMap<_,_>>()})
    }
}

/// The selected live object of the shown area: its area and id.
fn selected(app: &Moonglow) -> Option<(String, String)> {
    let area = app.palette.area.filter(|a| app.area_views.contains_key(a))?;
    let id = app.area_views[&area].live.selected.clone()?;
    Some((area.to_string(), id))
}

/// Cheap composer prerequisites; the queue still validates each command.
fn recipient_error(app: &Moonglow, to: To, area: Option<&str>) -> Option<&'static str> {
    let Some(store) = &app.live else { return Some("Connect to a gateway to send messages.") };
    if to != To::Npc && !store.policy().players {
        return Some("Enable player operations in Options > Live to message players.");
    }
    if to == To::AllPlayers {
        return None;
    }
    let Some((shown, id)) = selected(app) else {
        return Some("Select a creature on the Live map.");
    };
    if area.is_some_and(|area| area != shown) {
        return Some("Select a recipient on this area’s map.");
    }
    let Some((_, kind, o)) = store.locate(&id) else {
        return Some("The selection is no longer reported.");
    };
    if kind != mg_area::ObjectKind::Creature || o.dm || o.possessed {
        return Some("Select an NPC or player, not a world object or DM.");
    }
    match (to, o.pc) {
        (To::Npc, true) => Some("Select an NPC, or choose To the player."),
        (To::Player, false) => Some("Select a player, or choose As the NPC."),
        _ => None,
    }
}

/// The commands a message makes, or why it cannot be sent.
fn orders(
    app: &Moonglow,
    to: To,
    volume: SpeechVolume,
    text: &str,
    scope: Option<&str>,
) -> Result<Vec<Command>, String> {
    if let Some(reason) = recipient_error(app, to, scope) {
        return Err(reason.into());
    }
    let store = app.live.as_ref().ok_or("Not connected: Live › Connect to Gateway.")?;
    let command = |area: String, op| Command { id: String::new(), area, op };
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Nothing to say.".into());
    }
    match to {
        To::Npc | To::Player => {
            let (_, id) = selected(app).ok_or("Select a creature in the area's Live view.")?;
            target_order(store, &id, volume, &text).map(|c| vec![c])
        }
        To::AllPlayers => {
            let status = store.status(crate::live_view::now_ms());
            let mut out = Vec::new();
            for area in
                status["areas"].as_array().into_iter().flatten().filter_map(|a| a["area"].as_str())
            {
                if scope.is_some_and(|s| s != area) {
                    continue;
                }
                for o in store.area(area).map(|a| a.creatures).unwrap_or_default() {
                    if o.pc && !o.dm {
                        let op = Op::PcMessage {
                            target: o.id,
                            text: text.clone(),
                            mode: MessageMode::Chat,
                        };
                        out.push(command(area.to_string(), op));
                    }
                }
            }
            if out.is_empty() { Err("No player is reported.".into()) } else { Ok(out) }
        }
    }
}

/// Shared explicit recipient path for Live Chat and Focus; player policy stays enforced.
pub(crate) fn target_order(
    store: &mg_live::store::LiveStore,
    id: &str,
    volume: SpeechVolume,
    text: &str,
) -> Result<Command, String> {
    let (area, kind, o) = store.locate(id).ok_or("The recipient is no longer reported.")?;
    if kind != mg_area::ObjectKind::Creature || o.dm || o.possessed {
        return Err("Choose an NPC or player, not a world object or DM.".into());
    }
    if o.pc && !store.policy().players {
        return Err("Enable player operations in Options > Live to message players.".into());
    }
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("Write a message first.".into());
    }
    let target = id.to_string();
    let op = if o.pc {
        Op::PcMessage { target, text, mode: MessageMode::Chat }
    } else {
        Op::NpcSay { target, text, volume }
    };
    Ok(Command { id: String::new(), area, op })
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui, pinned: Option<ResRef>) {
    let mut state = if let Some(area) = pinned {
        app.live_chat.windows.remove(&area).unwrap_or_default()
    } else {
        std::mem::take(&mut app.live_chat)
    };
    if let Some(area) = pinned {
        state.area = Some(area.to_string());
    }
    body(app, ui, &mut state, pinned);
    if let Some(area) = pinned {
        app.live_chat.windows.insert(area, state);
    } else {
        app.live_chat = state;
    }
}
fn body(app: &mut Moonglow, ui: &mut egui::Ui, state: &mut LiveChat, pinned: Option<ResRef>) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("window.chat");
    ui.heading("Live Chat");
    ui.separator();
    let Some(store) = app.live.clone() else {
        ui.label("Not connected: Live › Connect to Gateway.");
        return;
    };
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.to_dm, false, "Conversation");
        ui.selectable_value(&mut state.to_dm, true, "To DM");
    });
    let last = store.event_cursor();
    let stamp = (store.boot(), last);
    if state.cache.as_ref().is_some_and(|c| c.0.0 != stamp.0) {
        state.dm_target = None;
    }
    if state.cache.as_ref().is_none_or(|c| c.0 != stamp) {
        let query = Query { kinds: vec!["chat".into()], limit: 500, ..Default::default() };
        let people = People::new(&store);
        let who = |id: &str| people.name(id);
        let lines = store
            .events(&query)
            .0
            .into_iter()
            .filter(|e| !ai_shout(e))
            .map(|e| {
                let speaker = people.speaker(&e);
                let face = e.id.as_deref().and_then(|id| people.face(id));
                {
                    let text = format!("{}  {}", crate::live_log::clock(e.at_ms), line(&e, &who));
                    (e, text, speaker, face)
                }
            })
            .collect();
        state.cache = Some((stamp, lines));
    }
    let choices = crate::live_areas::choices(
        app,
        state
            .cache
            .as_ref()
            .unwrap()
            .1
            .iter()
            .map(|r| r.0.area.as_str())
            .chain(state.area.as_deref()),
    );
    if let Some(area) = pinned {
        let name = choices
            .iter()
            .find(|a| a.id == area.to_string())
            .map(|a| a.name.as_str())
            .unwrap_or("Unknown area");
        ui.strong(format!("Area: {name}"));
        ui.weak("This window stays on this area. The main Chat window has all areas.");
    } else if let Some(id) = crate::live_areas::tabs(ui, &mut state.area, &choices, true, true)
        && let Ok(area) = id.parse()
    {
        app.actions.push(crate::Action::OpenTab(crate::Tab::LiveChatArea(area)));
    }
    let colors = app.live_log.colors;
    let footer =
        100.0 + crate::live_drafts::reserved(app, state.area.as_deref(), None, state.to_dm);
    // (Put back after drawing: a portrait reads pictures through the app.)
    let cache = state.cache.take();
    let lines: Vec<_> = cache
        .as_ref()
        .expect("filled above")
        .1
        .iter()
        .filter(|r| {
            state.area.as_ref().is_none_or(|a| &r.0.area == a) && (!state.to_dm || to_dm(&r.0, r.2))
        })
        .collect();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .max_height((ui.available_height() - footer).max(60.))
        .show(ui, |ui| {
            if lines.is_empty() {
                ui.label("No conversation yet.");
                ui.weak("Speech from the game appears here. Select an NPC to speak as them, or choose a player recipient below.");
            }
            // One portrait beside each run of lines by the same speaker.
            for run in lines.chunk_by(|a, b| a.3 == b.3 && a.0.area==b.0.area) {
                ui.horizontal_top(|ui| {
                    if let Some(face) = &run[0].3 {
                        crate::live_selection::portrait(app, ui, face.base.as_deref(), face.appearance, face.gender, &face.name);
                    }
                    ui.vertical(|ui| {
                        for (event, text, speaker, _) in run {
                            let text = if state.area.is_none() { format!("[{}] {text}", choices.iter().find(|a|a.id==event.area).map(|a|a.name.as_str()).unwrap_or("Server")) } else {text.clone()};
                            let mut row = egui::RichText::new(crate::live_log::attributed(&text,event));
                            if let Some(c) = speaker.and_then(|s| colors[s as usize]) {
                                row = row.color(c);
                            }
                            ui.label(row);
                            if state.to_dm && let Some(id)=&event.id {
                                let player=store.locate(id).is_some_and(|(_,_,o)|o.pc&&!o.dm&&!o.possessed);
                                let who=if event.name.is_empty() {run[0].3.as_ref().map(|f|f.name.as_str()).unwrap_or(id)}else{&event.name};
                                if player && ui.small_button(format!("Reply to {who}")).on_hover_text("Select this player for a private DM reply.").clicked() {state.dm_target=Some(id.clone());}
                            }
                        }
                    });
                });
            }
        });
    state.cache = cache;
    crate::live_drafts::ui(app, ui, state.area.as_deref(), None, state.to_dm);
    ui.separator();
    if state.to_dm {
        dm_composer(app, ui, state);
        return;
    }
    let chat = &mut *state;
    let mut send = false;
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut chat.to, To::Npc, "As the NPC");
        ui.selectable_value(&mut chat.to, To::Player, "To the player");
        ui.selectable_value(&mut chat.to, To::AllPlayers, "To all players");
        if chat.to == To::Npc {
            ui.separator();
            ui.selectable_value(&mut chat.volume, SpeechVolume::Talk, "Talk");
            ui.selectable_value(&mut chat.volume, SpeechVolume::Whisper, "Whisper");
            ui.selectable_value(&mut chat.volume, SpeechVolume::Shout, "Shout");
        }
    });
    let reason = recipient_error(app, state.to, state.area.as_deref());
    let chat = &mut *state;
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut chat.text)
                .hint_text("Message (Enter sends)")
                .desired_width(ui.available_width() - 60.),
        );
        let reason =
            reason.or_else(|| chat.text.trim().is_empty().then_some("Write a message first."));
        let ready = reason.is_none();
        send = ui
            .add_enabled(ready, egui::Button::new("Send"))
            .on_disabled_hover_text(reason.unwrap_or_default())
            .on_hover_text("Send this message. Enter also sends.")
            .clicked()
            || (ready && field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    });
    if let Some(reason) = reason {
        ui.weak(reason);
    }
    if let Some(note) = &chat.notice {
        ui.weak(note);
    }
    if send {
        let (to, volume, text) = (chat.to, chat.volume, chat.text.clone());
        let result = orders(app, to, volume, &text, state.area.as_deref()).and_then(|commands| {
            for command in commands {
                crate::live_scene::enqueue(app, command).map_err(|e| e.to_string())?;
            }
            Ok(())
        });
        let chat = &mut *state;
        match result {
            Ok(()) => {
                chat.text.clear();
                chat.notice = None;
            }
            Err(e) => chat.notice = Some(e),
        }
    }
}

fn to_dm(event: &Event, speaker: Option<Speaker>) -> bool {
    !ai_shout(event)
        && (event.data["to_dm"] == true
            || event.data["audience"] == "dm"
            || event.data["channel"] == 30
            || (event.data["channel"] == 14 && speaker == Some(Speaker::Player)))
}
fn dm_composer(app: &mut Moonglow, ui: &mut egui::Ui, state: &mut LiveChat) {
    let Some(store) = app.live.clone() else { return };
    if state.dm_target.is_none() {
        state.dm_target = selected(app)
            .map(|(_, id)| id)
            .filter(|id| store.locate(id).is_some_and(|(_, _, o)| o.pc && !o.dm));
    }
    let recipient = state.dm_target.as_deref().and_then(|id| store.locate(id));
    ui.strong(format!(
        "DM reply to {}",
        recipient
            .as_ref()
            .map(|(_, _, o)| crate::live_panel::name(o))
            .unwrap_or("no player selected")
    ));
    ui.weak("Private DM-prefixed server feedback; not an avatar chat message.");
    let reason = if !store.policy().players {
        Some("Enable player operations in Live Options.")
    } else if recipient.as_ref().is_none_or(|(_, _, o)| !o.pc || o.dm || o.possessed) {
        Some("Select a player's To DM message to reply.")
    } else if state
        .area
        .as_ref()
        .is_some_and(|a| recipient.as_ref().is_some_and(|(r, _, _)| r != a))
    {
        Some("The recipient is in another area. Choose All or their current area.")
    } else {
        None
    };
    let ready = reason.is_none() && !state.text.trim().is_empty();
    let mut send = false;
    ui.horizontal(|ui| {
        let input = ui.add(
            egui::TextEdit::singleline(&mut state.text)
                .hint_text("DM reply (Enter sends)")
                .desired_width((ui.available_width() - 100.).max(40.)),
        );
        send = ui
            .add_enabled(ready, egui::Button::new("Send DM reply"))
            .on_disabled_hover_text(reason.unwrap_or("Write a message first."))
            .clicked()
            || (ready && input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    });
    if let Some(reason) = reason {
        ui.weak(reason);
    }
    if send && let Some((area, _, o)) = recipient {
        match crate::live_scene::enqueue(
            app,
            Command {
                id: String::new(),
                area,
                op: Op::PcMessage {
                    target: o.id,
                    text: state.text.trim().into(),
                    mode: MessageMode::Dm,
                },
            },
        ) {
            Ok(_) => {
                state.text.clear();
                state.notice =
                    Some("DM reply queued. See Live Log for the acknowledgement.".into());
            }
            Err(e) => state.notice = Some(e.to_string()),
        }
    }
    if let Some(note) = &state.notice {
        ui.weak(note);
    }
}
