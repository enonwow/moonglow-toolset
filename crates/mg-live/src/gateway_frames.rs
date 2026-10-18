//! JSON frames for moonglow-gw/1. Transport limits are checked before decoding.
use crate::{
    MIRROR_VERSION,
    gateway::{Push, Resume},
    store::Policy,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const PROTOCOL_VERSION: u32 = 1;
pub const CLIENT_FRAME_LIMIT: usize = 65_536;
pub const BOOK_FRAME_LIMIT: usize = 262_144;
pub const SERVER_FRAME_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub proto: u32,
    pub mirror: u32,
    pub client: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<Resume>,
    pub watch: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerIdentity {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserIdentity {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub avatar: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub frame: usize,
    pub book_frame: usize,
    pub in_flight: usize,
    pub watch: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self { frame: CLIENT_FRAME_LIMIT, book_frame: BOOK_FRAME_LIMIT, in_flight: 64, watch: 16 }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nwsync {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<crate::resource_files::ResourceGeneration>,
    pub haks_url: String,
    pub haks_manifest: String,
    pub module_manifest: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub proto: u32,
    pub mirror: u32,
    pub gw_epoch: String,
    pub now_ms: u64,
    pub server: ServerIdentity,
    pub user: UserIdentity,
    pub groups: Vec<String>,
    pub policy: Policy,
    pub id_prefix: String,
    pub limits: Limits,
    pub features: Vec<String>,
    pub nwsync: Option<Nwsync>,
    pub resumed: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u32,
    pub op: String,
    pub a: Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameError {
    pub code: String,
    pub msg: String,
}
impl FrameError {
    pub fn new(code: &str, msg: impl Into<String>) -> Self {
        Self { code: code.into(), msg: msg.into() }
    }
}
impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.msg)
    }
}
impl std::error::Error for FrameError {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawResponse", into = "RawResponse")]
pub struct Response {
    pub id: u32,
    pub result: Result<Value, FrameError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawResponse {
    id: u32,
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present_json")]
    r: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    err: Option<FrameError>,
}
fn present_json<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
impl TryFrom<RawResponse> for Response {
    type Error = String;
    fn try_from(raw: RawResponse) -> Result<Self, String> {
        let result = match (raw.ok, raw.r, raw.err) {
            (true, Some(r), None) => Ok(r),
            (false, None, Some(e)) => Err(e),
            _ => return Err("Inconsistent response outcome".into()),
        };
        Ok(Self { id: raw.id, result })
    }
}
impl From<Response> for RawResponse {
    fn from(response: Response) -> Self {
        match response.result {
            Ok(r) => Self { id: response.id, ok: true, r: Some(r), err: None },
            Err(err) => Self { id: response.id, ok: false, r: None, err: Some(err) },
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ack {
    pub results: Vec<(String, u64)>,
    pub event_n: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bye {
    pub code: String,
    pub msg: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientFrame {
    Hello(Hello),
    Req(Request),
    Ack(Ack),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerFrame {
    Welcome(Welcome),
    Res(Response),
    Push(Push),
    Bye(Bye),
}
#[derive(Debug, Clone, PartialEq)]
pub enum DecodedServer {
    Frame(Box<ServerFrame>),
    Ignored { kind: String, topic: Option<String> },
}

fn parse(bytes: &[u8], limit: usize) -> Result<Value, FrameError> {
    if bytes.len() > limit {
        return Err(FrameError::new("too_large", "Frame exceeds byte limit"));
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| FrameError::new("invalid", e.to_string()))?;
    if !value.is_object() {
        return Err(FrameError::new("invalid", "Expected JSON object"));
    }
    Ok(value)
}
pub fn decode_client(bytes: &[u8]) -> Result<ClientFrame, FrameError> {
    let value = parse(bytes, BOOK_FRAME_LIMIT)?;
    let is_book = value["t"] == "req" && value["op"] == "book.put";
    if bytes.len() > CLIENT_FRAME_LIMIT && !is_book {
        return Err(FrameError::new("too_large", "Client frame exceeds byte limit"));
    }
    serde_json::from_value(value).map_err(|e| FrameError::new("invalid", e.to_string()))
}
pub fn decode_server(bytes: &[u8]) -> Result<DecodedServer, FrameError> {
    let value = parse(bytes, SERVER_FRAME_LIMIT)?;
    let kind =
        value["t"].as_str().ok_or_else(|| FrameError::new("invalid", "Missing frame type"))?;
    let topic = value["topic"].as_str();
    let known = matches!(kind, "welcome" | "res" | "bye")
        || kind == "push"
            && topic.is_some_and(|t| {
                matches!(
                    t,
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
                )
            });
    if kind == "push" && topic.is_none() {
        return Err(FrameError::new("invalid", "Missing push topic"));
    }
    if !known {
        log::warn!("Skipping unknown gateway frame type {kind}, topic {topic:?}");
        return Ok(DecodedServer::Ignored { kind: kind.into(), topic: topic.map(str::to_owned) });
    }
    let frame =
        serde_json::from_value(value).map_err(|e| FrameError::new("invalid", e.to_string()))?;
    Ok(DecodedServer::Frame(Box::new(frame)))
}

/// Connection-local request accounting, independent of a WebSocket implementation.
#[derive(Debug, Default)]
pub struct Connection {
    hello: bool,
    in_flight: BTreeSet<u32>,
    seen: BTreeSet<u32>,
    highest: Option<u32>,
}
impl Connection {
    pub fn receive(&mut self, frame: &ClientFrame) -> Result<(), FrameError> {
        match frame {
            ClientFrame::Hello(h) => {
                if self.hello {
                    return Err(FrameError::new("invalid", "Duplicate hello"));
                }
                if h.proto != PROTOCOL_VERSION || h.mirror != MIRROR_VERSION {
                    return Err(FrameError::new("upgrade", "Protocol or mirror version mismatch"));
                }
                self.hello = true;
            }
            _ if !self.hello => return Err(FrameError::new("invalid", "Hello must be first")),
            ClientFrame::Req(r) => {
                if self.seen.contains(&r.id)
                    || self.highest.is_some_and(|high| high >= 4096 && r.id <= high - 4096)
                {
                    return Err(FrameError::new("invalid", "Request ID reused on this connection"));
                }
                if self.in_flight.len() >= 64 {
                    return Err(FrameError::new("rate_limited", "Too many in-flight requests"));
                }
                let high = self.highest.map_or(r.id, |high| high.max(r.id));
                self.highest = Some(high);
                if high >= 4096 {
                    self.seen.retain(|id| *id > high - 4096);
                }
                self.seen.insert(r.id);
                self.in_flight.insert(r.id);
            }
            ClientFrame::Ack(_) => {}
        };
        Ok(())
    }
    pub fn complete(&mut self, id: u32) -> bool {
        self.in_flight.remove(&id)
    }
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    #[test]
    fn audit_m4_request_history_is_bounded_and_evicted_ids_stay_invalid() {
        let mut c = Connection { hello: true, ..Default::default() };
        for id in 0..10_000 {
            c.receive(&ClientFrame::Req(Request {
                id,
                op: "call".into(),
                a: serde_json::json!({}),
            }))
            .unwrap();
            c.complete(id);
        }
        assert!(c.seen.len() <= 4096);
        assert!(
            c.receive(&ClientFrame::Req(Request {
                id: 0,
                op: "call".into(),
                a: serde_json::json!({})
            }))
            .is_err()
        );
    }
}
