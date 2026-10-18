//! Local DM review of agent speech. Drafts never enter the native command queue.
use super::{Command, CommandError, CommandResult, ErrorCode, LiveStore, Op, State, push_result};
use crate::access::{Access, DeliveryMode, Scope};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

#[path = "assistant_book.rs"]
mod book;
#[path = "assistant_cards.rs"]
pub mod cards;
#[path = "assistant_memory.rs"]
pub mod memory;
pub(super) fn capture(s: &mut State) {
    book::capture(s);
}

const MAX_DRAFTS: usize = 128;

#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    pub command: Command,
    pub client: String,
    pub boot: String,
    pub created_ms: u64,
    pub reply_to: Option<u64>,
    pub name: String,
    pub area: String,
    pub notice: Option<String>,
}
impl Draft {
    pub fn text(&self) -> &str {
        speech(&self.command.op).unwrap_or_default()
    }
    pub fn target(&self) -> Option<&str> {
        self.command.target()
    }
}

#[derive(Debug, Clone)]
struct Sending {
    draft: Draft,
    edited: bool,
    access: Arc<Access>,
}

#[derive(Debug, Default)]
pub(super) struct AssistantState {
    knowledge: Option<book::Knowledge>,
    knowledge_revision: u64,
    drafts: HashMap<String, Draft>,
    sending: HashMap<String, Sending>,
    revision: u64,
    access: Option<Arc<Access>>,
}

#[cfg(feature = "measure")]
impl AssistantState {
    pub(super) fn measure_census(&self) -> serde_json::Value {
        json!({"drafts":self.drafts.len(),"sending":self.sending.len(),"knowledge":self.knowledge.as_ref().map(|k|k.measure_census())})
    }
}

pub(crate) fn speech(op: &Op) -> Option<&str> {
    match op {
        Op::NpcSay { text, .. } | Op::ReplyToPlayer { text, .. } | Op::PcMessage { text, .. } => {
            Some(text)
        }
        _ => None,
    }
}
fn set_text(op: &mut Op, replacement: &str) {
    match op {
        Op::NpcSay { text, .. } | Op::ReplyToPlayer { text, .. } | Op::PcMessage { text, .. } => {
            *text = replacement.into()
        }
        _ => unreachable!("only speech can be staged"),
    }
}
fn answer(draft: &Draft, outcome: &str, pending: bool, ok: bool, err: &str) -> CommandResult {
    CommandResult {
        id: draft.command.id.clone(),
        ok,
        pending,
        err: err.into(),
        value: Some(json!({"draft":{"outcome":outcome,"client":draft.client,"text":draft.text(),
            "target":draft.target(),"reply_to":draft.reply_to}})),
        ..Default::default()
    }
}

/// Called after normal command/scope/policy/target validation and ID allocation,
/// before accepted/owners insertion. Returning true suppresses native enqueue/kick.
pub(super) fn stage(
    s: &mut State,
    command: &Command,
    scope: &Scope,
    now_ms: u64,
) -> Result<bool, CommandError> {
    if let Some(sending) = s.assistant.sending.get(&command.id) {
        if sending.draft.boot != s.boot {
            return Err(CommandError::new(
                ErrorCode::Expired,
                "The game restarted during approval",
            ));
        }
        if s.assistant.access.as_ref().is_none_or(|a| !Arc::ptr_eq(a, &sending.access)) {
            return Err(CommandError::new(
                ErrorCode::Forbidden,
                "Client permissions changed during approval; review again",
            ));
        }
    }
    if scope.delivery != DeliveryMode::Drafts || speech(&command.op).is_none() {
        return Ok(false);
    }
    if s.assistant.drafts.len() + s.assistant.sending.len() >= MAX_DRAFTS {
        return Err("At most 128 speech drafts may await review or delivery".into());
    }
    let target = command.target();
    let area = s.areas.iter().find(|(_, a)| target.is_some_and(|id| a.find(id).is_some()));
    let area_id = area.map(|(id, _)| id.clone()).unwrap_or_else(|| command.area.clone());
    let name = area
        .and_then(|(_, a)| target.and_then(|id| a.find(id)))
        .map(|(_, o)| o.name.clone())
        .unwrap_or_default();
    let reply_to = s
        .events
        .query(&crate::events::Query {
            kinds: vec!["chat".into()],
            object: target.map(str::to_owned),
            limit: 1,
            ..Default::default()
        })
        .last()
        .map(|e| e.n);
    let draft = Draft {
        command: command.clone(),
        client: scope.name.clone(),
        boot: s.boot.clone(),
        created_ms: now_ms,
        reply_to,
        name,
        area: area_id,
        notice: None,
    };
    let result =
        answer(&draft, "pending", true, true, "Waiting for the DM to Send, Edit or Reject");
    s.events.command(&command.area, &serde_json::to_value(result).unwrap_or_default(), now_ms);
    s.assistant.drafts.insert(command.id.clone(), draft);
    s.assistant.revision += 1;
    Ok(true)
}

/// Link the eventual native answer back to the original agent-visible draft ID.
/// Remove the link first, so push_result may safely recurse for that original ID.
pub(super) fn completed(
    s: &mut State,
    command: &Command,
    result: &CommandResult,
) -> Option<(Command, CommandResult)> {
    if result.pending {
        return None;
    }
    let sent = s.assistant.sending.remove(&command.id)?;
    s.assistant.revision += 1;
    let outcome = if !result.ok {
        "send_failed"
    } else if sent.edited {
        "edited"
    } else {
        "sent"
    };
    let mut original = answer(&sent.draft, outcome, false, result.ok, &result.err);
    original.code = result.code;
    original.obj.clone_from(&result.obj);
    if let Some(value) = original.value.as_mut() {
        value["native"] = serde_json::to_value(result).unwrap_or(Value::Null);
    }
    Some((sent.draft.command, original))
}

