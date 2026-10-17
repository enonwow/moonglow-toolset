//! Opt-in replay diagnostics. No listener, Redis connection or command sender.
use super::*;
use crate::access::{Access, ClientToken};

#[derive(Debug)]
pub struct ReplayClient {
    access: Arc<Access>,
    name: String,
    drafts: Mutex<HashMap<String, String>>,
    native: Mutex<HashMap<String, String>>,
}
impl ReplayClient {
    /// A recorded proposal: {id, command:{area,op,...}} (id is the logical replay key).
    pub fn submit(&self, store: &LiveStore, input: Value, now: u64) -> Result<Value, String> {
        let mut command: Command =
            serde_json::from_value(input.get("command").unwrap_or(&input).clone())
                .map_err(|e| e.to_string())?;
        let key =
            input["id"].as_str().filter(|id| !id.is_empty()).unwrap_or(&command.id).to_owned();
        if key.is_empty() {
            return Err("Replay draft requires a stable id".into());
        }
        command.id.clear();
        let scope = self.access.scope_named(&self.name).expect("fixture client");
        let op = serde_json::to_value(&command).map_err(|e| e.to_string())?;
        if !scope.allows(op["op"].as_str().unwrap_or_default()) {
            return Err("Replay client scope forbids this operation".into());
        }
        let id = store.enqueue_scoped(command, now, &scope).map_err(|e| e.to_string())?;
        self.drafts.lock().unwrap().insert(key, id.clone());
        Ok(json!({"queued":id}))
    }
    /// {id,text:null|string,native_id?:recorded approved command id}. Null rejects.
    pub fn decision(&self, store: &LiveStore, input: Value, now: u64) -> Result<(), String> {
        let key = input["id"].as_str().ok_or("Replay decision id missing")?;
        let id = self.drafts.lock().unwrap().remove(key).ok_or("Replay draft not found")?;
        let native =
            store.decide_draft(&id, input["text"].as_str(), now).map_err(|e| e.to_string())?;
        if input["text"].is_string()
            && let Some(recorded) = input["native_id"].as_str()
        {
            self.native.lock().unwrap().insert(recorded.into(), native);
        }
        Ok(())
    }
    /// Preserve recorded acknowledgements after allocating fresh replay command ids.
    pub fn translate_results(&self, packet: &mut Value) {
        let mut ids = self.native.lock().unwrap();
        let mut done = Vec::new();
        for result in packet["results"].as_array_mut().into_iter().flatten() {
            if let Some(old) = result["id"].as_str().map(str::to_owned)
                && let Some(new) = ids.get(&old)
            {
                result["id"] = new.clone().into();
                done.push(old);
            }
        }
        for ack in packet["ack"].as_array_mut().into_iter().flatten() {
            if let Some(old) = ack.as_str().map(str::to_owned)
                && let Some(new) = ids.get(&old)
            {
                *ack = new.clone().into();
                done.push(old);
            }
        }
        for old in done {
            ids.remove(&old);
        }
    }
}
impl LiveStore {
    pub fn measure_event_packet(&self, body: &[u8], now: u64) -> Result<bool, String> {
        self.event_packet(body, now)
    }
    pub fn measure_replay_client(&self, name: &str) -> ReplayClient {
        let client = ClientToken::conversation(name, "r".repeat(32));
        let access = Arc::new(Access::new("d".repeat(32), &[client]).expect("fixture access"));
        self.set_assistant_access(access.clone());
        ReplayClient {
            access,
            name: name.into(),
            drafts: Mutex::new(HashMap::new()),
            native: Mutex::new(HashMap::new()),
        }
    }
    pub fn measure_census(&self) -> Value {
        let s = self.lock();
        json!({"areas":s.areas.len(),"objects":s.areas.values().map(|a|a.all().count()).sum::<usize>(),
            "players":s.areas.values().flat_map(|a|a.creatures.iter()).filter(|o|o.pc).count(),
            "queue":s.queue.len(),"results":s.results.len(),"accepted":s.accepted.len(),"owners":s.owners.len(),
            "skill_waiting":s.skill_waiting.len(),"logs":s.logs.len(),"resync_pending":s.resync.is_some(),"resyncs":s.resyncs,
            "events":s.events.measure_census(),"assistant":s.assistant.measure_census()})
    }
}
