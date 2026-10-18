//! Transport-independent replication, cursors and bounded GUI notifications.
use mg_live::{
    gateway::{Push, Resume},
    gateway_frames::{self as frames, Ack, DecodedServer, Limits, ServerFrame, Welcome},
    store::LiveStore,
};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};

/// Deliberately contains no remote text, URLs, credentials or underlying errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("Gateway connection unavailable")]
    Transport,
    #[error("Gateway certificate does not match the saved fingerprint")]
    PinMismatch,
    #[error("Gateway credentials unavailable")]
    Credentials,
    #[error("Gateway protocol error")]
    Protocol,
    #[error("Gateway frame exceeds the negotiated limit")]
    TooLarge,
    #[error("Gateway request capacity reached")]
    Busy,
    #[error("Gateway request timed out; its outcome may be unknown")]
    Timeout,
    #[error("Gateway rejected the request")]
    Rejected,
    #[error("Gateway permission denied")]
    Forbidden,
    #[error("Gateway entry changed; reload before saving")]
    Conflict,
    #[error("Gateway operation is not supported")]
    Unsupported,
    #[error("Gateway session revoked")]
    Revoked,
    #[error("Gateway permissions changed; reconnect explicitly")]
    RoleChanged,
    #[error("Gateway protocol upgrade required")]
    Upgrade,
    #[error("Gateway session replaced")]
    Replaced,
    #[error("Gateway connection stopped")]
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkStatus {
    #[default]
    Connecting,
    Connected,
    Reconnecting,
    Stopped,
}

/// Auxiliary state that does not belong in the passive game store.
#[derive(Debug, Clone, Default)]
pub struct LinkSnapshot {
    pub status: LinkStatus,
    pub error: Option<LinkError>,
    pub welcome: Option<Welcome>,
    pub drafts_revision: u64,
    pub drafts: Vec<Value>,
    pub presence_revision: u64,
    pub actors: Vec<Value>,
    pub focus: BTreeMap<String, Vec<String>>,
    /// Changes for every book invalidation. Consumers invalidate their entire local cache.
    pub book_revision: u64,
    pub notices: VecDeque<(String, String)>,
    pub lost_events: u64,
    pub ignored_frames: u64,
    pub connections: u64,
    /// None until this connection has received confirmation of llm.allow.
    pub llm_allowed: Option<bool>,
}

#[derive(Default)]
pub(crate) struct Replica {
    pub snapshot: LinkSnapshot,
    pub resume: Option<Resume>,
    pub limits: Limits,
    results: BTreeMap<String, u64>,
    ack_dirty: bool,
    ack_items: usize,
}

fn limits(limits: &Limits) -> Result<Limits, LinkError> {
    if limits.frame == 0 || limits.book_frame == 0 || limits.in_flight == 0 {
        return Err(LinkError::Protocol);
    }
    Ok(Limits {
        frame: limits.frame.min(frames::CLIENT_FRAME_LIMIT),
        book_frame: limits.book_frame.min(frames::BOOK_FRAME_LIMIT),
        in_flight: limits.in_flight.min(64),
        watch: limits.watch.min(16),
    })
}

impl Replica {
    pub(crate) fn welcome(&mut self, store: &LiveStore, welcome: Welcome) -> Result<(), LinkError> {
        if welcome.proto != frames::PROTOCOL_VERSION
            || welcome.mirror != mg_live::MIRROR_VERSION
            || welcome.gw_epoch.is_empty()
            || (welcome.resumed
                && self.resume.as_ref().is_none_or(|r| r.gw_epoch != welcome.gw_epoch))
        {
            return Err(LinkError::Protocol);
        }
        let negotiated = limits(&welcome.limits)?;
        store.set_id_prefix(&welcome.id_prefix).map_err(|_| LinkError::Protocol)?;
        if !welcome.resumed {
            store.mirror_reset("gateway snapshot replaced").map_err(|_| LinkError::Protocol)?;
            self.resume = Some(Resume { gw_epoch: welcome.gw_epoch.clone(), ..Resume::default() });
            let connections = self.snapshot.connections;
            self.snapshot = LinkSnapshot { connections, ..Default::default() };
            self.results.clear();
            self.ack_dirty = false;
            self.ack_items = 0;
        }
        store.set_policy(welcome.policy);
        self.limits = negotiated;
        self.snapshot.status = LinkStatus::Connected;
        self.snapshot.error = None;
        self.snapshot.connections += 1;
        self.snapshot.welcome = Some(welcome);
        Ok(())
    }

