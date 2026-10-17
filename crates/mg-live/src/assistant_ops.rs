//! Assistant operation descriptors, kept with their store implementation.
use super::{Handler, Operation, text};
use crate::{
    access::{ClientGroup, Scope},
    store::{CommandError, ErrorCode, LiveStore},
};
use serde_json::{Value, json};

pub(super) fn operations() -> Vec<Operation> {
    let npc =
        || text("Current NPC id, unique tag/resref, or saved card id tag:... / resref:...", 1, 260);
    let player = || text("Current player id or unique tag/resref from the report.", 1, 256);
    [
        ("live_gm_card","GM card","Read the module's DM-authored campaign guidance before assisting. Never reveal its private fields to players.",vec![],json!({})),
        ("live_npc_card","NPC card","Read the NPC's DM-authored identity, knowledge, secrets and voice before speaking as it.",vec!["npc"],json!({"npc":npc()})),
        ("live_npc_memory","NPC conversation memory","Read retained NPC/player turns and DM/agent summary. Nearby turns are estimates, not delivery receipts.",vec!["npc"],json!({"npc":npc(),"player":player(),"limit":{"type":"integer","minimum":1,"maximum":200,"description":"Recent turns per player (default 50)."}})),
        ("live_npc_memory_note","Update NPC memory summary","Save a bounded summary; this does not alter captured turns or DM cards.",vec!["npc","summary"],json!({"npc":npc(),"player":player(),"summary":text("Short summary, up to 4000 characters. Omit player for the NPC's overall summary.",0,4000)})),
    ].into_iter().map(|(name,title,description,required,properties)|Operation {
        handler:Handler::Assistant(name),descriptor:json!({"name":name,"group":"live","title":title,"description":description,
            "readOnly":name!="live_npc_memory_note","destructive":false,"input":{"type":"object","additionalProperties":false,"required":required,"properties":properties}})
    }).collect()
}
pub(super) fn call(
    name: &str,
    input: &Value,
    live: &LiveStore,
    scope: &Scope,
) -> Result<Value, CommandError> {
    if name.starts_with("live_npc_memory") && !scope.has(ClientGroup::ReadChat) {
        return Err(CommandError::new(
            ErrorCode::Forbidden,
            "Conversation memory requires read_chat",
        ));
    }
    let npc = input["npc"].as_str().unwrap_or_default();
    let player = input["player"].as_str();
    match name {
        "live_gm_card"=>live.gm_card().map(|card|json!({"card":card})).map_err(Into::into),
        "live_npc_card"=>live.npc_card(npc).map(|card|json!({"card":card})).map_err(Into::into),
        "live_npc_memory"=>live.npc_memory(npc,player,input["limit"].as_u64().unwrap_or(50) as usize)
            .map(|conversations|json!({"conversations":conversations,"persistence_error":live.assistant_error()})).map_err(Into::into),
        "live_npc_memory_note"=>{
            live.npc_memory_note(npc,player,input["summary"].as_str().unwrap_or_default())?;
            live.flush_npc_memory()?;
            Ok(json!({"saved":true}))
        },
        _=>Err("Unknown assistant operation".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{Access, ClientToken};
    use std::sync::atomic::AtomicBool;
    #[test]
    fn assistant_operations_enforce_memory_read_scope_and_summary_bounds() {
        let access = Access::new(
            "d".repeat(32),
            &[ClientToken {
                name: "Reader".into(),
                token: "r".repeat(32),
                groups: vec![ClientGroup::Read],
                ..Default::default()
            }],
        )
        .unwrap();
        let scope = access.scope_named("Reader").unwrap();
        let store = LiveStore::default();
        let stop = AtomicBool::new(false);
        let error = crate::operations::call_scoped(
            "live_npc_memory",
            json!({"npc":"tag:captain"}),
            &store,
            &stop,
            &scope,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Forbidden);
        assert!(
            crate::operations::call_scoped(
                "live_npc_memory_note",
                json!({"npc":"tag:captain","summary":"forbidden"}),
                &store,
                &stop,
                &scope
            )
            .is_err()
        );
        let note = operations()
            .into_iter()
            .find(|o| o.descriptor["name"] == "live_npc_memory_note")
            .unwrap();
        assert!(
            super::super::validate(
                &note.descriptor["input"],
                &json!({"npc":"a","summary":"s".repeat(4001)})
            )
            .is_err()
        );
        assert!(
            super::super::validate(
                &note.descriptor["input"],
                &json!({"npc":"a","summary":"A concise update"})
            )
            .is_ok()
        );
    }
}
