//! Display fields resolved once per event-window refresh, without retaining looks.
use crate::live_log::Speaker;
use mg_live::{events::Event, store::LiveStore};
use std::{cell::RefCell, collections::HashMap};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Face {
    pub base: Option<String>,
    pub appearance: Option<u16>,
    pub gender: Option<u8>,
    pub name: String,
}

struct Person {
    face: Face,
    speaker: Speaker,
}

/// Lives only while rebuilding one window's bounded event rows. A later refresh
/// resolves the current report again, including renamed or disappeared objects.
pub(crate) struct People<'a> {
    store: &'a LiveStore,
    objects: RefCell<HashMap<String, Option<Person>>>,
}

impl<'a> People<'a> {
    pub(crate) fn new(store: &'a LiveStore) -> Self {
        Self { store, objects: RefCell::new(HashMap::new()) }
    }

    fn with<T>(&self, id: &str, read: impl FnOnce(Option<&Person>) -> T) -> T {
        let mut objects = self.objects.borrow_mut();
        if !objects.contains_key(id) {
            let person = self.store.locate(id).map(|(_, _, o)| Person {
                face: Face {
                    base: o.portrait.clone(),
                    appearance: o.app,
                    gender: crate::live_portrait::gender(&o),
                    name: crate::live_panel::name(&o).to_owned(),
                },
                speaker: Speaker::object(&o),
            });
            objects.insert(id.to_owned(), person);
        }
        read(objects.get(id).unwrap().as_ref())
    }

    pub(crate) fn name(&self, id: &str) -> String {
        self.with(id, |o| o.map_or_else(|| id.to_owned(), |o| o.face.name.clone()))
    }

    pub(crate) fn face(&self, id: &str) -> Option<Face> {
        self.with(id, |o| o.map(|o| o.face.clone()))
    }

    pub(crate) fn speaker(&self, e: &Event) -> Option<Speaker> {
        (e.kind == "chat").then(|| {
            e.id.as_deref().map_or(Speaker::Server, |id| {
                self.with(id, |o| o.map_or(Speaker::Npc, |o| o.speaker))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_event_people_refresh_names_portraits_roles_and_missing_objects() {
        let store = LiveStore::default();
        let report = |seq, name, present| {
            json!({"v":1,"boot":"people","seq":seq,"areas":[
            {"area":"gate","full":true,"cre":if present {vec![
                json!({"id":"7","name":name,"portrait":"po_behold_","app":17}),
                json!({"id":"8","pc":true,"r":"player"}),
                json!({"id":"9","pc":true,"dm":true}),
                json!({"id":"a","possessed":true})]} else {vec![]}}]})
        };
        store.sync(&report(1, "Guard", true).to_string().into_bytes(), 1).unwrap();
        let event = |id: Option<&str>, kind: &str| Event {
            n: 1,
            at_ms: 1,
            kind: kind.into(),
            area: "gate".into(),
            id: id.map(str::to_owned),
            name: String::new(),
            target: None,
            data: json!({}),
        };
        let people = People::new(&store);
        for _ in 0..500 {
            assert_eq!(people.name("7"), "Guard");
            assert_eq!(people.face("7").unwrap().base.as_deref(), Some("po_behold_"));
            assert_eq!(people.face("7").unwrap().appearance, Some(17));
            assert_eq!(people.speaker(&event(Some("7"), "chat")), Some(Speaker::Npc));
        }
        assert_eq!(people.objects.borrow().len(), 1);
        for (id, role) in [
            (None, Speaker::Server),
            (Some("7"), Speaker::Npc),
            (Some("8"), Speaker::Player),
            (Some("9"), Speaker::Dm),
            (Some("a"), Speaker::Dm),
            (Some("ff"), Speaker::Npc),
        ] {
            let e = event(id, "chat");
            assert_eq!(people.speaker(&e), Some(role));
            assert_eq!(people.speaker(&e), crate::live_log::speaker(&store, &e));
        }
        assert_eq!(people.name("8"), "player");
        assert_eq!(people.name("9"), "9");
        assert_eq!(people.name("ff"), "ff");
        assert_eq!(people.face("ff"), None);
        assert_eq!(people.speaker(&event(Some("7"), "damage")), None);
        store.sync(&report(2, "Captain", true).to_string().into_bytes(), 2).unwrap();
        assert_eq!(people.name("7"), "Guard", "one refresh has a consistent display snapshot");
        assert_eq!(People::new(&store).name("7"), "Captain");
        store.sync(&report(3, "", false).to_string().into_bytes(), 3).unwrap();
        assert_eq!(People::new(&store).face("7"), None);
        assert_eq!(People::new(&store).name("7"), "7");
    }
}