/// A game restart invalidates all not-yet-approved identities.
pub(super) fn cancel(s: &mut State, reason: &str) {
    let drafts = std::mem::take(&mut s.assistant.drafts);
    if !drafts.is_empty() {
        s.assistant.revision += 1;
    }
    for (_, draft) in drafts {
        let mut result = answer(&draft, "expired", false, false, reason);
        result.code = Some(ErrorCode::Expired);
        push_result(s, draft.command, result);
    }
}

pub(super) fn pending(s: &State, id: &str) -> Option<CommandResult> {
    s.assistant
        .drafts
        .get(id)
        .map(|d| answer(d, "pending", true, true, "Waiting for DM review"))
        .or_else(|| {
            s.assistant.sending.values().find(|d| d.draft.command.id == id).map(|d| {
                answer(&d.draft, "sending", true, true, "DM approved; waiting for the game")
            })
        })
}

impl LiveStore {
    /// Service hook: replace the current authenticated-client policy on reconfiguration.
    pub(crate) fn set_assistant_access(&self, access: Arc<Access>) {
        self.lock().assistant.access = Some(access);
    }
    pub(crate) fn draft_result_scoped(&self, id: &str, scope: &Scope) -> Option<CommandResult> {
        let s = self.lock();
        if !scope.unrestricted() && s.owners.get(id) != Some(&scope.name) {
            return None;
        }
        pending(&s, id)
    }
    pub fn draft_revision(&self) -> u64 {
        self.lock().assistant.revision
    }
    pub fn drafts(&self) -> Vec<Draft> {
        let mut out: Vec<_> = self.lock().assistant.drafts.values().cloned().collect();
        out.sort_by(|a, b| {
            a.created_ms.cmp(&b.created_ms).then_with(|| a.command.id.cmp(&b.command.id))
        });
        out
    }
    /// Trusted local UI only. No HTTP operation exposes this decision method.
    /// None rejects; Some(text) sends, including edits. Approval revalidates current
    /// client permissions, global policy and live target state through enqueue_scoped.
    pub fn decide_draft(
        &self,
        id: &str,
        text: Option<&str>,
        now_ms: u64,
    ) -> Result<String, CommandError> {
        let (native, scope, original_command) = {
            let mut s = self.lock();
            let mut draft =
                s.assistant.drafts.get(id).cloned().ok_or("Draft is no longer awaiting review")?;
            if draft.boot != s.boot {
                return Err("Draft belongs to an earlier game boot".into());
            }
            let Some(text) = text else {
                s.assistant.drafts.remove(id);
                s.assistant.revision += 1;
                let result = answer(&draft, "rejected", false, false, "Rejected by the DM");
                push_result(&mut s, draft.command, result);
                return Ok(id.into());
            };
            let current_area =
                s.areas.iter().find(|(_, a)| draft.target().is_some_and(|id| a.find(id).is_some()));
            if current_area.is_none_or(|(area, _)| area != &draft.area) {
                return Err("Draft target moved to another area or is no longer reported; reject and request a fresh proposal".into());
            }
            let access =
                s.assistant.access.clone().ok_or("No current client permissions are available")?;
            let mut scope = access
                .scope_named(&draft.client)
                .ok_or("The proposing client has been removed; reject this draft")?;
            let op = serde_json::to_value(&draft.command).unwrap_or_default();
            if !scope.allows(op["op"].as_str().unwrap_or_default()) {
                return Err(CommandError::new(
                    ErrorCode::Forbidden,
                    "The proposing client no longer has permission",
                ));
            }
            scope.delivery = DeliveryMode::Auto;
            let edited = text != draft.text();
            let original = draft.clone();
            set_text(&mut draft.command.op, text);
            draft.command.validate()?;
            let mut native = draft.command.clone();
            native.id = format!("draft-{}", &crate::service::generate_token()?[..32]);
            // Install the link before enqueue can wake the publisher.
            s.assistant.drafts.remove(id);
            s.assistant.sending.insert(native.id.clone(), Sending { draft, edited, access });
            s.assistant.revision += 1;
            // Keep the original submitted command for identical client retries.
            debug_assert_eq!(
                s.accepted.get(id),
                Some(&super::command_fingerprint(&original.command))
            );
            (native, scope, original.command)
        };
        match self.enqueue_scoped(native.clone(), now_ms, &scope) {
            Ok(queued) => Ok(queued),
            Err(error) => {
                let mut s = self.lock();
                if let Some(mut sending) = s.assistant.sending.remove(&native.id) {
                    if sending.draft.boot != s.boot {
                        let mut result = answer(
                            &sending.draft,
                            "expired",
                            false,
                            false,
                            "The game restarted during approval",
                        );
                        result.code = Some(ErrorCode::Expired);
                        push_result(&mut s, sending.draft.command, result);
                        s.assistant.revision += 1;
                        return Err(error);
                    }
                    // A failed send remains editable/rejectable, but never silently retried.
                    sending.draft.command = original_command;
                    sending.draft.notice = Some(error.to_string());
                    s.assistant.drafts.insert(id.into(), sending.draft);
                    s.assistant.revision += 1;
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
#[path = "assistant_tests.rs"]
mod tests;
