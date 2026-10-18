//! Live › Live Log: what happened in the running game, newest at the
//! bottom: chat, combat, players coming and going, the server log and the
//! game's answers to commands ([`mg_live::events`]). A page per subject,
//! a search, the shown area only, pause, and copy; a click on an event
//! selects its object in the area's view.

use mg_core::ResRef;
use mg_live::events::{Event, Query};
use serde_json::Value;

use crate::Moonglow;

/// The Live Log's pages: which kinds of event each shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    All,
    Chat,
    Combat,
    Players,
    Server,
    Commands,
}

impl Page {
    pub const ALL: [Page; 6] =
        [Page::All, Page::Chat, Page::Combat, Page::Players, Page::Server, Page::Commands];

    pub fn name(self) -> &'static str {
        match self {
            Page::All => "All",
            Page::Chat => "Chat",
            Page::Combat => "Combat",
            Page::Players => "Players",
            Page::Server => "Server",
            Page::Commands => "Commands",
        }
    }

    fn kinds(self) -> &'static [&'static str] {
        match self {
            Page::All => &[],
            Page::Chat => &["chat"],
            Page::Combat => &[
                "attack",
                "damage",
                "death",
                "spell",
                "vfx",
                "combat_start",
                "combat_end",
                "target",
                "healed",
            ],
            Page::Players => &["player_join", "player_leave", "entered", "left"],
            Page::Server => &["server_log"],
            Page::Commands => &["command"],
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveLog {
    pub page: Page,
    pub search: String,
    /// One area's events only (its resref); `None`: every area's.
    pub area: Option<String>,
    /// Paused: nothing newer than this event is shown.
    pub paused: Option<u64>,
    /// Cleared: nothing up to this event is shown (the store keeps them for agents).
    pub cleared: u64,
    /// The AI's silent shouts (NW_I_WAS_ATTACKED…) shown too; agents always get them.
    pub ai_shouts: bool,
    /// Chat colours by who speaks: player, DM, NPC, server (`None`: the usual text colour).
    pub colors: [Option<egui::Color32>; 4],
    /// The lines last shown, read again only when the store or the filters change.
    cache: Option<(Shown, Lines, Vec<String>)>,
}

#[cfg(feature = "measure")]
impl LiveLog {
    pub(crate) fn measure_cache(&self) -> serde_json::Value {
        serde_json::json!({"rows":self.cache.as_ref().map_or(0,|(_,rows,_)|rows.len())})
    }
}

impl Default for LiveLog {
    fn default() -> Self {
        // shortcut: colours last for the session, persist them in Settings when asked
        Self {
            page: Page::default(),
            search: String::new(),
            area: None,
            paused: None,
            cleared: 0,
            ai_shouts: false,
            colors: [
                Some(egui::Color32::from_rgb(110, 170, 255)),
                Some(egui::Color32::from_rgb(190, 140, 255)),
                None,
                Some(egui::Color32::from_rgb(230, 160, 70)),
            ],
            cache: None,
        }
    }
}

/// Who says a chat line, for its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Player,
    Dm,
    Npc,
    Server,
}

impl Speaker {
    pub(crate) fn object(o: &mg_live::store::LiveObject) -> Self {
        if o.dm || o.possessed {
            Self::Dm
        } else if o.pc {
            Self::Player
        } else {
            Self::Npc
        }
    }
    const ALL: [Speaker; 4] = [Speaker::Player, Speaker::Dm, Speaker::Npc, Speaker::Server];

    fn name(self) -> &'static str {
        match self {
            Speaker::Player => "Player",
            Speaker::Dm => "DM",
            Speaker::Npc => "NPC",
            Speaker::Server => "Server",
        }
    }
}

/// Events with their lines and, for chat, who speaks.
type Lines = Vec<(Event, String, Option<Speaker>)>;

