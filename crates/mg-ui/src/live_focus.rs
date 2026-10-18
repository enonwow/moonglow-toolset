//! Saved per-module watch lists. Conversation matches are fixed when events arrive,
//! using the speaker event position and the latest watcher report. This
//! proximity estimate is explicitly labelled, not a claim of native delivery.
use crate::{Action, Moonglow, Tab};
use mg_area::ObjectKind;
use mg_live::{
    events::{Event, Query},
    store::{LiveObject, LiveStore},
};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf, sync::Weak};

const HISTORY: usize = 500;
const MAX_ENTRIES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub id: String,
    pub tag: String,
    pub name: String,
    pub kind: String,
    pub pc: bool,
    pub boot: String,
    pub area: String,
    pub portrait: Option<String>,
    pub appearance: Option<u16>,
    #[serde(default)]
    pub gender: Option<u8>,
    #[serde(skip)]
    pub reported: bool,
    #[serde(skip)]
    identity_valid: bool,
    #[serde(skip)]
    pub hp: Option<[i32; 2]>,
    #[serde(skip)]
    pub combat: bool,
    #[serde(skip)]
    pub position: [f32; 3],
}
impl Member {
    fn from_object(area: String, kind: ObjectKind, o: &LiveObject, boot: &str) -> Self {
        Self {
            id: o.id.clone(),
            tag: o.tag.clone(),
            name: crate::live_panel::name(o).into(),
            kind: format!("{kind:?}"),
            pc: o.pc,
            boot: boot.into(),
            area,
            portrait: o.portrait.clone(),
            appearance: o.app,
            gender: crate::live_portrait::gender(o),
            reported: true,
            identity_valid: true,
            hp: o.hp,
            combat: o.combat,
            position: [o.x, o.y, o.z],
        }
    }
    fn compatible(&self, kind: ObjectKind, o: &LiveObject) -> bool {
        self.kind == format!("{kind:?}")
            && self.pc == o.pc
            && (self.tag.is_empty() || self.tag == o.tag)
            && (!(self.pc || self.tag.is_empty()) || self.name == crate::live_panel::name(o))
    }
}
#[derive(Debug, Clone)]
pub struct Row {
    pub event: Event,
    /// Identities matched on receipt, so moving later does not reclassify old speech.
    pub members: Vec<String>,
    pub read: bool,
    pub text: String,
    pub speaker: Option<crate::live_log::Speaker>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub members: Vec<Member>,
    #[serde(skip)]
    pub rows: VecDeque<Row>,
    #[serde(skip)]
    pub unread: usize,
    #[serde(skip)]
    pub watched_from_ms: u64,
}
/// Captured history is retained when optional conversation audiences are hidden.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConversationFilter {
    pub nearby: bool,
    pub server: bool,
}
impl ConversationFilter {
    fn shows(self, row: &Row, members: &[Member]) -> bool {
        let event = &row.event;
        let server = event.kind == "dm_message"
            || row.speaker == Some(crate::live_log::Speaker::Server)
            || (event.kind == "chat" && event.data["channel"] == 5);
        if server {
            return self.server;
        }
        let speaker = if event.kind == "dialog" {
            event.data["speaker"].as_str().or(event.id.as_deref())
        } else {
            event.id.as_deref()
        };
        self.nearby || speaker.is_some_and(|id| members.iter().any(|m| m.id == id))
    }
}

