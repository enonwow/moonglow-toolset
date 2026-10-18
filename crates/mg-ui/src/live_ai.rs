//! Gateway-owned AI credentials. Secrets exist only in the current creation view.
use crate::Moonglow;
use mg_live_link::{LinkError, LinkService, PendingCall};
use serde::Deserialize;
use serde_json::{Value, json};

const INVALID_REPLY: &str = "Gateway returned an invalid AI response. Refresh or try again.";

#[derive(Deserialize)]
struct AiInfo {
    id: String,
    owner: String,
    name: String,
    #[serde(alias = "llm_allowed")]
    llm: bool,
}
impl AiInfo {
    fn valid(&self) -> bool {
        fn identity(value: &str, prefix: &str) -> bool {
            value.strip_prefix(prefix).is_some_and(|id| {
                !id.is_empty()
                    && id.len() <= 128
                    && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            })
        }
        identity(&self.id, "ai:")
            && identity(&self.owner, "dm:")
            && !self.name.trim().is_empty()
            && self.name.len() <= 256
            && !self.name.chars().any(char::is_control)
    }
}
// No Debug/Serialize implementation: a creation reply contains a one-time secret.
#[derive(Deserialize)]
struct AiCreateResponse {
    ai: AiInfo,
    token: String,
}
impl AiCreateResponse {
    fn parse(value: Value) -> Result<Self, ()> {
        let reply: Self = serde_json::from_value(value).map_err(|_| ())?;
        if !reply.ai.valid()
            || !reply.token.starts_with("mgai_")
            || reply.token.len() <= 5
            || reply.token.len() > 4096
            || !reply.token.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(());
        }
        Ok(reply)
    }
}
#[derive(Deserialize)]
#[serde(untagged)]
enum AiListResponse {
    Current(Vec<AiInfo>),
    Legacy { items: Vec<AiInfo> },
}
impl AiListResponse {
    fn parse(value: Value) -> Result<Vec<AiInfo>, ()> {
        let reply: Self = serde_json::from_value(value).map_err(|_| ())?;
        let (Self::Current(items) | Self::Legacy { items }) = reply;
        let mut ids = std::collections::BTreeSet::new();
        if items.len() > 256 || items.iter().any(|item| !item.valid() || !ids.insert(&item.id)) {
            return Err(());
        }
        Ok(items)
    }
}

