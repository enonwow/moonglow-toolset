//! User consent and bounded, payload-free agent presence, independent of game state.
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub const PRESENCE_MS: u64 = 60_000;
pub(crate) const DENIED: &str = "LLM access is off in Moonglow (Live > Allow LLM)";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentPresence {
    pub client: String,
    /// Unix milliseconds of the last authenticated request.
    pub last_ms: u64,
    /// Canonical operation name, never arguments or credentials.
    pub op: String,
}

#[derive(Debug, Default)]
struct State {
    allowed: bool,
    clients: BTreeMap<String, AgentPresence>,
}

#[derive(Debug, Default)]
pub(crate) struct Agents {
    state: Mutex<State>,
    revision: AtomicU64,
    wake: AtomicBool,
}
impl Agents {
    pub(crate) fn set_allowed(&self, allowed: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.allowed != allowed {
            self.revision.fetch_add(1, Ordering::Release);
        }
        state.allowed = allowed;
        if !allowed {
            state.clients.clear();
        }
    }

    pub(crate) fn allowed(&self) -> bool {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).allowed
    }

    pub(crate) fn request(&self, client: &str, op: &str, now: u64) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.allowed {
            return false;
        }
        // Only configured, authenticated client names enter this map (at most 65).
        let visible_change = state.clients.get(client).is_none_or(|p| {
            now.saturating_sub(p.last_ms) >= PRESENCE_MS || (op != "ping" && p.op != op)
        });
        let entry = state.clients.entry(client.into()).or_insert_with(|| AgentPresence {
            client: client.into(),
            last_ms: now,
            op: String::new(),
        });
        entry.last_ms = now;
        if op != "ping" || entry.op.is_empty() {
            entry.op.clear();
            entry.op.push_str(op);
        }
        self.revision.fetch_add(1, Ordering::Release);
        if visible_change {
            self.wake.store(true, Ordering::Release);
        }
        true
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
    pub(crate) fn take_wake(&self) -> bool {
        self.wake.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn recent(&self, now: u64) -> Vec<AgentPresence> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .clients
            .values()
            .filter(|p| now.saturating_sub(p.last_ms) < PRESENCE_MS)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consent_presence_and_expiration() {
        let agents = Agents::default();
        assert!(!agents.request("default", "ping", 10));
        agents.set_allowed(true);
        assert!(agents.request("default", "live_status", 100));
        assert!(agents.take_wake());
        let revision = agents.revision();
        assert!(agents.request("default", "ping", 100));
        assert!(!agents.take_wake());
        assert!(agents.revision() > revision);
        assert_eq!(agents.recent(100)[0].op, "live_status");
        assert!(agents.request("campaign", "ping", 200));
        assert_eq!(agents.recent(60_099).len(), 2);
        assert_eq!(agents.recent(60_100)[0].client, "campaign");
        assert!(agents.recent(60_200).is_empty());
        agents.set_allowed(false);
        assert!(agents.recent(200).is_empty());
        agents.set_allowed(true);
        assert!(agents.recent(200).is_empty(), "old connections must not reappear");
    }
}