#[derive(Debug)]
pub struct LiveFocus {
    pub data_dir: Option<PathBuf>,
    pub entries: Vec<Entry>,
    pub selected: Option<usize>,
    pub events_page: bool,
    pub memory_page: bool,
    pub conversation: ConversationFilter,
    pub member: Option<String>,
    pub area: Option<String>,
    pub text: String,
    pub volume: mg_live::store::SpeechVolume,
    pub notice: Option<String>,
    module_path: Option<PathBuf>,
    writable: bool,
    stamp: Option<(String, u64)>,
    source: Weak<LiveStore>,
    cursor: u64,
    pub nearby_missing: bool,
    pub nearby_estimated: bool,
}
impl Default for LiveFocus {
    fn default() -> Self {
        Self {
            data_dir: crate::recovery::data_dir(),
            entries: Vec::new(),
            selected: None,
            events_page: false,
            memory_page: false,
            conversation: ConversationFilter::default(),
            member: None,
            area: None,
            text: String::new(),
            volume: Default::default(),
            notice: None,
            module_path: None,
            writable: true,
            stamp: None,
            source: Weak::new(),
            cursor: 0,
            nearby_missing: false,
            nearby_estimated: false,
        }
    }
}
impl LiveFocus {
    pub fn unread(&self) -> usize {
        self.entries.iter().map(|e| e.unread).sum()
    }
    pub fn path(&self) -> Result<PathBuf, String> {
        let module =
            self.module_path.as_ref().ok_or("Save and open a module to keep a Focus list.")?;
        let dir = self.data_dir.as_ref().ok_or("Application data directory unavailable.")?;
        // Stable path hash separates different modules with the same file name.
        let key = module.to_string_lossy();
        #[cfg(windows)]
        let key = key.to_lowercase();
        let hash = key
            .bytes()
            .fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
        let stem: String = module
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
            .take(40)
            .collect();
        Ok(dir.join("live").join("focus").join(format!("{stem}-{hash:016x}")).join("watch.json"))
    }
    pub fn context(&mut self, module: Option<PathBuf>) {
        if self.module_path == module {
            return;
        }
        self.module_path = module;
        self.writable = true;
        self.entries.clear();
        self.selected = None;
        self.member = None;
        self.area = None;
        self.stamp = None;
        self.cursor = 0;
        self.text.clear();
        self.notice = None;
        self.nearby_missing = false;
        self.nearby_estimated = false;
        let loaded = self.path().and_then(|path| {
            if !path.exists() {
                return Ok(Vec::new());
            }
            let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
            if size > 1_048_576 {
                return Err("Focus list exceeds 1 MiB.".into());
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let entries: Vec<Entry> = serde_json::from_slice(&bytes)
                .map_err(|e| format!("Cannot read Focus list: {e}"))?;
            if entries.len() > MAX_ENTRIES
                || entries.iter().any(|e| e.members.is_empty() || e.members.len() > 256)
            {
                return Err("Invalid Focus list size.".into());
            }
            Ok(entries)
        });
        match loaded {
            Ok(entries) => {
                self.entries = entries;
                for e in &mut self.entries {
                    e.watched_from_ms = crate::live_view::now_ms();
                }
                self.selected = (!self.entries.is_empty()).then_some(0);
            }
            Err(e) => {
                self.writable = false;
                self.notice = Some(e)
            }
        }
    }
    pub fn save(&mut self) -> Result<(), String> {
        if !self.writable {
            return Err("Existing Focus list could not be read; it has been preserved. Repair the file before saving.".into());
        }
        let path = self.path()?;
        std::fs::create_dir_all(path.parent().expect("parent")).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(&self.entries).map_err(|e| e.to_string())?;
        if bytes.len() > 1_048_576 {
            return Err("Focus list exceeds 1 MiB; remove entries before saving.".into());
        }
        let part = path.with_extension("json.part");
        std::fs::write(&part, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&part, &path).map_err(|e| e.to_string())
    }
    pub fn add(&mut self, store: &LiveStore, ids: &[String]) -> Result<(), String> {
        self.path()?;
        let mut ids = ids.to_vec();
        ids.sort();
        ids.dedup();
        if let Some(i) = self.entries.iter().position(|e| {
            let mut old: Vec<_> = e.members.iter().map(|m| m.id.clone()).collect();
            old.sort();
            old == ids
        }) {
            self.selected = Some(i);
            return Ok(());
        }
        if self.entries.len() >= MAX_ENTRIES || ids.len() > 256 {
            return Err("Focus allows 100 entries, up to 256 members each.".into());
        }
        let boot = store.boot();
        let members: Vec<_> = ids
            .iter()
            .filter_map(|id| store.locate(id))
            .map(|(a, k, o)| Member::from_object(a, k, &o, &boot))
            .collect();
        if members.is_empty() {
            return Err("Select a reported object first.".into());
        }
        let name = if members.len() == 1 {
            members[0].name.clone()
        } else {
            let n = (1..)
                .find(|n| !self.entries.iter().any(|e| e.name == format!("Group {n}")))
                .unwrap();
            format!("Group {n}")
        };
        self.entries.push(Entry {
            name,
            members,
            rows: VecDeque::new(),
            unread: 0,
            watched_from_ms: crate::live_view::now_ms(),
        });
        self.selected = Some(self.entries.len() - 1);
        self.stamp = None;
        // Replay the retained event ring for this new watch, without duplicating older rows.
        self.cursor = 0;
        self.save()
    }
    fn resolve(&mut self, store: &LiveStore, boot: &str) {
        for e in &mut self.entries {
            for m in &mut e.members {
                let exact = (m.boot == boot)
                    .then(|| store.locate(&m.id))
                    .flatten()
                    .filter(|(_, k, o)| m.compatible(*k, o));
                let found = exact.or_else(|| {
                    if m.tag.is_empty() {
                        return None;
                    }
                    let mut candidates = store
                        .find(&m.tag, 1001)
                        .into_iter()
                        .filter(|v| v["tag"].as_str() == Some(&m.tag))
                        .filter_map(|v| v["id"].as_str().and_then(|id| store.locate(id)))
                        .filter(|(_, k, o)| m.compatible(*k, o));
                    let first = candidates.next()?;
                    candidates.next().is_none().then_some(first)
                });
                if let Some((a, k, o)) = found {
                    *m = Member::from_object(a, k, &o, boot);
                } else {
                    m.reported = false;
                    // A disappearing actor can still have a final death/leave event.
                    // An id reused by another actor or boot must never route their speech here.
                    m.identity_valid = m.boot == boot && store.locate(&m.id).is_none();
                    m.hp = None;
                    m.combat = false;
                }
            }
        }
    }
    fn ingest(&mut self, store: &LiveStore, events: Vec<Event>) {
        for event in events {
            if event.kind == "chat"
                && !crate::live_log::ai_shout(&event)
                && matches!(event.data["channel"].as_i64(), Some(1 | 3 | 17 | 19))
            {
                if event.data["position"]["x"].is_number() {
                    self.nearby_estimated = true;
                } else {
                    self.nearby_missing = true;
                }
            }
            for entry in &mut self.entries {
                if entry.rows.back().is_some_and(|r| r.event.n >= event.n)
                    || !matches(entry, &event)
                {
                    continue;
                }
                let members = entry
                    .members
                    .iter()
                    .filter(|m| {
                        matches_members(std::slice::from_ref(m), entry.watched_from_ms, &event)
                    })
                    .map(|m| m.id.clone())
                    .collect();
                let who = |id: &str| {
                    store
                        .locate(id)
                        .map_or_else(|| id.into(), |(_, _, o)| crate::live_panel::name(&o).into())
                };
                let text = if event.kind == "inventory_command" {
                    format!(
                        "{}: {} {} (command acknowledged)",
                        event.id.as_deref().map(&who).unwrap_or_default(),
                        event.data["operation"].as_str().unwrap_or_default().replace('_', " "),
                        event.data["item"].as_str().unwrap_or_default()
                    )
                } else if event.kind == "dm_message" {
                    format!(
                        "DM (command acknowledged): {}",
                        event.data["text"].as_str().unwrap_or_default()
                    )
                } else if event.kind == "dialog" {
                    let name = if event.name.is_empty() {
                        event.id.as_deref().map(&who).unwrap_or_default()
                    } else {
                        event.name.clone()
                    };
                    format!(
                        "{name} (dialogue): {}",
                        event.data["text"].as_str().unwrap_or_default()
                    )
                } else {
                    crate::live_log::line(&event, &who)
                };
                let speaker = if event.kind == "dm_message" {
                    Some(crate::live_log::Speaker::Dm)
                } else if event.kind == "dialog" {
                    Some(if event.data["speaker_is_pc"] == true {
                        crate::live_log::Speaker::Player
                    } else {
                        crate::live_log::Speaker::Npc
                    })
                } else {
                    crate::live_log::speaker(store, &event)
                };
                let alert = matches!(event.kind.as_str(), "chat" | "dialog" | "dm_message")
                    || matches!(
                        event.kind.as_str(),
                        "attack" | "damage" | "combat_start" | "death"
                    );
                entry.unread = entry.unread.saturating_add(usize::from(alert));
                entry.rows.push_back(Row {
                    event: event.clone(),
                    text,
                    speaker,
                    members,
                    read: !alert,
                });
                while entry.rows.len() > HISTORY {
                    entry.rows.pop_front();
                }
            }
        }
    }
}

fn matches(entry: &Entry, e: &Event) -> bool {
    matches_members(&entry.members, entry.watched_from_ms, e)
}
fn matches_members(members: &[Member], watched_from_ms: u64, e: &Event) -> bool {
    let has = |id: Option<&str>| {
        id.is_some_and(|id| members.iter().any(|m| m.identity_valid && m.id == id))
    };
    let own = has(e.id.as_deref());
    let addressed = has(e.target.as_deref())
        || has(e.data["recipient"].as_str())
        || has(e.data["listener"].as_str());
    if e.kind == "dm_message" {
        return addressed;
    }
    if e.kind == "dialog" {
        return own || addressed || has(e.data["speaker"].as_str());
    }
    if e.kind == "chat" {
        if crate::live_log::ai_shout(e) {
            return false;
        }
        if own || addressed {
            return true;
        }
        let channel = e.data["channel"].as_i64().unwrap_or(0);
        if matches!(channel, 2 | 18) {
            return e.at_ms >= watched_from_ms
                && members.iter().any(|m| m.reported && m.area == e.area);
        }
        if !matches!(channel, 1 | 3 | 17 | 19) {
            return false;
        }
        if e.target.is_some() || e.data["recipient"].is_string() {
            return false;
        }
        if e.at_ms < watched_from_ms {
            return false;
        }
        let p = &e.data["position"];
        let (Some(x), Some(y)) = (p["x"].as_f64(), p["y"].as_f64()) else { return false };
        return members.iter().any(|m| {
            m.reported
                && m.area == e.area
                && (f64::from(m.position[0]) - x).powi(2) + (f64::from(m.position[1]) - y).powi(2)
                    <= 400.
        });
    }
    (own || addressed || has(e.data["killer"].as_str()))
        && matches!(
            e.kind.as_str(),
            "attack"
                | "damage"
                | "death"
                | "combat_start"
                | "combat_end"
                | "target"
                | "healed"
                | "entered"
                | "left"
                | "area_change"
                | "item"
                | "item_acquire"
                | "item_lost"
                | "item_acquired"
                | "item_loss"
                | "inventory_command"
                | "player_leave"
        )
}

/// SendMessageToPC is native feedback, outside the chat hook. A successful
/// command acknowledgement can be shown, explicitly without a delivery claim.
fn command_echo(event: &mut Event, status: &serde_json::Value) {
    if event.kind != "command" || event.data["ok"] != true {
        return;
    }
    let Some(result) = status["results"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["command"]["id"] == event.data["id"])
    else {
        return;
    };
    let command = &result["command"];
    let op = command["op"].as_str().unwrap_or_default();
    if matches!(
        op,
        "give_item"
            | "take_item"
            | "equip_item"
            | "unequip_item"
            | "transfer_item"
            | "drop_item"
            | "use_item"
    ) {
        event.kind = "inventory_command".into();
        event.id = command["target"].as_str().map(str::to_owned);
        event.target = command["recipient"].as_str().map(str::to_owned);
        event.data["operation"] = command["op"].clone();
        event.data["item"] =
            command.get("resref").or_else(|| command.get("item")).cloned().unwrap_or_default();
        return;
    }
    if op != "pc_message" {
        return;
    }
    event.kind = "dm_message".into();
    event.id = None;
    event.name = "DM".into();
    event.target = command["target"].as_str().map(str::to_owned);
    event.data["text"] = command["text"].clone();
}

pub(crate) fn update(app: &mut Moonglow) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("live.focus_update");
    if app.settings.no_live_focus {
        return;
    }
    let module = app.live_backend().context();
    app.live_focus.context(module);
    let Some(store) = app.live.clone() else {
        for e in &mut app.live_focus.entries {
            for m in &mut e.members {
                m.reported = false;
            }
        }
        return;
    };
    let focus = &mut app.live_focus;
    if focus.entries.is_empty() {
        return;
    }
    let source = std::sync::Arc::downgrade(&store);
    if !focus.source.ptr_eq(&source) {
        focus.source = source;
        focus.cursor = 0;
        focus.stamp = None;
        for e in &mut focus.entries {
            e.rows.clear();
            e.unread = 0;
        }
    }
    store.expire(crate::live_view::now_ms());
    let stamp = store.revision();
    if focus.stamp.as_ref() != Some(&stamp) {
        focus.resolve(&store, &stamp.0);
        focus.stamp = Some(stamp);
    }
    if store.event_cursor() == focus.cursor {
        return;
    }
    let (mut events, last) = store.events(&Query { since: focus.cursor, ..Default::default() });
    // Results are needed only for successful command echoes, not ordinary speech.
    if events.iter().any(|e| e.kind == "command" && e.data["ok"] == true) {
        let status = store.status(crate::live_view::now_ms());
        for event in &mut events {
            command_echo(event, &status);
        }
    }
    focus.ingest(&store, events);
    focus.cursor = last;
}

pub(crate) fn add(app: &mut Moonglow, ids: &[String]) {
    if let Some(store) = app.live.clone() {
        app.live_focus.context(app.live_backend().context());
        if let Err(e) = app.live_focus.add(&store, ids) {
            app.live_focus.notice = Some(e);
        }
        app.actions.push(Action::OpenTab(Tab::LiveFocus));
    }
}

mod window;
pub(crate) use window::ui;

#[cfg(test)]
mod tests;
