//! The same local-only approval controls beside Chat and Focus conversations.
use crate::Moonglow;
use mg_live::store::{LiveStore, Op, assistant::Draft};
use std::{collections::HashMap, sync::Weak};

#[derive(Debug, Default)]
pub struct LiveDrafts {
    source: Weak<LiveStore>,
    revision: Option<(u64, u64)>,
    rows: Vec<Draft>,
    editing: HashMap<String, String>,
    notice: Option<String>,
    pending: HashMap<String, mg_live_link::PendingCall>,
}
impl LiveDrafts {
    #[cfg(feature = "measure")]
    pub(crate) fn measure_cache(&self) -> serde_json::Value {
        serde_json::json!({"rows":self.rows.len(),"editing":self.editing.len()})
    }
    fn update(
        &mut self,
        store: &std::sync::Arc<LiveStore>,
        revision: (u64, u64),
        read: impl FnOnce() -> Vec<Draft>,
    ) {
        let same = self.source.upgrade().is_some_and(|old| std::sync::Arc::ptr_eq(&old, store));
        if same && self.revision == Some(revision) {
            return;
        }
        if !same {
            self.editing.clear();
            self.pending.clear();
            self.notice = None;
        }
        self.rows = read();
        self.editing.retain(|id, _| self.rows.iter().any(|r| &r.command.id == id));
        self.revision = Some(revision);
        self.source = std::sync::Arc::downgrade(store);
    }
}

fn matches(draft: &Draft, area: Option<&str>, targets: Option<&[String]>, dm: bool) -> bool {
    area.is_none_or(|a| a == draft.area)
        && targets.is_none_or(|ids| {
            ids.iter().any(|id| {
                draft.target() == Some(id.as_str())
                    || matches!(&draft.command.op,Op::ReplyToPlayer{speaker:Some(s),..} if s==id)
            })
        })
        && (!dm || matches!(draft.command.op, Op::ReplyToPlayer { .. } | Op::PcMessage { .. }))
}

pub(crate) fn reserved(
    app: &mut Moonglow,
    area: Option<&str>,
    targets: Option<&[String]>,
    dm: bool,
) -> f32 {
    let Some(store) = app.live.clone() else { return 0. };
    let mut state = std::mem::take(&mut app.live_drafts);
    state.update(&store, app.live_backend().draft_revision(), || app.live_backend().drafts());
    let height = if state.rows.iter().any(|d| matches(d, area, targets, dm)) { 280. } else { 0. };
    app.live_drafts = state;
    height
}

