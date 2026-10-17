//! Active assistant context and identity resolution; cards are module-scoped,
//! conversations and object notes are additionally scoped to the server endpoint.
use super::{
    cards::{self, Cards, GmCard, Notes, NpcCard},
    memory::{self, Conversation, Turn},
};
use crate::store::{LiveObject, LiveStore, State};
use mg_area::ObjectKind;
use std::path::PathBuf;

#[derive(Debug)]
pub(super) struct Knowledge {
    pub context: (PathBuf, PathBuf, String),
    cards_path: PathBuf,
    notes_path: PathBuf,
    cards: Cards,
    notes: Notes,
    memory: memory::Book,
    pub cursor: u64,
}
#[cfg(feature = "measure")]
impl Knowledge {
    pub(super) fn measure_census(&self) -> serde_json::Value {
        serde_json::json!({"npc_cards":self.cards.npcs.len(),"notes":self.notes.objects.len(),
            "note_bytes":self.notes.objects.values().map(String::len).sum::<usize>(),"memory":self.memory.measure_census()})
    }
}

fn resolve<'a>(s: &'a State, id: &str) -> Result<(ObjectKind, &'a LiveObject), String> {
    if let Some(found) = s.areas.values().find_map(|a| a.find(id)) {
        return Ok(found);
    }
    let mut found =
        s.areas.values().flat_map(|a| a.all()).filter(|(_, o)| o.tag == id || o.resref == id);
    let first = found.next().ok_or("Object is not currently reported")?;
    if found.next().is_some() {
        return Err("Object tag/resref is ambiguous; use its current id".into());
    }
    Ok(first)
}
fn npc(s: &State, id: &str) -> Result<(String, String), String> {
    // Explicit saved card IDs can be read even while the NPC is absent.
    if (id.starts_with("tag:") || id.starts_with("resref:")) && id.len() <= 260 {
        let name = s
            .areas
            .values()
            .flat_map(|a| a.creatures.iter())
            .find(|o| {
                id.strip_prefix("tag:").is_some_and(|tag| tag == o.tag)
                    || id.strip_prefix("resref:").is_some_and(|r| r == o.resref)
            })
            .map(|o| o.name.clone())
            .unwrap_or_default();
        return Ok((id.into(), name));
    }
    let (kind, o) = resolve(s, id)?;
    if kind != ObjectKind::Creature || o.pc || o.dm || o.possessed {
        return Err("Choose an NPC for its card or conversation memory".into());
    }
    let key = if !o.tag.is_empty() {
        format!("tag:{}", o.tag)
    } else if !o.resref.is_empty() {
        format!("resref:{}", o.resref)
    } else {
        return Err("NPC has no tag or blueprint resref for a saved card".into());
    };
    Ok((key, o.name.clone()))
}
fn player(s: &State, id: &str) -> Result<(String, String), String> {
    let (kind, o) = resolve(s, id)?;
    if kind != ObjectKind::Creature || !o.pc || o.dm {
        return Err("Choose a reported player".into());
    }
    // Reports do not contain a CD key or vault id. Use tag + character name;
    // refuse simultaneous duplicates instead of silently mixing characters.
    let key = format!("pc:{}:{}", o.tag, o.name);
    if s.areas
        .values()
        .flat_map(|a| a.creatures.iter())
        .filter(|c| c.pc && c.tag == o.tag && c.name == o.name)
        .count()
        != 1
    {
        return Err("Player tag/name is ambiguous; memory cannot be safely assigned".into());
    }
    Ok((key, o.name.clone()))
}
fn object_key(s: &State, id: &str) -> Result<String, String> {
    let (kind, o) = resolve(s, id)?;
    if o.pc {
        return player(s, id).map(|(k, _)| k);
    }
    let unique = !o.tag.is_empty()
        && s.areas
            .values()
            .flat_map(|a| a.all())
            .filter(|(k, c)| *k == kind && c.tag == o.tag)
            .count()
            == 1;
    Ok(if unique {
        format!("{kind:?}:tag:{}", o.tag)
    } else {
        format!("{kind:?}:boot:{}:id:{}", s.boot, o.id)
    })
}
fn book(s: &State) -> Result<&Knowledge, String> {
    s.assistant
        .knowledge
        .as_ref()
        .ok_or("Open and save a module to use assistant cards, notes and memory".into())
}
fn book_mut(s: &mut State) -> Result<&mut Knowledge, String> {
    s.assistant
        .knowledge
        .as_mut()
        .ok_or("Open and save a module to use assistant cards, notes and memory".into())
}
impl LiveStore {
    pub fn assistant_context(
        &self,
        data: PathBuf,
        module: PathBuf,
        server: String,
    ) -> Result<(), String> {
        let context = (data, module, server);
        if self.lock().assistant.knowledge.as_ref().is_some_and(|k| k.context == context) {
            return Ok(());
        }
        // Stop serving the old module's secrets even if the new file fails to load.
        self.clear_assistant_context();
        let path = cards::folder(&context.0, &context.1);
        let server = path.join(format!("server-{:016x}", cards::hash(&context.2)));
        // A corrupt file fails closed and is never overwritten.
        let cards_path = path.join("cards.json");
        let notes_path = server.join("notes.json");
        let cards = cards::read(&cards_path)?;
        let notes = cards::read(&notes_path)?;
        let memory = memory::Book::open(server.join("memory.json"))?;
        let mut s = self.lock();
        super::cancel(&mut s, "Assistant module/server context changed");
        let cursor = s.events.last();
        s.assistant.knowledge_revision += 1;
        let old = s.assistant.knowledge.replace(Knowledge {
            context,
            cards_path,
            notes_path,
            cards,
            notes,
            memory,
            cursor,
        });
        drop(s);
        drop(old);
        Ok(())
    }
    pub fn clear_assistant_context(&self) {
        let mut s = self.lock();
        super::cancel(&mut s, "Assistant module context closed");
        let old = s.assistant.knowledge.take();
        s.assistant.knowledge_revision += 1;
        drop(s);
        drop(old);
    }
    pub fn assistant_revision(&self) -> (u64, u64) {
        let s = self.lock();
        (
            s.assistant.knowledge_revision,
            s.assistant.knowledge.as_ref().map_or(0, |k| k.memory.revision()),
        )
    }
    pub fn object_note_key(&self, id: &str) -> Result<String, String> {
        object_key(&self.lock(), id)
    }
    /// Snapshot only nonempty notes for an area. Called by the UI on report or
    /// note revision changes, never once per marker on every frame.
    pub fn area_notes(&self, area: &str) -> std::collections::HashMap<String, String> {
        let s = self.lock();
        let Some(k) = s.assistant.knowledge.as_ref().filter(|k| !k.notes.objects.is_empty()) else {
            return Default::default();
        };
        let Some(area) = s.areas.get(area) else { return Default::default() };
        let mut tags = std::collections::HashMap::new();
        let mut players = std::collections::HashMap::new();
        for (kind, o) in s.areas.values().flat_map(|a| a.all()) {
            *tags.entry((kind, o.tag.as_str())).or_insert(0usize) += 1;
            if o.pc {
                *players.entry((o.tag.as_str(), o.name.as_str())).or_insert(0usize) += 1;
            }
        }
        area.all()
            .filter_map(|(kind, o)| {
                let key = if o.pc {
                    if players.get(&(o.tag.as_str(), o.name.as_str())) != Some(&1) {
                        return None;
                    }
                    format!("pc:{}:{}", o.tag, o.name)
                } else if !o.tag.is_empty() && tags.get(&(kind, o.tag.as_str())) == Some(&1) {
                    format!("{kind:?}:tag:{}", o.tag)
                } else {
                    format!("{kind:?}:boot:{}:id:{}", s.boot, o.id)
                };
                k.notes
                    .objects
                    .get(&key)
                    .filter(|note| !note.is_empty())
                    .map(|note| (o.id.clone(), note.clone()))
            })
            .collect()
    }
    pub fn assistant_error(&self) -> Option<String> {
        self.lock().assistant.knowledge.as_ref().and_then(|k| k.memory.error())
    }
    pub fn gm_card(&self) -> Result<GmCard, String> {
        Ok(book(&self.lock())?.cards.gm.clone())
    }
    pub fn save_gm_card(&self, card: GmCard) -> Result<(), String> {
        card.check()?;
        let mut s = self.lock();
        let k = book_mut(&mut s)?;
        let mut cards = k.cards.clone();
        cards.gm = card;
        cards::write(&k.cards_path, &cards)?;
        k.cards = cards;
        s.assistant.knowledge_revision += 1;
        Ok(())
    }
    pub fn npc_card(&self, id: &str) -> Result<NpcCard, String> {
        let s = self.lock();
        let (id, name) = npc(&s, id)?;
        let k = book(&s)?;
        Ok(k.cards.npcs.get(&id).cloned().unwrap_or(NpcCard { id, name, ..Default::default() }))
    }
    pub fn save_npc_card(&self, id: &str, mut card: NpcCard) -> Result<(), String> {
        card.check()?;
        let mut s = self.lock();
        let (key, _) = npc(&s, id)?;
        card.id = key.clone();
        let k = book_mut(&mut s)?;
        if k.cards.npcs.len() >= 1000 && !k.cards.npcs.contains_key(&key) {
            return Err("At most 1000 NPC cards are supported".into());
        }
        let mut cards = k.cards.clone();
        cards.npcs.insert(key, card);
        cards::write(&k.cards_path, &cards)?;
        k.cards = cards;
        s.assistant.knowledge_revision += 1;
        Ok(())
    }
    pub fn object_note(&self, id: &str) -> Result<String, String> {
        let s = self.lock();
        let key = object_key(&s, id)?;
        Ok(book(&s)?.notes.objects.get(&key).cloned().unwrap_or_default())
    }
    pub fn save_object_note(&self, id: &str, note: &str) -> Result<(), String> {
        cards::bounded(&[note])?;
        let mut s = self.lock();
        let key = object_key(&s, id)?;
        let k = book_mut(&mut s)?;
        if k.notes.objects.len() >= 4000 && !k.notes.objects.contains_key(&key) {
            return Err("At most 4000 object notes are supported".into());
        }
        let mut notes = k.notes.clone();
        if note.is_empty() {
            notes.objects.remove(&key);
        } else {
            notes.objects.insert(key, note.into());
        }
        cards::write(&k.notes_path, &notes)?;
        k.notes = notes;
        s.assistant.knowledge_revision += 1;
        Ok(())
    }
    pub fn npc_memory(
        &self,
        id: &str,
        who: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Conversation>, String> {
        let s = self.lock();
        let (npc, _) = npc(&s, id)?;
        let player = who.map(|p| player(&s, p)).transpose()?;
        Ok(book(&s)?.memory.read(&npc, player.as_ref().map(|(p, _)| p.as_str()), limit))
    }
    pub fn npc_memory_note(
        &self,
        id: &str,
        who: Option<&str>,
        summary: &str,
    ) -> Result<(), String> {
        let s = self.lock();
        let (npc, name) = npc(&s, id)?;
        let player = who.map(|p| player(&s, p)).transpose()?;
        book(&s)?.memory.note(
            &npc,
            &name,
            player.as_ref().map(|(p, n)| (p.as_str(), n.as_str())),
            summary,
        )
    }
    pub fn flush_npc_memory(&self) -> Result<(), String> {
        book(&self.lock())?.memory.flush()
    }
}

/// Called after accepted event ingestion, under the store lock. Only new events
/// are inspected; the separate writer coalesces disk work outside this lock.
pub(super) fn capture(s: &mut State) {
    let Some(k) = s.assistant.knowledge.as_ref() else { return };
    let cursor = k.cursor;
    if cursor == s.events.last() {
        return;
    }
    let events = s.events.query(&crate::events::Query {
        since: cursor,
        kinds: vec!["chat".into(), "dialog".into()],
        limit: 500,
        ..Default::default()
    });
    for e in events {
        if e.data["boot"].as_str().is_some_and(|boot| boot != s.boot) {
            continue;
        }
        if e.data["silent"] == true || e.data["channel"] == 5 {
            continue;
        }
        let Some(text) = e.data["text"].as_str().filter(|t| !t.is_empty()) else { continue };
        let Some(id) = e.data["speaker"].as_str().or(e.id.as_deref()) else { continue };
        let Ok((kind, speaker)) = resolve(s, id) else { continue };
        if kind != ObjectKind::Creature || speaker.dm || speaker.possessed {
            continue;
        }
        let explicit = e.data["recipient"].as_str().or(e.target.as_deref()).filter(|id| *id != "0");
        let others: Vec<_> = if let Some(target) = explicit {
            vec![(target.to_string(), "addressed")]
        } else {
            // Nearby is an approximation from the event position plus latest report.
            if e.kind == "chat" && !matches!(e.data["channel"].as_i64(), Some(1 | 3 | 17 | 19)) {
                continue;
            }
            if s.areas.get(&e.area).is_none_or(|a| e.at_ms.saturating_sub(a.received_ms) > 5000) {
                continue;
            }
            let pos = e.data.get("position");
            let x = pos.and_then(|p| p["x"].as_f64()).unwrap_or(f64::from(speaker.x));
            let y = pos.and_then(|p| p["y"].as_f64()).unwrap_or(f64::from(speaker.y));
            s.areas
                .get(&e.area)
                .into_iter()
                .flat_map(|a| a.creatures.iter())
                .filter(|o| {
                    o.id != speaker.id
                        && o.pc != speaker.pc
                        && !o.dm
                        && !o.possessed
                        && ((f64::from(o.x) - x).powi(2) + (f64::from(o.y) - y).powi(2)) <= 400.
                })
                .take(20)
                .map(|o| (o.id.clone(), "nearby (latest report, not delivery proof)"))
                .collect()
        };
        for (other, context) in others {
            let (npc_id, player_id) = if speaker.pc {
                (other.as_str(), speaker.id.as_str())
            } else {
                (speaker.id.as_str(), other.as_str())
            };
            let (Ok((npc, npc_name)), Ok((player, player_name))) =
                (npc(s, npc_id), player(s, player_id))
            else {
                continue;
            };
            let turn = Turn {
                at_ms: e.at_ms,
                speaker: speaker.name.clone(),
                text: text.chars().take(2048).collect(),
                area: e.area.clone(),
                context: context.into(),
                by: e.data["by"].as_str().map(str::to_owned),
            };
            s.assistant.knowledge.as_ref().unwrap().memory.append(
                &npc,
                &npc_name,
                &player,
                &player_name,
                turn,
            );
        }
    }
    s.assistant.knowledge.as_mut().unwrap().cursor = s.events.last();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Work(PathBuf);
    impl Work {
        fn new(name: &str) -> Self {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-output/live-audit7/assistant-tests");
            let path = root.join(format!("{name}-{}", crate::service::generate_token().unwrap()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Work {
        fn drop(&mut self) {
            if !std::thread::panicking() {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
    }
    fn store() -> LiveStore {
        let s = LiveStore::default();
        s.sync(br#"{"v":1,"boot":"b","seq":1,"areas":[{"area":"gate","cre":[{"id":"a","tag":"captain","name":"Captain","r":"guard","x":10,"y":10},{"id":"b","tag":"traveller","name":"Traveller","pc":true,"x":12,"y":10},{"id":"c","tag":"far_player","name":"Far player","pc":true,"x":90,"y":90}]}]}"#,100).unwrap();
        s
    }
    #[test]
    fn assistant_cards_and_notes_persist_with_module_and_server_boundaries() {
        let work = Work::new("cards");
        let s = store();
        let module = work.0.join("campaign.mod");
        s.assistant_context(work.0.clone(), module.clone(), "server-one".into()).unwrap();
        let gm = GmCard {
            campaign_tone: "Quiet intrigue".into(),
            never_reveal: "The captain is a spy".into(),
            ..Default::default()
        };
        s.save_gm_card(gm.clone()).unwrap();
        let mut npc = s.npc_card("a").unwrap();
        npc.personality = "Patient".into();
        s.save_npc_card("a", npc).unwrap();
        s.save_object_note("a", "Has the harbour key").unwrap();
        assert_eq!(s.npc_card("captain").unwrap().id, "tag:captain");
        assert!(s.npc_card("b").is_err());
        s.assistant_context(work.0.clone(), module.clone(), "server-two".into()).unwrap();
        assert_eq!(s.gm_card().unwrap(), gm);
        assert_eq!(s.npc_card("a").unwrap().personality, "Patient");
        assert_eq!(s.object_note("a").unwrap(), "");
        s.assistant_context(work.0.clone(), module, "server-one".into()).unwrap();
        assert_eq!(s.object_note("a").unwrap(), "Has the harbour key");
        s.assistant_context(work.0.clone(), work.0.join("other.mod"), "server-one".into()).unwrap();
        assert_eq!(s.gm_card().unwrap(), GmCard::default());
        s.clear_assistant_context();
    }
    #[test]
    fn assistant_memory_keeps_both_directions_deduplicates_and_persists() {
        let work = Work::new("memory");
        let s = store();
        let module = work.0.join("campaign.mod");
        s.assistant_context(work.0.clone(), module.clone(), "server".into()).unwrap();
        let events = json!({"v":1,"boot":"b","seq":2,"areas":[],"events":[
            {"seq":1,"kind":"chat","area":"gate","id":"b","text":"May I pass?","channel":1,"position":{"x":12,"y":10}},
            {"seq":2,"kind":"chat","area":"gate","id":"a","target":"b","text":"Show your papers.","channel":1},
            {"seq":3,"kind":"chat","area":"gate","id":"a","text":"Unresolved private message","channel":4},
            {"seq":4,"kind":"chat","area":"gate","id":"a","text":"AI shout","silent":true,"channel":2}
        ]});
        s.sync(events.to_string().as_bytes(), 200).unwrap();
        let mut second = events.clone();
        second["seq"] = json!(3);
        s.sync(second.to_string().as_bytes(), 201).unwrap();
        let memory = s.npc_memory("a", Some("b"), 200).unwrap();
        assert_eq!(memory.len(), 1);
        assert_eq!(memory[0].turns.len(), 2);
        assert_eq!(memory[0].turns[0].speaker, "Traveller");
        assert!(memory[0].turns[0].context.starts_with("nearby"));
        assert_eq!(memory[0].turns[1].context, "addressed");
        assert!(s.npc_memory("a", Some("c"), 200).unwrap().is_empty());
        s.npc_memory_note("a", Some("b"), "Requested entry; papers needed.").unwrap();
        s.flush_npc_memory().unwrap();
        s.clear_assistant_context();
        s.assistant_context(work.0.clone(), module, "server".into()).unwrap();
        let memory = s.npc_memory("a", Some("b"), 1).unwrap();
        assert_eq!(memory[0].turns.len(), 1);
        assert_eq!(memory[0].summary, "Requested entry; papers needed.");
        s.clear_assistant_context();
    }
    #[test]
    fn assistant_corrupt_file_is_preserved_and_does_not_leak_previous_module() {
        let work = Work::new("corrupt");
        let s = store();
        s.assistant_context(work.0.clone(), work.0.join("first.mod"), "server".into()).unwrap();
        s.save_gm_card(GmCard { never_reveal: "Secret".into(), ..Default::default() }).unwrap();
        let module = work.0.join("broken.mod");
        let path = cards::folder(&work.0, &module).join("cards.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "broken original").unwrap();
        assert!(s.assistant_context(work.0.clone(), module, "server".into()).is_err());
        assert!(s.gm_card().is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "broken original");
    }
    #[test]
    fn assistant_memory_accepts_first_speech_with_first_object_report() {
        let work = Work::new("first-report");
        let s = LiveStore::default();
        s.assistant_context(work.0.clone(), work.0.join("campaign.mod"), "server".into()).unwrap();
        s.sync(br#"{"v":1,"boot":"b","seq":1,"areas":[{"area":"gate","cre":[{"id":"a","tag":"captain","name":"Captain"},{"id":"b","pc":true,"name":"Traveller"}]}],"events":[{"seq":1,"kind":"chat","area":"gate","id":"b","target":"a","text":"Hello","channel":1}]}"#,100).unwrap();
        assert_eq!(s.npc_memory("a", Some("b"), 50).unwrap()[0].turns[0].text, "Hello");
        s.clear_assistant_context();
    }
}