/// Who says a chat line (`None`: not chat).
pub(crate) fn speaker(store: &mg_live::store::LiveStore, e: &Event) -> Option<Speaker> {
    (e.kind == "chat").then(|| match e.id.as_deref().and_then(|id| store.locate(id)) {
        Some((_, _, o)) => Speaker::object(&o),
        None if e.id.is_some() => Speaker::Npc,
        None => Speaker::Server,
    })
}

/// Whether an event is one of the AI's silent shouts (`NW_I_AM_DEAD`…), not speech.
pub fn ai_shout(e: &Event) -> bool {
    e.kind == "chat"
        && (e.data["silent"] == true
            || e.data["text"].as_str().is_some_and(|t| {
                t.starts_with("NW_")
                    && t.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            }))
}

/// What the cached lines were read for.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    page: Page,
    search: String,
    area: Option<String>,
    paused: Option<u64>,
    cleared: u64,
    ai_shouts: bool,
    last: u64,
}

/// An event's clock time, `HH:MM:SS` (UTC).
// shortcut: UTC, the standard library has no local time zone; add one with a time-zone crate.
pub(crate) fn clock(at_ms: u64) -> String {
    let s = at_ms / 1000 % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// What an attack's `result` number says (`docs/live-events-wire.md`).
fn attack_result(n: i64) -> &'static str {
    match n {
        1 | 7 => "hits",
        3 | 10 => "critically hits",
        2 => "is parried by",
        5 => "is resisted by",
        8 | 9 => "misses (concealment)",
        4 => "misses",
        _ => "attacks",
    }
}

/// An event as one line, `who(id)` naming an object by its id.
pub fn line(e: &Event, who: &dyn Fn(&str) -> String) -> String {
    let name = if e.name.is_empty() {
        e.id.as_deref().map(who).unwrap_or_default()
    } else {
        e.name.clone()
    };
    let target = e.target.as_deref().map(who).unwrap_or_default();
    let d = &e.data;
    if e.kind == "command"
        && d["ok"] == true
        && let Some(text) = crate::live_skill::log_result(&d["value"], who)
    {
        return text;
    }
    let num = |k: &str| d[k].as_i64().unwrap_or_default();
    match e.kind.as_str() {
        "chat" => {
            let channel = match num("channel") {
                1 => "",
                2 => " (shout)",
                3 => " (whisper)",
                4 => " (tell)",
                6 => " (party)",
                14 | 17..=22 | 30 => " (DM)",
                _ => " (server)",
            };
            format!("{name}{channel}: {}", d["text"].as_str().unwrap_or_default())
        }
        "attack" => {
            let verb = attack_result(num("result"));
            format!("{name} {verb} {target} (roll {} + {})", num("roll"), num("modifier"))
        }
        "damage" => format!("{name} deals {} damage to {target}", num("amount")),
        "death" => match d["killer"].as_str() {
            Some(killer) => format!("{name} dies (killed by {})", who(killer)),
            None => format!("{name} dies"),
        },
        "spell" => match e.target {
            Some(_) => format!("{name} casts spell {} at {target}", num("spell")),
            None => format!("{name} casts spell {}", num("spell")),
        },
        "vfx" => format!("{name}: projectile of spell {} towards {target}", num("spell")),
        "player_join" => format!("Player {} joined", d["player"].as_str().unwrap_or("?")),
        "player_leave" => format!("Player {} left", d["player"].as_str().unwrap_or("?")),
        "entered" => format!("{name} entered"),
        "left" => format!("{name} left"),
        "combat_start" => format!("{name} enters combat"),
        "combat_end" => format!("{name} leaves combat"),
        "target" => format!("{name} targets {target}"),
        "healed" => format!("{name} healed {} → {} HP", num("from"), num("to")),
        "server_log" => d["line"].as_str().unwrap_or_default().to_string(),
        "command" => {
            let id = d["id"].as_str().unwrap_or_default();
            if unknown(e) {
                format!("Command {id}: {UNKNOWN_OUTCOME}")
            } else if d["ok"] == true {
                match answer(&d["value"]) {
                    Some(value) => format!("Command {id}: {value}"),
                    None => format!("Command {id}: done"),
                }
            } else {
                let code = d["code"].as_str().map(|c| format!(" ({c})")).unwrap_or_default();
                format!("Command {id} failed: {}{code}", d["err"].as_str().unwrap_or_default())
            }
        }
        other => format!("{name}: {other} {d}"),
    }
}