    pub(crate) fn push(
        &mut self,
        store: &LiveStore,
        push: Push,
        now: u64,
    ) -> Result<(), LinkError> {
        if self.snapshot.status != LinkStatus::Connected {
            return Err(LinkError::Protocol);
        }
        let resume = self.resume.as_mut().ok_or(LinkError::Protocol)?;
        if let Push::Meta(meta) = &push
            && !resume.boot.is_empty()
            && meta.boot != resume.boot
        {
            return Err(LinkError::Protocol);
        }
        if let Push::Area(a) = &push
            && a.boot == resume.boot
            && resume.areas.get(&a.area).is_some_and(|rev| *rev >= a.rev)
        {
            return Ok(());
        }
        push.apply(store, now).map_err(|_| LinkError::Protocol)?;
        match push {
            Push::Meta(m) => resume.boot = m.boot,
            Push::Area(a) => {
                resume.areas.insert(a.area, a.rev);
            }
            Push::AreaGone { area, .. } => {
                resume.areas.remove(&area);
            }
            Push::Boot { boot, .. } => {
                resume.boot = boot;
                resume.areas.clear();
                resume.event_n = 0;
                self.results.clear();
                self.ack_dirty = false;
                self.ack_items = 0;
                self.snapshot.drafts.clear();
                self.snapshot.drafts_revision = self.snapshot.drafts_revision.wrapping_add(1);
                self.snapshot.book_revision = self.snapshot.book_revision.wrapping_add(1);
            }
            Push::Events(e) => {
                resume.event_n = resume.event_n.max(e.through);
                self.snapshot.lost_events = self.snapshot.lost_events.saturating_add(e.lost);
                self.ack_dirty = true;
                self.ack_items = self.ack_items.saturating_add(e.rows.len().max(1));
            }
            Push::Result(r) => {
                self.results
                    .entry(r.id)
                    .and_modify(|rev| *rev = (*rev).max(r.rev))
                    .or_insert(r.rev);
                self.ack_dirty = true;
                self.ack_items += 1;
            }
            Push::Drafts { rev, items } => {
                self.snapshot.drafts_revision = rev;
                self.snapshot.drafts = items;
            }
            Push::Presence { rev, actors, focus } => {
                self.snapshot.presence_revision = rev;
                self.snapshot.actors = actors;
                self.snapshot.focus = focus;
            }
            Push::Book { .. } => {
                self.snapshot.book_revision = self.snapshot.book_revision.wrapping_add(1);
            }
            Push::Session { groups, policy, limits: value } => {
                let parsed: Limits =
                    serde_json::from_value(value).map_err(|_| LinkError::Protocol)?;
                self.limits = limits(&parsed)?;
                store.set_policy(policy);
                if let Some(w) = &mut self.snapshot.welcome {
                    w.groups = groups;
                    w.policy = policy;
                    w.limits = self.limits.clone();
                }
            }
            Push::Notice { level, text } => {
                if self.snapshot.notices.len() == 128 {
                    self.snapshot.notices.pop_front();
                }
                self.snapshot.notices.push_back((level, text));
            }
        }
        Ok(())
    }

    pub(crate) fn ack(&mut self, periodic: bool) -> Option<Ack> {
        if !self.ack_dirty || (!periodic && self.ack_items < 32) {
            return None;
        }
        self.ack_dirty = false;
        self.ack_items = 0;
        Some(Ack {
            results: std::mem::take(&mut self.results).into_iter().collect(),
            event_n: self.resume.as_ref().map_or(0, |r| r.event_n),
        })
    }
}

/// Unknown variants are filtered before the shared decoder, whose compatibility
/// warning includes remote strings. Never let those strings become log messages.
pub(crate) fn decode(bytes: &[u8], secret: &str) -> Result<Option<ServerFrame>, LinkError> {
    if bytes.len() > frames::SERVER_FRAME_LIMIT {
        return Err(LinkError::TooLarge);
    }
    let mut value: Value = serde_json::from_slice(bytes).map_err(|_| LinkError::Protocol)?;
    let kind = value["t"].as_str().ok_or(LinkError::Protocol)?;
    let known = match kind {
        "welcome" | "res" | "bye" => true,
        "push" => matches!(
            value["topic"].as_str().ok_or(LinkError::Protocol)?,
            "meta"
                | "area"
                | "area_gone"
                | "boot"
                | "events"
                | "result"
                | "drafts"
                | "presence"
                | "book"
                | "session"
                | "notice"
        ),
        _ => false,
    };
    if !known {
        log::warn!("Skipping unknown gateway frame variant");
        return Ok(None);
    }
    redact(&mut value, secret);
    let bytes = serde_json::to_vec(&value).map_err(|_| LinkError::Protocol)?;
    match frames::decode_server(&bytes).map_err(|_| LinkError::Protocol)? {
        DecodedServer::Frame(frame) => Ok(Some(*frame)),
        DecodedServer::Ignored { .. } => Ok(None),
    }
}

fn redact(value: &mut Value, secret: &str) {
    if secret.is_empty() {
        return;
    }
    match value {
        Value::String(s) => *s = s.replace(secret, "[redacted]"),
        Value::Array(rows) => rows.iter_mut().for_each(|v| redact(v, secret)),
        Value::Object(map) => {
            for (key, mut v) in std::mem::take(map) {
                redact(&mut v, secret);
                map.insert(key.replace(secret, "[redacted]"), v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
