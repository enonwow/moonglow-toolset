//! Gateway-owned AI credentials. Secrets exist only in the current creation view.
use crate::Moonglow;
use mg_live_link::{LinkError, LinkService, PendingCall};
use serde_json::{Value, json};

enum Operation {
    List,
    Create,
    Change,
}
#[derive(Default)]
pub(crate) struct Agents {
    pub open: bool,
    source: std::sync::Weak<mg_live::store::LiveStore>,
    items: Vec<Value>,
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
    fn poll(&mut self, link: &LinkService) {
        if let Some((op, call)) = &self.pending {
            match call.try_recv() {
                Ok(None) => return,
                Ok(Some(mut value)) => {
                    match op {
                        Operation::List => {
                            self.items = value["items"].as_array().cloned().unwrap_or_default();
                            self.loaded = true;
                        }
                        Operation::Create => {
                            // Do not put the response, token or a Debug rendering in any log/cache.
                            self.secret = value["token"].take().as_str().map(str::to_owned);
                            self.name.clear();
                            self.loaded = false;
                        }
                        Operation::Change => {
                            self.loaded = false;
                            self.notice = Some("Updated on gateway.".into());
                        }
                    }
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
                    let id = item["id"].as_str().unwrap_or_default().to_owned();
                    ui.push_id(&id, |ui| {
                        ui.group(|ui| {
                            ui.label(format!("Owner: {}", item["owner"].as_str().unwrap_or_default()));
                            let mut name = item["name"].as_str().unwrap_or_default().to_owned();
                            let label = ui.label("AI name");
                            if ui.add(egui::TextEdit::singleline(&mut name).char_limit(256)).labelled_by(label.id).changed() { item["name"] = json!(name); }
                            let mut on = item["llm_allowed"].as_bool().unwrap_or(false);
                            ui.add_enabled(false, egui::Checkbox::new(&mut on, "Allow LLM for this AI"))
                                .on_disabled_hover_text("This gateway does not advertise per-AI consent yet. Its older switch affects all your AIs.");
                            ui.weak("Per-AI consent and avatars require an updated gateway.");
                            ui.horizontal(|ui| {
                                if ui.button("Save AI").clicked() { action = Some(link.ai_update(json!({"id":id,"name":item["name"]}))); }
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
