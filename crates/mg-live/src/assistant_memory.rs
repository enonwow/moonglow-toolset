//! Bounded conversation memory with a coalesced, off-store-lock disk writer.
use super::cards;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{SyncSender, sync_channel},
    },
    thread::JoinHandle,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub at_ms: u64,
    pub speaker: String,
    pub text: String,
    pub area: String,
    /// Exact recipient or reported proximity; neither proves client delivery.
    pub context: String,
    pub by: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Conversation {
    pub npc: String,
    pub npc_name: String,
    pub player: String,
    pub player_name: String,
    pub summary: String,
    pub turns: VecDeque<Turn>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Memory {
    pub conversations: BTreeMap<String, Conversation>,
}
#[derive(Debug, Default)]
struct Shared {
    data: Mutex<(u64, Memory)>,
    saved: Mutex<(u64, Option<String>)>,
    changed: Condvar,
}
#[derive(Debug)]
pub(super) struct Book {
    shared: Arc<Shared>,
    wake: Option<SyncSender<()>>,
    worker: Option<JoinHandle<()>>,
}
impl Book {
    #[cfg(feature = "measure")]
    pub(super) fn measure_census(&self) -> serde_json::Value {
        let data = self.shared.data.lock().unwrap_or_else(|e| e.into_inner());
        let saved = self.shared.saved.lock().unwrap_or_else(|e| e.into_inner());
        serde_json::json!({"pairs":data.1.conversations.len(),"turns":data.1.conversations.values().map(|c|c.turns.len()).sum::<usize>(),
            "text_bytes":data.1.conversations.values().map(|c|c.summary.len()+c.turns.iter().map(|t|t.text.len()).sum::<usize>()).sum::<usize>(),
            "revision":data.0,"saved_revision":saved.0,"writer_pending":data.0!=saved.0,"writer_error":saved.1})
    }

    pub(super) fn open(path: PathBuf) -> Result<Self, String> {
        let data = cards::read(&path)?;
        let shared = Arc::new(Shared { data: Mutex::new((0, data)), ..Default::default() });
        let (wake, receive) = sync_channel(1);
        let worker_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("live-memory".into())
            .spawn(move || {
                while receive.recv().is_ok() {
                    let (revision, snapshot) =
                        worker_shared.data.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    let error = cards::write(&path, &snapshot).err();
                    *worker_shared.saved.lock().unwrap_or_else(|e| e.into_inner()) =
                        (revision, error);
                    worker_shared.changed.notify_all();
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { shared, wake: Some(wake), worker: Some(worker) })
    }
    pub(super) fn revision(&self) -> u64 {
        self.shared.data.lock().unwrap_or_else(|e| e.into_inner()).0
    }
    pub(super) fn error(&self) -> Option<String> {
        self.shared.saved.lock().unwrap_or_else(|e| e.into_inner()).1.clone()
    }
    pub(super) fn read(&self, npc: &str, player: Option<&str>, limit: usize) -> Vec<Conversation> {
        let data = self.shared.data.lock().unwrap_or_else(|e| e.into_inner());
        data.1
            .conversations
            .values()
            .filter(|c| c.npc == npc && player.is_none_or(|p| p == c.player))
            .map(|c| {
                let mut c = c.clone();
                while c.turns.len() > limit.clamp(1, 200) {
                    c.turns.pop_front();
                }
                c
            })
            .collect()
    }
    pub(super) fn note(
        &self,
        npc: &str,
        npc_name: &str,
        player: Option<(&str, &str)>,
        summary: &str,
    ) -> Result<(), String> {
        cards::bounded(&[summary])?;
        let mut data = self.shared.data.lock().unwrap_or_else(|e| e.into_inner());
        let (player, player_name) = player.unwrap_or(("*", "All players"));
        let key = format!("{npc}\n{player}");
        if !data.1.conversations.contains_key(&key) && data.1.conversations.len() >= 1000 {
            return Err("Memory already has 1000 conversations".into());
        }
        let c = data.1.conversations.entry(key).or_insert_with(|| Conversation {
            npc: npc.into(),
            npc_name: npc_name.into(),
            player: player.into(),
            player_name: player_name.into(),
            ..Default::default()
        });
        c.summary = summary.into();
        data.0 += 1;
        drop(data);
        self.changed();
        Ok(())
    }
    pub(super) fn append(
        &self,
        npc: &str,
        npc_name: &str,
        player: &str,
        player_name: &str,
        turn: Turn,
    ) {
        let mut data = self.shared.data.lock().unwrap_or_else(|e| e.into_inner());
        let key = format!("{npc}\n{player}");
        if !data.1.conversations.contains_key(&key) && data.1.conversations.len() >= 1000 {
            return;
        }
        let c = data.1.conversations.entry(key).or_insert_with(|| Conversation {
            npc: npc.into(),
            npc_name: npc_name.into(),
            player: player.into(),
            player_name: player_name.into(),
            ..Default::default()
        });
        c.turns.push_back(turn);
        while c.turns.len() > 200 {
            c.turns.pop_front();
        }
        // Keep at most 10,000 turns across the server, removing the oldest first.
        while data.1.conversations.values().map(|c| c.turns.len()).sum::<usize>() > 10000 {
            let oldest = data
                .1
                .conversations
                .iter()
                .filter_map(|(k, c)| c.turns.front().map(|t| (k.clone(), t.at_ms)))
                .min_by_key(|(_, t)| *t);
            if let Some((key, _)) = oldest {
                data.1.conversations.get_mut(&key).unwrap().turns.pop_front();
            } else {
                break;
            }
        }
        data.0 += 1;
        drop(data);
        self.changed();
    }
    fn changed(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
    }
    pub(super) fn flush(&self) -> Result<(), String> {
        let wanted = self.revision();
        self.changed();
        let mut saved = self.shared.saved.lock().unwrap_or_else(|e| e.into_inner());
        while saved.0 < wanted {
            saved = self.shared.changed.wait(saved).unwrap_or_else(|e| e.into_inner());
        }
        saved.1.clone().map_or(Ok(()), Err)
    }
}
impl Drop for Book {
    fn drop(&mut self) {
        self.wake.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