/// What a command gave back (`value`: an inventory, locals, a variable),
/// short enough for one line.
fn answer(value: &Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let text = match value["items"].as_array() {
        Some(items) => {
            let names: Vec<String> = items
                .iter()
                .map(|i| {
                    let name = i["name"].as_str().or(i["resref"].as_str()).unwrap_or("?");
                    match i["stack"].as_i64() {
                        Some(n) if n > 1 => format!("{name} ×{n}"),
                        _ => name.to_string(),
                    }
                })
                .collect();
            let more = if value["truncated"] == true { ", …" } else { "" };
            format!("{} items: {}{more}", names.len(), names.join(", "))
        }
        None => value.to_string(),
    };
    Some(if text.chars().count() > 400 {
        text.chars().take(400).collect::<String>() + "…"
    } else {
        text
    })
}

pub(crate) const UNKNOWN_OUTCOME: &str = "outcome unknown - it may have run; not retried";

fn unknown(e: &Event) -> bool {
    e.kind == "command" && (e.data["code"] == "unknown" || e.data["state"] == "unknown")
}

/// Whether an event says something went wrong (shown in the error colour).
fn bad(e: &Event) -> bool {
    if unknown(e) {
        return false;
    }
    match e.kind.as_str() {
        "server_log" => e.data["script_error"] == true,
        "command" => e.data["ok"] == false,
        "death" => true,
        _ => false,
    }
}