pub(crate) fn ui(
    app: &mut Moonglow,
    ui: &mut egui::Ui,
    area: Option<&str>,
    targets: Option<&[String]>,
    dm: bool,
) {
    let Some(store) = app.live.clone() else { return };
    let mut state = std::mem::take(&mut app.live_drafts);
    state.update(&store, app.live_backend().draft_revision(), || app.live_backend().drafts());
    let rows: Vec<_> = state.rows.iter().filter(|d| matches(d, area, targets, dm)).collect();
    if rows.is_empty() {
        app.live_drafts = state;
        return;
    }
    let mut actions = Vec::new();
    state.pending.retain(|_, call| match call.try_recv() {
        Ok(None) => true,
        Ok(Some(_)) => false,
        Err(error) => {
            state.notice = Some(error.to_string());
            false
        }
    });
    let reason = (!state.pending.is_empty()).then_some("Waiting for gateway draft decision.");
    ui.group(|ui| {
        if let Some(reason) = reason { ui.weak(reason); }
        ui.add_enabled_ui(reason.is_none(), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("LLM drafts ({})",rows.len()));
            if ui.button("Send all drafts").on_hover_text("Approve every draft shown in this conversation, including your edits.").clicked() {
                actions.extend(rows.iter().map(|d|(d.command.id.clone(),Some(state.editing.get(&d.command.id).cloned().unwrap_or_else(||d.text().into())))));
            }
        });
        ui.weak("Nothing is sent until you approve. Enter sends the top draft while pointing at this panel.");
        // Only the hovered conversation handles an unfocused Enter; text fields and
        // ordinary buttons retain their own keyboard behavior.
        if reason.is_none() && ui.ui_contains_pointer() && !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Enter)) {
            let d=rows[0];
            actions.push((d.command.id.clone(),Some(state.editing.get(&d.command.id).cloned().unwrap_or_else(||d.text().into()))));
        }
        let names: std::collections::HashMap<String, String> = rows
            .iter()
            .filter_map(|d| Some((d.client.clone(), app.live_backend().actor_name(&d.client)?)))
            .collect();
        egui::ScrollArea::vertical().id_salt(("llm-drafts",area,targets)).max_height(220.).show(ui,|ui| {
            for d in rows {
                ui.push_id(&d.command.id,|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(if d.name.is_empty(){d.target().unwrap_or("Conversation")}else{&d.name});
                        ui.weak(format!("by LLM · {}",names.get(&d.client).map_or(d.client.as_str(),String::as_str)));
                        if area.is_none() {ui.small(&d.area);}
                    });
                    if let Some(n)=d.reply_to {ui.small(format!("Conversation context: event {n}"));}
                    if let Some(text)=state.editing.get_mut(&d.command.id) {
                        let label=ui.label("Edit proposed message");
                        ui.add(egui::TextEdit::multiline(text).desired_rows(2).desired_width(f32::INFINITY)).labelled_by(label.id);
                    } else {ui.label(d.text());}
                    if let Some(error)=&d.notice {ui.colored_label(ui.visuals().error_fg_color,error);}
                    ui.horizontal(|ui| {
                        if ui.button("Send draft").on_hover_text("Approve this message; current permissions and target are checked again.").clicked() {
                            actions.push((d.command.id.clone(),Some(state.editing.get(&d.command.id).cloned().unwrap_or_else(||d.text().into()))));
                        }
                        if ui.button("Edit draft").clicked() {state.editing.entry(d.command.id.clone()).or_insert_with(||d.text().into());}
                        if ui.button("Reject draft").on_hover_text("Tell the agent that this proposal was rejected. Nothing is sent to the game.").clicked() {
                            actions.push((d.command.id.clone(),None));
                        }
                    });
                    ui.separator();
                });
            }
        });
        if let Some(error)=&state.notice {ui.colored_label(ui.visuals().error_fg_color,error);}
        });
    });
    for (id, text) in actions {
        if app.live_backend().is_gateway() {
            match app
                .live_backend()
                .link()
                .ok_or(mg_live_link::LinkError::Transport)
                .and_then(|link| link.decide_draft(&id, text.as_deref()))
            {
                Ok(call) => {
                    state.pending.insert(id, call);
                    state.notice = None;
                }
                Err(error) => state.notice = Some(error.to_string()),
            }
            continue;
        }
        match store.decide_draft(&id, text.as_deref(), mg_live::now_ms()) {
            Ok(_) => {
                state.notice = None;
                ui.ctx().request_repaint();
            }
            Err(e) => state.notice = Some(e.to_string()),
        }
    }
    app.live_drafts = state;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drafts_follow_conversation_area_member_and_dm_filters() {
        let draft=Draft {command:serde_json::from_value(serde_json::json!({"id":"p","op":"reply_to_player","player":"b","as":"a","text":"Hello"})).unwrap(),
            client:"Storyteller".into(),boot:"test".into(),created_ms:0,reply_to:None,name:"Traveller".into(),area:"gate".into(),notice:None};
        assert!(matches(&draft, None, None, false));
        assert!(matches(&draft, Some("gate"), Some(&["a".into()]), false));
        assert!(matches(&draft, Some("gate"), Some(&["b".into()]), true));
        assert!(!matches(&draft, Some("inn"), None, false));
        assert!(!matches(&draft, None, Some(&["c".into()]), false));
    }

    #[test]
    fn a_new_connection_rereads_drafts_even_with_the_same_revision_number() {
        let store = std::sync::Arc::new(LiveStore::default());
        let mut state = LiveDrafts::default();
        let mut reads = 0;
        state.update(&store, (1, 1), || {
            reads += 1;
            vec![]
        });
        state.update(&store, (1, 1), || {
            reads += 1;
            vec![]
        });
        // A restarted gateway counts draft revisions from the start again.
        state.update(&store, (2, 1), || {
            reads += 1;
            vec![]
        });
        assert_eq!(reads, 2);
    }
}
