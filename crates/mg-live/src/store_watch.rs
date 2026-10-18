//! Idempotent UI subscriptions, independent of game objects and command freshness.
//! Retain only our removals: explicit agent watches and server extras are not ours.
use super::{LiveStore, SyncRequest, Value, json, resref};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub(super) struct Watches {
    owners: BTreeMap<String, BTreeSet<String>>,
    pub(super) desired: Option<BTreeSet<String>>,
    managed: BTreeSet<String>,
    pub reported: Option<Vec<String>>,
    pub pending: Vec<Value>,
    pub error: Option<String>,
    force: bool,
    retry_after: u64,
    sent_ms: u64,
}

pub(super) fn validate(areas: &[String]) -> Result<(), String> {
    if areas.is_empty() || areas.len() > 64 {
        return Err("areas must contain 1-64 resrefs".into());
    }
    let mut seen = BTreeSet::new();
    for area in areas {
        resref(area)?;
        if !seen.insert(area.to_ascii_lowercase()) {
            return Err("Duplicate watched area".into());
        }
    }
    Ok(())
}

impl Watches {
    pub(super) fn restart(&mut self) {
        self.reported = None;
        self.error = None;
        // A different module cannot acknowledge removals of the previous module's areas.
        self.managed = self.desired.clone().unwrap_or_default();
        self.pending.clear();
        self.force = true;
        self.retry_after = 0;
    }
}

impl LiveStore {
    /// Union of the areas currently used by local views. Empty releases our watches.
    /// May be called before the first report. No area row or player consent is needed.
    pub fn set_watched_areas(&self, areas: &[String], now_ms: u64) -> Result<(), String> {
        self.set_watched_areas_owned("local", areas, now_ms)
    }

    /// Replace one owner's desired set without releasing another owner's watches.
    /// Admission includes player areas; excess requests leave all sets unchanged.
    pub fn set_watched_areas_owned(
        &self,
        owner: &str,
        areas: &[String],
        now_ms: u64,
    ) -> Result<(), String> {
        if self.is_mirror() {
            return Err("Use gateway watch.set for a mirror".into());
        }
        if !areas.is_empty() {
            validate(areas)?;
        }
        let requested: BTreeSet<_> = areas.iter().map(|s| s.to_ascii_lowercase()).collect();
        let mut s = self.lock();
        let desired: BTreeSet<_> = s
            .watch
            .owners
            .iter()
            .filter(|(name, _)| name.as_str() != owner)
            .flat_map(|(_, set)| set.iter().cloned())
            .chain(requested.iter().cloned())
            .collect();
        let players: BTreeSet<_> = s
            .areas
            .iter()
            .filter(|(_, area)| area.creatures.iter().any(|o| o.pc))
            .map(|(name, _)| name.clone())
            .collect();
        if desired.union(&players).count() > super::MAX_AREAS {
            return Err(
                "Watch limit exceeded: watched and player areas must fit in 64 areas".into()
            );
        }
        if requested.is_empty() {
            s.watch.owners.remove(owner);
        } else {
            s.watch.owners.insert(owner.into(), requested);
        }
        if s.watch.desired.as_ref() != Some(&desired) {
            s.watch.managed.extend(desired.iter().cloned());
            s.watch.desired = Some(desired);
            s.watch.retry_after = 0;
            s.watch.force = true;
        }
        let was_pending = !s.watch.pending.is_empty();
        if !s.boot.is_empty() {
            prepare(&mut s.watch, now_ms);
        }
        if !was_pending
            && !s.watch.pending.is_empty()
            && let Some(kick) = &s.kick
        {
            let _ = kick.send(());
        }
        Ok(())
    }

    /// Both Redis transports call this on connection establishment, including
    /// reconnects that complete between two GUI frames.
    pub(crate) fn resend_watched_areas(&self, now_ms: u64) {
        let mut s = self.lock();
        s.watch.pending.clear();
        s.watch.force = true;
        s.watch.retry_after = 0;
        if !s.boot.is_empty() {
            prepare(&mut s.watch, now_ms);
        }
        if let Some(kick) = &s.kick {
            let _ = kick.send(());
        }
    }
}

pub(super) fn receive(w: &mut Watches, r: &SyncRequest, fresh: bool, now: u64) {
    if fresh && let Some(areas) = &r.watched_areas {
        w.reported = Some(areas.iter().map(|s| s.to_ascii_lowercase()).collect());
    }
    w.pending.retain(|c| {
        let id = c["id"].as_str().unwrap_or_default();
        let result = r.results.iter().find(|r| r.id == id);
        let acknowledged = r.ack.iter().any(|ack| ack == id);
        if c["on"] == false && (result.is_some_and(|r| r.ok) || (result.is_none() && acknowledged))
        {
            for area in c["areas"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                if w.desired.as_ref().is_none_or(|d| !d.contains(area)) {
                    w.managed.remove(area);
                }
            }
        }
        if let Some(result) = result {
            w.error = (!result.ok).then(|| result.err.clone());
            if !result.ok {
                w.retry_after = now + 5000;
                w.force = true;
            }
            false
        } else {
            !acknowledged
        }
    });
    if !w.pending.is_empty() && now.saturating_sub(w.sent_ms) >= super::COMMAND_TTL_MS {
        w.pending.clear();
        w.force = true;
    }
}

pub(super) fn prepare(w: &mut Watches, now: u64) {
    let Some(desired) = &w.desired else { return };
    // Older game scripts do not advertise watch support; keep full-only compatibility.
    if w.reported.is_none() || !w.pending.is_empty() || now < w.retry_after {
        return;
    }
    let reported: BTreeSet<_> = w.reported.iter().flatten().cloned().collect();
    // Include removals even before a report: an older command may be in flight.
    let remove: Vec<_> = w.managed.difference(desired).take(64).cloned().collect();
    let (areas, on) = if !remove.is_empty() {
        (remove, false)
    } else {
        (desired.iter().filter(|a| w.force || !reported.contains(*a)).cloned().collect(), true)
    };
    if areas.is_empty() {
        w.force = false;
        return;
    }
    match crate::service::generate_token() {
        Ok(token) => {
            w.pending.push(json!({"id":format!("watch-{}", &token[..32]),"op":"watch_areas","areas":areas,"on":on}));
            w.sent_ms = now;
            if on {
                w.force = false;
            }
        }
        Err(e) => {
            w.error = Some(e);
            w.retry_after = now + 5000;
        }
    }
}