/// The area whose view was shown last, while it is open.
fn shown_area(app: &Moonglow) -> Option<ResRef> {
    app.palette.area.filter(|a| app.area_views.contains_key(a))
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("window.log");
    ui.heading("Live Log");
    crate::live_backend::status_ui(app, ui);
    ui.separator();
    let Some(store) = app.live.clone() else {
        ui.label("Live mode is off: Live › Start Live.");
        return;
    };
    let shown_area = shown_area(app).map(|a| a.to_string());
    let last = store.event_cursor();
    let areas = app.live_log.cache.as_ref().map(|c| c.2.clone()).unwrap_or_default();
    let choices = crate::live_areas::choices(
        app,
        areas.iter().map(String::as_str).chain(shown_area.as_deref()),
    );
    crate::live_areas::tabs(ui, &mut app.live_log.area, &choices, true, false);
    let log = &mut app.live_log;
    ui.horizontal_wrapped(|ui| {
        for page in Page::ALL {
            ui.selectable_value(&mut log.page, page, page.name());
        }
        ui.separator();
        let label = ui.label("Search the log");
        ui.add(
            egui::TextEdit::singleline(&mut log.search).hint_text("Text, name").desired_width(200.),
        )
        .labelled_by(label.id);
        let mut paused = log.paused.is_some();
        if ui.toggle_value(&mut paused, "⏸ Pause").changed() {
            log.paused = paused.then_some(last);
        }
        ui.toggle_value(&mut log.ai_shouts, "AI shouts")
            .on_hover_text("The AI's silent shouts (NW_I_WAS_ATTACKED…) between creatures");
        ui.menu_button("Colours", |ui| {
            for (speaker, color) in Speaker::ALL.into_iter().zip(&mut log.colors) {
                ui.horizontal(|ui| {
                    let mut on = color.is_some();
                    if ui.checkbox(&mut on, speaker.name()).changed() {
                        *color = on.then_some(ui.visuals().text_color());
                    }
                    if let Some(c) = color {
                        ui.color_edit_button_srgba(c);
                    }
                });
            }
        });
        if ui
            .button("Clear")
            .on_hover_text("Hide what is shown now; agents still read it")
            .clicked()
        {
            log.cleared = last;
        }
    });
    let wanted = Shown {
        page: log.page,
        search: log.search.clone(),
        area: log.area.clone(),
        paused: log.paused,
        cleared: log.cleared,
        ai_shouts: log.ai_shouts,
        last,
    };
    if log.cache.as_ref().is_none_or(|c| c.0 != wanted) {
        let query = Query {
            since: wanted.cleared,
            area: wanted.area.clone(),
            kinds: wanted.page.kinds().iter().map(|k| k.to_string()).collect(),
            text: Some(wanted.search.clone()).filter(|t| !t.trim().is_empty()),
            limit: 500,
            ..Default::default()
        };
        let (mut events, _) = store.events(&query);
        if let Some(last) = wanted.paused {
            events.retain(|e| e.n <= last);
        }
        if !wanted.ai_shouts {
            events.retain(|e| !ai_shout(e));
        }
        let people = crate::live_event_people::People::new(&store);
        let who = |id: &str| people.name(id);
        // Every area the server reports or an event names, and those seen before.
        let mut areas: std::collections::BTreeSet<String> = areas.into_iter().collect();
        areas.extend(
            store.status(crate::live_view::now_ms())["areas"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a["area"].as_str().map(str::to_string)),
        );
        areas.extend(events.iter().filter(|e| !e.area.is_empty()).map(|e| e.area.clone()));
        let lines = events
            .into_iter()
            .map(|e| {
                let text = line(&e, &who);
                let speaker = people.speaker(&e);
                (e, text, speaker)
            })
            .collect();
        log.cache = Some((wanted, lines, areas.into_iter().collect()));
    }
    let colors = log.colors;
    let lines = &log.cache.as_ref().expect("filled above").1;
    ui.horizontal(|ui| {
        ui.weak(format!("{} events (UTC times)", lines.len()));
        if ui
            .add_enabled(!lines.is_empty(), egui::Button::new("Copy"))
            .on_disabled_hover_text("No visible events to copy.")
            .on_hover_text("Copy the visible events as text")
            .clicked()
        {
            let text: Vec<String> = lines
                .iter()
                .map(|(e, l, _)| format!("{} [{}] {l}", clock(e.at_ms), e.kind))
                .collect();
            ui.ctx().copy_text(text.join(
                "
",
            ));
        }
    });
    ui.separator();
    if lines.is_empty() {
        ui.label("No events in this view.");
        ui.weak(if log.paused.is_some() {
            "Resume with Pause to see new events."
        } else {
            "Waiting for game events. Choose All, All areas, or clear Search to widen this view."
        });
    }
    let mut chosen = None;
    let row_height = ui.text_style_height(&egui::TextStyle::Body);
    // (Only the rows in sight are laid out.)
    egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show_rows(
        ui,
        row_height,
        lines.len(),
        |ui, range| {
            for (e, text, speaker) in &lines[range] {
                let row = format!("{}  {}", clock(e.at_ms), attributed(text, e));
                let color = speaker.and_then(|s| colors[s as usize]);
                let shown = if unknown(e) {
                    egui::RichText::new(&row).color(ui.visuals().warn_fg_color)
                } else if bad(e) {
                    egui::RichText::new(&row).color(ui.visuals().error_fg_color)
                } else if let Some(c) = color {
                    egui::RichText::new(&row).color(c)
                } else {
                    egui::RichText::new(&row)
                };
                let response =
                    ui.add(egui::Label::new(shown).truncate().sense(egui::Sense::click()));
                let response = response.on_hover_text(&row);
                if response.clicked() {
                    chosen = e.id.clone().map(|id| (e.area.clone(), id));
                }
            }
        },
    );
    // A click: the object selected and centred in its area's view.
    if let Some((area, id)) = chosen
        && let Some((found, _, object)) = store.locate(&id)
        && let Ok(area) = ResRef::from_str(if area.is_empty() { &found } else { &area })
        && let Some(view) = app.area_views.get_mut(&area)
    {
        view.live.on = true;
        view.live.selected = Some(id);
        if let Some(orbit) = &mut view.orbit {
            orbit.target = object.position();
        }
    }
}

