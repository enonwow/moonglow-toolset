//! Time/byte-bounded identities. The rejection floor is durable, not a tombstone per ID.
use crate::store::{CommandError, CommandResult, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub(crate) const HORIZON_MS: u64 = 24 * 60 * 60 * 1000;
pub(crate) const HISTORY_BYTES: usize = 16 * 1024 * 1024;
const TIMED: u64 = 1 << 63;
const FUTURE_MS: u64 = 300_000;

/// Wire-compatible decimal counter: a version bit, Unix milliseconds, 10 ordinal bits.
pub fn command_counter(now_ms: u64, ordinal: u16) -> Option<u64> {
    (now_ms < (TIMED >> 10) && ordinal < 1024)
        .then_some(TIMED | (now_ms << 10) | u64::from(ordinal))
}
pub fn command_issued_ms(id: &str) -> Option<u64> {
    let n = id.rsplit('.').next()?.parse::<u64>().ok()?;
    (n & TIMED != 0).then_some((n & !TIMED) >> 10)
}
pub(crate) fn boot_key(boot: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, boot.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) at: u64,
    pub(crate) fingerprint: Option<[u8; 32]>,
    pub(crate) owner: String,
    pub(crate) outcome: Option<CommandResult>,
}
impl Entry {
    pub(crate) fn new(id: &str, now: u64, fingerprint: [u8; 32], owner: String) -> Self {
        Self {
            at: command_issued_ms(id).unwrap_or(now),
            fingerprint: Some(fingerprint),
            owner,
            outcome: None,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Boundary {
    pub(crate) floor: u64,
    pub(crate) clock: u64,
    pub(crate) legacy_closed: bool,
}
#[derive(Debug, Clone)]
pub(crate) struct History {
    pub(crate) boundary: Boundary,
    pub(crate) entries: HashMap<(String, String), Entry>,
    order: BTreeMap<(u64, String, String), usize>,
    pub(crate) bytes: usize,
    pub(crate) budget: usize,
    pub(crate) removed: Vec<(String, String)>,
}
impl Default for History {
    fn default() -> Self {
        Self {
            boundary: Boundary::default(),
            entries: HashMap::new(),
            order: BTreeMap::new(),
            bytes: 0,
            budget: HISTORY_BYTES,
            removed: Vec::new(),
        }
    }
}
impl History {
    fn cost(boot: &str, id: &str, entry: &Entry) -> usize {
        // Include conservative per-row map/tree/string overhead, not just payload bytes.
        512 + boot.len() * 2
            + id.len() * 2
            + serde_json::to_vec(entry).expect("identity serializes").len()
    }
    pub(crate) fn remove(&mut self, boot: &str, id: &str) {
        if let Some(e) = self.entries.remove(&(boot.into(), id.into())) {
            self.removed.push((boot.into(), id.into()));
            self.bytes -=
                self.order.remove(&(e.at, boot.into(), id.into())).expect("indexed identity");
        }
    }
    fn prune(&mut self) {
        while let Some((at, boot, id)) = self.order.first_key_value().map(|(k, _)| k.clone()) {
            if at >= self.boundary.floor {
                break;
            }
            self.boundary.legacy_closed = true;
            self.remove(&boot, &id);
        }
    }
    pub(crate) fn advance(&mut self, now: u64) {
        self.boundary.clock = self.boundary.clock.max(now);
        self.boundary.floor =
            self.boundary.floor.max(self.boundary.clock.saturating_sub(HORIZON_MS));
        self.prune();
    }
    pub(crate) fn merge_boundary(&mut self, b: &Boundary) {
        self.boundary.floor = self.boundary.floor.max(b.floor);
        self.boundary.clock = self.boundary.clock.max(b.clock);
        self.boundary.legacy_closed |= b.legacy_closed;
        self.prune();
    }
    pub(crate) fn check(&self, boot: &str, id: &str, now: u64) -> Result<(), CommandError> {
        if let Some(at) = command_issued_ms(id) {
            if at < self.boundary.floor {
                return Err(expired());
            }
            if at > now.max(self.boundary.clock).saturating_add(FUTURE_MS) {
                return Err(CommandError::new(
                    ErrorCode::Invalid,
                    "Command ID is ahead of the server clock",
                ));
            }
        } else if self.boundary.legacy_closed
            && !self.entries.contains_key(&(boot.into(), id.into()))
        {
            return Err(CommandError::new(
                ErrorCode::Expired,
                "Legacy command ID horizon closed; use a timestamped ID",
            ));
        }
        Ok(())
    }
    /// Eviction advances a floor, so forgetting a row can never enable its execution again.
    pub(crate) fn insert(&mut self, boot: String, id: String, entry: Entry) {
        self.remove(&boot, &id);
        if entry.at < self.boundary.floor {
            return;
        }
        let cost = Self::cost(&boot, &id, &entry);
        self.order.insert((entry.at, boot.clone(), id.clone()), cost);
        self.entries.insert((boot, id), entry);
        self.bytes += cost;
        if self.bytes > self.budget {
            // Reclaim a batch: avoid a checkpoint/eviction storm at the byte boundary.
            while self.bytes > self.budget * 3 / 4 {
                let (at, boot, id) =
                    self.order.first_key_value().expect("nonempty history").0.clone();
                self.boundary.floor = self.boundary.floor.max(at.saturating_add(1));
                self.prune();
                if at == u64::MAX {
                    self.boundary.legacy_closed = true;
                    self.remove(&boot, &id);
                }
            }
        }
    }
    pub(crate) fn get(&self, boot: &str, id: &str) -> Option<&Entry> {
        self.entries.get(&(boot.into(), id.into()))
    }
    pub(crate) fn outcome(&mut self, boot: &str, id: &str, result: CommandResult) {
        if let Some(mut e) = self.get(boot, id).cloned() {
            e.outcome = Some(result);
            self.insert(boot.into(), id.into(), e);
        }
    }
}
pub(crate) fn expired() -> CommandError {
    CommandError::new(
        ErrorCode::Expired,
        "Command ID is outside the idempotency horizon; it will not run again",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timed_counter_version_and_bounds_are_unambiguous() {
        for now in [0, 1_800_000_000_000, (TIMED >> 10) - 1] {
            for ordinal in [0, 1023] {
                let n = command_counter(now, ordinal).unwrap();
                assert_eq!(command_issued_ms(&format!("test.1234abcd.{n}")), Some(now));
            }
        }
        assert_eq!(command_counter(TIMED >> 10, 0), None);
        assert_eq!(command_counter(0, 1024), None);
        assert_eq!(command_issued_ms("test.1234abcd.42"), None);
        assert_eq!(command_issued_ms("legacy"), None);
    }
    #[test]
    fn extreme_legacy_clock_cannot_loop_during_eviction() {
        let mut history = History { budget: 1, ..Default::default() };
        history.insert(
            "boot".into(),
            "legacy".into(),
            Entry::new("legacy", u64::MAX, [0; 32], "local".into()),
        );
        assert_eq!(history.bytes, 0);
        assert_eq!(history.check("boot", "legacy", u64::MAX).unwrap_err().code, ErrorCode::Expired);
    }
}
