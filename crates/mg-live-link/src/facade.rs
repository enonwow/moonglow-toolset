//! Asynchronous counterparts of the GUI's non-mirror Live operations.
use crate::{LinkError, LinkService, LinkStatus, PendingCall};
use mg_live::store::{
    Command,
    assistant::{
        Draft,
        cards::{GmCard, NpcCard},
    },
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookKind {
    Npc,
    Gm,
    Note,
}
impl BookKind {
    fn wire(self) -> &'static str {
        match self {
            Self::Npc => "npc",
            Self::Gm => "gm",
            Self::Note => "note",
        }
    }
}

impl LinkService {
    pub fn is_connected(&self) -> bool {
        self.snapshot().status == LinkStatus::Connected
    }
    /// Stable server identity for editor cache keys; never a Redis endpoint.
    pub fn server_id(&self) -> &str {
        &self.config().server_id
    }
    pub fn draft_revision(&self) -> u64 {
        self.shared.replica.lock().unwrap_or_else(|e| e.into_inner()).snapshot.drafts_revision
    }
    pub fn drafts(&self) -> Result<Vec<Draft>, LinkError> {
        self.shared
            .replica
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .drafts
            .iter()
            .cloned()
            .map(|value| DraftWire::deserialize(value).map_err(|_| LinkError::Protocol))
            .collect()
    }
    pub fn decide_draft(&self, id: &str, text: Option<&str>) -> Result<PendingCall, LinkError> {
        self.request("draft.decide", json!({"id":id,"text":text}))
    }
    pub fn agents_revision(&self) -> u64 {
        self.shared.replica.lock().unwrap_or_else(|e| e.into_inner()).snapshot.presence_revision
    }
    /// Gateway actor records, including server-provided identities and focus.
    /// Unlike local API presence, these are not inferred from local HTTP requests.
    pub fn agents(&self) -> Vec<Value> {
        self.shared.replica.lock().unwrap_or_else(|e| e.into_inner()).snapshot.actors.clone()
    }
    pub fn set_allow_agents(&self, on: bool) -> Result<PendingCall, LinkError> {
        self.request("llm.allow", json!({"on":on}))
    }
    pub fn allow_agents(&self) -> Option<bool> {
        self.shared.replica.lock().unwrap_or_else(|e| e.into_inner()).snapshot.llm_allowed
    }
    pub fn set_focus(&self, objects: &[(String, String)]) -> Result<PendingCall, LinkError> {
        if objects.len() > 256 {
            return Err(LinkError::Protocol);
        }
        self.request("focus.set",json!({"objects":objects.iter().map(|(area,id)|json!({"area":area,"id":id})).collect::<Vec<_>>()}))
    }
    /// An invalidation counter, not an optimistic-concurrency revision for book.put.
    /// Use each fetched book entry's server revision as if_rev.
    pub fn book_revision(&self) -> u64 {
        self.shared.replica.lock().unwrap_or_else(|e| e.into_inner()).snapshot.book_revision
    }
    pub fn assistant_revision(&self) -> (u64, u64) {
        (self.book_revision(), self.shared.store.event_cursor())
    }
    pub fn npc_card(&self, npc: &str) -> Result<PendingCall, LinkError> {
        self.call("npc_card", json!({"npc":npc}))
    }
    pub fn gm_card(&self) -> Result<PendingCall, LinkError> {
        self.call("gm_card", json!({}))
    }
    pub fn npc_memory(
        &self,
        npc: &str,
        player: Option<&str>,
        limit: usize,
    ) -> Result<PendingCall, LinkError> {
        if !(1..=200).contains(&limit) {
            return Err(LinkError::Protocol);
        }
        let mut input = json!({"npc":npc,"limit":limit});
        if let Some(player) = player {
            input["player"] = json!(player);
        }
        self.call("npc_memory", input)
    }
    pub fn area_notes(&self, area: &str) -> Result<PendingCall, LinkError> {
        self.call("area_notes", json!({"area":area}))
    }
    /// Local cache identity from the mirrored object. It neither reads nor writes a book.
    pub fn object_note_key(&self, id: &str) -> Result<String, LinkError> {
        self.shared.store.object_note_key(id).map_err(|_| LinkError::Protocol)
    }
    /// Returns the area's note response; the GUI selects its entry for this object.
    pub fn object_note(&self, id: &str) -> Result<PendingCall, LinkError> {
        let (area, _, _) = self.shared.store.locate(id).ok_or(LinkError::Protocol)?;
        self.area_notes(&area)
    }
    pub fn book_put(
        &self,
        kind: BookKind,
        key: &str,
        value: Value,
        if_rev: u64,
    ) -> Result<PendingCall, LinkError> {
        self.request(
            "book.put",
            json!({"kind":kind.wire(),"key":key,"value":value,"if_rev":if_rev}),
        )
    }
    pub fn save_npc_card(
        &self,
        key: &str,
        card: &NpcCard,
        if_rev: u64,
    ) -> Result<PendingCall, LinkError> {
        self.book_put(
            BookKind::Npc,
            key,
            serde_json::to_value(card).map_err(|_| LinkError::Protocol)?,
            if_rev,
        )
    }
    /// The gateway supplies the GM book key; no local module path is sent.
    pub fn save_gm_card(
        &self,
        key: &str,
        card: &GmCard,
        if_rev: u64,
    ) -> Result<PendingCall, LinkError> {
        self.book_put(
            BookKind::Gm,
            key,
            serde_json::to_value(card).map_err(|_| LinkError::Protocol)?,
            if_rev,
        )
    }
    pub fn save_object_note(
        &self,
        key: &str,
        text: &str,
        if_rev: u64,
    ) -> Result<PendingCall, LinkError> {
        self.book_put(BookKind::Note, key, json!(text), if_rev)
    }
    pub fn events_since(
        &self,
        n: u64,
        limit: usize,
        kinds: &[String],
    ) -> Result<PendingCall, LinkError> {
        if !(1..=500).contains(&limit) {
            return Err(LinkError::Protocol);
        }
        self.request("events.since", json!({"n":n,"limit":limit,"kinds":kinds}))
    }
    /// Drain notifications for the GUI's Live log without synthesizing game events.
    pub fn take_notices(&self) -> Vec<(String, String)> {
        self.shared
            .replica
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .notices
            .drain(..)
            .collect()
    }
    pub fn ai_list(&self) -> Result<PendingCall, LinkError> {
        self.request("ai.list", json!({}))
    }
    pub fn ai_create(&self, input: Value) -> Result<PendingCall, LinkError> {
        self.request("ai.create", input)
    }
    pub fn ai_update(&self, input: Value) -> Result<PendingCall, LinkError> {
        self.request("ai.update", input)
    }
    pub fn ai_revoke(&self, id: &str) -> Result<PendingCall, LinkError> {
        self.request("ai.revoke", json!({"id":id}))
    }
}

// Draft currently derives Serialize in mg-live. Its remote derive avoids changing
// the P1-owned type and keeps the facade's return type compatible with GUI cards.
#[derive(Deserialize)]
#[serde(remote = "Draft")]
struct DraftWire {
    command: Command,
    client: String,
    boot: String,
    created_ms: u64,
    reply_to: Option<u64>,
    name: String,
    area: String,
    notice: Option<String>,
}

#[cfg(test)]
mod tests;