/// Authenticated causal attribution supplied by the store; never infer it from speech text.
pub(crate) fn attributed(text: &str, event: &mg_live::events::Event) -> String {
    let by = event.data["by"]
        .as_str()
        .or_else(|| event.data["result"]["by"].as_str())
        .or_else(|| event.data["command"]["by"].as_str());
    match by.filter(|name| !name.is_empty() && *name != "local") {
        Some(name) => format!("{text} [⚡ by LLM: {name}]"),
        None => text.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn unknown_is_a_warning_even_if_a_result_has_a_success_flag() {
        for ok in [false, true] {
            let e = event("command", json!({"id":"c1","ok":ok,"code":"unknown"}));
            assert!(unknown(&e));
            assert!(!bad(&e));
            assert_eq!(line(&e, &str::to_owned), format!("Command c1: {UNKNOWN_OUTCOME}"));
        }
    }

    fn event(kind: &str, data: Value) -> Event {
        Event {
            n: 1,
            at_ms: 3_723_000,
            kind: kind.into(),
            area: "gate".into(),
            id: Some("7".into()),
            name: "Guard".into(),
            target: Some("9".into()),
            data,
        }
    }

    #[test]
    fn the_ai_s_silent_shouts_are_told_from_speech() {
        assert!(ai_shout(&event("chat", json!({"channel":5,"text":"NW_I_AM_DEAD"}))));
        assert!(ai_shout(&event("chat", json!({"channel":1,"text":"anything","silent":true}))));
        assert!(!ai_shout(&event(
            "chat",
            json!({"channel":1,"text":"NW_ is a prefix, not a shout"})
        )));
        assert!(!ai_shout(&event("chat", json!({"channel":1,"text":"Halt!"}))));
        assert!(!ai_shout(&event("death", json!({"text":"NW_I_AM_DEAD"}))));
    }

    #[test]
    fn events_read_as_lines() {
        let who = |id: &str| if id == "9" { "Bandit".to_string() } else { id.to_string() };
        assert_eq!(clock(3_723_000), "01:02:03");
        assert_eq!(
            line(&event("chat", json!({"channel":2,"text":"Halt!"})), &who),
            "Guard (shout): Halt!"
        );
        assert_eq!(
            line(&event("attack", json!({"result":3,"roll":19,"modifier":4})), &who),
            "Guard critically hits Bandit (roll 19 + 4)"
        );
        assert_eq!(
            line(&event("death", json!({"killer":"9"})), &who),
            "Guard dies (killed by Bandit)"
        );
        assert_eq!(
            line(&event("damage", json!({"amount":7})), &who),
            "Guard deals 7 damage to Bandit"
        );
        let failed =
            event("command", json!({"id":"c1","ok":false,"err":"target is dead","code":"dead"}));
        assert_eq!(line(&failed, &who), "Command c1 failed: target is dead (dead)");
        assert!(bad(&failed));
        let inventory = json!({"id":"inv","ok":true,"value":{"items":[
            {"name":"Potion","stack":3},{"resref":"nw_wswls001"}],"truncated":true}});
        assert_eq!(
            line(&event("command", inventory), &who),
            "Command inv: 2 items: Potion ×3, nw_wswls001, …"
        );
        let locals = json!({"id":"loc","ok":true,"value":{"locals":[{"name":"QUEST","type":"int","value":2}]}});
        assert!(line(&event("command", locals), &who).contains("\"QUEST\""));
        let log = event("server_log", json!({"line":"Script error: x","script_error":true}));
        assert_eq!(line(&log, &who), "Script error: x");
        assert!(bad(&log) && !bad(&event("chat", json!({}))));
        for page in Page::ALL {
            assert!(page == Page::All || !page.kinds().is_empty());
        }
    }
}