#[derive(Clone, Copy)]
enum Operation {
    List,
    Create,
    Change,
}
#[derive(Default)]
pub(crate) struct Agents {
    pub open: bool,
    source: std::sync::Weak<mg_live::store::LiveStore>,
    items: Vec<AiInfo>,
    pending: Option<(Operation, PendingCall)>,
    loaded: bool,
    connection: u64,
    name: String,
    secret: Option<String>,
    notice: Option<String>,
}
impl Agents {
    fn start(&mut self, operation: Operation, result: Result<PendingCall, LinkError>) {
        match result {
            Ok(call) => {
                if !matches!(operation, Operation::List) {
                    self.notice = None;
                }
                self.pending = Some((operation, call));
            }
            Err(error) => self.notice = Some(error.to_string()),
        }
    }
    fn received(&mut self, op: Operation, value: Value) {
        let result = match op {
            Operation::List => AiListResponse::parse(value).map(|items| {
                self.items = items;
                self.loaded = true;
                if self.notice.as_deref() == Some(INVALID_REPLY) {
                    self.notice = None;
                }
            }),
            Operation::Create => AiCreateResponse::parse(value).map(|reply| {
                // Only the explicit one-time view retains the credential.
                self.secret = Some(reply.token);
                self.name.clear();
                self.loaded = false;
            }),
            Operation::Change => {
                self.loaded = false;
                self.notice = Some("Updated on gateway.".into());
                Ok(())
            }
        };
        if result.is_err() {
            // Never echo remote fields or serde diagnostics, and never retry a write.
            self.loaded = true;
            self.notice = Some(INVALID_REPLY.into());
        }
    }
    fn poll(&mut self, link: &LinkService) {
        if let Some((op, call)) = &self.pending {
            match call.try_recv() {
                Ok(None) => return,
                Ok(Some(value)) => {
                    self.received(*op, value);
                    self.pending = None;
                }
                Err(error) => {
                    self.pending = None;
                    self.loaded = true; // A failed create/update is never retried automatically.
                    self.notice = Some(error.to_string());
                }
            }
        }
        if self.pending.is_none() && !self.loaded {
            self.start(Operation::List, link.ai_list());
        }
    }
}
pub(crate) fn window(app: &mut Moonglow, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.live_ai);
    if let Some(id) = ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::new("live-open-ai"))) {
        state.open |= app.live_backend().server_id() == Some(id.as_str());
    }
    if !state.open {
        state.secret = None;
        state.pending = None;
        state.loaded = false;
        app.live_ai = state;
        return;
    }
    let backend = app.live_backend();
    let Some(link) = backend.link() else {
        app.live_ai = Agents::default();
        return;
    };
    let store = link.store();
    if !state.source.upgrade().is_some_and(|old| std::sync::Arc::ptr_eq(&old, &store)) {
        state =
            Agents { open: true, source: std::sync::Arc::downgrade(&store), ..Default::default() };
    }
    let connection = link.snapshot().connections;
    if state.connection != connection {
        state.connection = connection;
        state.loaded = false;
    }
    state.poll(link);
    let mut open = true;
    egui::Window::new("AI agents").open(&mut open).default_width(560.).show(ctx, |ui| {
        ui.weak("Manage your AIs. Administrators can manage all AIs. New AIs start with LLM access off.");
        if let Some(message) = &state.notice { ui.label(message); }
        if let Some(secret) = &state.secret {
            ui.group(|ui| {
                ui.strong("Save this token now. It will not be shown again.");
                ui.label(egui::RichText::new(secret).monospace());
                if ui.button("Copy AI token").clicked() { ui.ctx().copy_text(secret.clone()); }
            });
            if ui.button("I saved the token").clicked() { state.secret = None; }
        }
        if state.pending.is_some() { ui.spinner(); ctx.request_repaint_after(std::time::Duration::from_millis(50)); }
        ui.add_enabled_ui(state.pending.is_none(), |ui| {
            ui.horizontal(|ui| {
                let label = ui.label("New AI name");
                ui.add(egui::TextEdit::singleline(&mut state.name).char_limit(256)).labelled_by(label.id);
                if ui.add_enabled(!state.name.trim().is_empty() && state.secret.is_none(), egui::Button::new("Create AI")).clicked() {
                    state.start(Operation::Create, link.ai_create(json!({"name":state.name.trim()})));
                }
            });
            if ui.button("Refresh AI agents").clicked() { state.loaded = false; }
            let mut action = None;
            egui::ScrollArea::vertical().max_height(460.).show(ui, |ui| {
                for item in &mut state.items {
                    let id = item.id.clone();
                    ui.push_id(&id, |ui| {
                        ui.group(|ui| {
                            ui.label(format!("Owner: {}", item.owner));
                            let label = ui.label("AI name");
                            ui.add(egui::TextEdit::singleline(&mut item.name).char_limit(256)).labelled_by(label.id);
                            let mut on = item.llm;
                            ui.add_enabled(false, egui::Checkbox::new(&mut on, "Allow LLM for this AI"))
                                .on_disabled_hover_text("This gateway does not advertise per-AI consent yet. Its older switch affects all your AIs.");
                            ui.weak("Per-AI consent and avatars require an updated gateway.");
                            ui.horizontal(|ui| {
                                if ui.button("Save AI").clicked() { action = Some(link.ai_update(json!({"id":id,"name":item.name}))); }
                                if ui.button("Revoke AI").clicked() { action = Some(link.ai_revoke(&id)); }
                            });
                        });
                    });
                }
            });
            if let Some(result) = action { state.start(Operation::Change, result); }
        });
    });
    state.open = open;
    if !open {
        state.secret = None;
        state.pending = None;
        state.loaded = false;
    }
    app.live_ai = state;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_create_reply_is_an_error_not_a_panic_or_a_token() {
        for value in [json!(1), json!(null), json!([]), json!({}), json!({"token":false})] {
            let mut state = Agents { name: "Keep draft".into(), ..Default::default() };
            state.received(Operation::Create, value);
            assert!(state.notice.is_some(), "invalid create reply must be visible");
            assert!(state.secret.is_none());
            assert_eq!(state.name, "Keep draft");
            assert!(state.loaded, "a malformed write must not be retried");
        }
    }
    #[test]
    fn malformed_list_reply_is_an_error_instead_of_an_empty_success() {
        for value in [
            json!(1),
            json!({}),
            json!({"items":{}}),
            json!({"items":[1]}),
            json!({"items":[{"id":"ai:test"}]}),
        ] {
            let mut state = Agents::default();
            state.received(Operation::List, value);
            assert!(state.notice.is_some(), "invalid list reply must be visible");
            assert!(state.items.is_empty());
            assert!(state.loaded, "invalid lists must wait for explicit refresh");
        }
    }
    fn ai() -> Value {
        json!({"id":"ai:scribe","owner":"dm:test","name":"Scribe","llm":false})
    }
    #[test]
    fn valid_replies_and_invalid_list_refresh_preserve_the_expected_state() {
        let mut state = Agents { name: "Scribe".into(), ..Default::default() };
        state.received(Operation::Create, json!({"ai":ai(),"token":"mgai_fixture"}));
        assert_eq!(state.secret.as_deref(), Some("mgai_fixture"));
        assert!(state.name.is_empty());
        for value in [json!([ai()]), json!({"items":[ai()]})] {
            state.received(Operation::List, value);
            assert_eq!(state.items.len(), 1);
            assert_eq!(state.items[0].id, "ai:scribe");
            assert!(state.notice.is_none());
        }
        for value in
            [json!([ai(), ai()]), json!([ai(), 1]), json!({"items":1,"token":"mgai_fixture"})]
        {
            state.received(Operation::List, value);
            assert_eq!(state.items.len(), 1, "invalid replies cannot partially replace the list");
            assert_eq!(state.notice.as_deref(), Some(INVALID_REPLY));
            assert!(!state.notice.as_ref().unwrap().contains("mgai_fixture"));
        }
        state.received(Operation::List, json!([ai()]));
        assert!(state.notice.is_none(), "a valid explicit refresh clears the protocol error");
    }
    #[test]
    fn create_validates_identity_and_token_before_showing_a_secret() {
        for value in [
            json!({"ai":ai(),"token":1}),
            json!({"ai":1,"token":"mgai_fixture"}),
            json!({"ai":ai(),"token":""}),
            json!({"ai":ai(),"token":"mgai_\n"}),
        ] {
            let mut state = Agents::default();
            state.received(Operation::Create, value);
            assert_eq!(state.notice.as_deref(), Some(INVALID_REPLY));
            assert!(state.secret.is_none());
        }
    }
    #[test]
    fn list_validates_each_identity_name_and_consent_and_bounds_item_count() {
        for (field, invalid) in [
            ("id", json!("dm:wrong")),
            ("owner", json!("ai:wrong")),
            ("name", json!("")),
            ("name", json!("x".repeat(257))),
            ("llm", json!("yes")),
        ] {
            let mut row = ai();
            row[field] = invalid;
            assert!(AiListResponse::parse(json!([row])).is_err());
        }
        let rows: Vec<_> = (0..257)
            .map(|i| json!({"id":format!("ai:a{i}"),"owner":"dm:test","name":"AI","llm":false}))
            .collect();
        assert!(AiListResponse::parse(json!(rows)).is_err());
        let legacy = json!({"items":[{"id":"ai:scribe","owner":"dm:test","name":"Scribe","llm_allowed":false}]});
        assert!(AiListResponse::parse(legacy).is_ok());
    }
}
