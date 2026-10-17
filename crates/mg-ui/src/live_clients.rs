//! Named credentials in the Options draft. Applying restarts the service;
//! editing, rotating and removing here do not change the running credentials.
use mg_live::{
    LiveConfig,
    access::{ClientGroup, ClientToken, DeliveryMode},
};

pub(crate) fn error(config: &LiveConfig) -> Option<&'static str> {
    let mut names = std::collections::HashSet::from(["default".to_string(), "local".to_string()]);
    let mut tokens = std::collections::HashSet::new();
    if !config.token.is_empty() {
        tokens.insert(config.token.as_str());
    }
    if config.clients.len() > 64 {
        return Some("At most 64 named clients are allowed.");
    }
    for client in &config.clients {
        if client.name.is_empty()
            || client.name.chars().count() > 64
            || client.name.chars().any(char::is_control)
            || client.name.trim() != client.name
            || !names.insert(client.name.to_lowercase())
        {
            return Some(
                "Client names must be unique, 1–64 characters, without edge spaces or controls. default/local are reserved.",
            );
        }
        if !(32..=256).contains(&client.token.len())
            || !client.token.bytes().all(|b| b.is_ascii_graphic())
            || !tokens.insert(client.token.as_str())
        {
            return Some(
                "Generate a unique token for each client (32–256 printable ASCII characters, no spaces).",
            );
        }
        if !(1..=60_000).contains(&client.per_minute) {
            return Some("The command limit must be 1–60000 per minute.");
        }
    }
    if config.clients.iter().any(|c| tokens.iter().any(|t| c.name.contains(t))) {
        return Some("Client names must not contain credentials.");
    }
    None
}

pub(crate) fn ui(ui: &mut egui::Ui, config: &mut LiveConfig) {
    egui::CollapsingHeader::new("Named clients").show(ui, |ui| {
        ui.weak("Each client gets its own token and permissions. Apply restarts Live with these changes.");
        ui.weak("The default token keeps full access. Named tokens are copied separately; automatic discovery uses the default token.");
        if config.clients.is_empty() {
            ui.label("No named clients. Add one to give an agent its own permissions.");
        }
        let mut remove = None;
        let mut failure = None;
        for (i, client) in config.clients.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.group(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        let label = ui.label("Client name");
                        ui.add(egui::TextEdit::singleline(&mut client.name).desired_width(190.))
                            .labelled_by(label.id);
                        if ui.button("Remove client").on_hover_text("Remove this client from the draft. Apply revokes its access.").clicked() { remove = Some(i); }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Commands / minute");
                        ui.add(egui::DragValue::new(&mut client.per_minute).range(1..=60_000))
                            .on_hover_text("Command attempts per 60-second window; reads and waits are unlimited");
                        if ui.button("Generate token").on_hover_text("Replace this client token in the draft. Apply activates it.").clicked() {
                            match mg_live::service::generate_token() {
                                Ok(token) => client.token = token,
                                Err(e) => failure = Some(e),
                            }
                        }
                        if ui.add_enabled(!client.token.is_empty(), egui::Button::new("Copy client token")).on_disabled_hover_text("Generate a token first.").clicked() { ui.ctx().copy_text(client.token.clone()); }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Agent speech");
                        ui.selectable_value(&mut client.delivery, DeliveryMode::Auto, "Auto")
                            .on_hover_text("Send permitted speech immediately.");
                        ui.selectable_value(&mut client.delivery, DeliveryMode::Drafts, "Drafts")
                            .on_hover_text("Hold speech and replies in Chat and Focus until the DM sends, edits or rejects them.");
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (group, label, help) in [
                            (ClientGroup::Read, "read", "Status, state, events, search and command results"),
                            (ClientGroup::ReadChat, "read_chat", "Chat and raw server logs; also needs read"),
                            (ClientGroup::Speech, "speech", "Speak and reply, and update NPC memory summaries; no combat, scripts or inventory control"),
                            (ClientGroup::Npc, "npc", "Creature and object commands"),
                            (ClientGroup::Spawn, "spawn", "Create live objects"),
                            (ClientGroup::Scripts, "scripts", "Run module scripts; also needs local permission"),
                            (ClientGroup::Chunks, "chunks", "Run arbitrary NWScript; also needs local and server permission"),
                            (ClientGroup::Players, "players", "Player operations; also needs local and server permission"),
                            (ClientGroup::Variables, "variables", "Write application local variables"),
                        ] {
                            let mut checked = client.groups.contains(&group);
                            if ui.checkbox(&mut checked, label).on_hover_text(help).changed() {
                                client.groups.retain(|g| *g != group);
                                if checked { client.groups.push(group); }
                            }
                        }
                    });
                });
            });
        }
        if let Some(i) = remove { config.clients.remove(i); }
        if ui.add_enabled(config.clients.len() < 64, egui::Button::new("Add client")).on_disabled_hover_text("At most 64 named clients are supported.").on_hover_text("Create a client with a separate token; choose its permissions before Apply.").clicked() {
            match mg_live::service::generate_token() {
                Ok(token) => {
                    let n = (1..).find(|n| !config.clients.iter().any(|c| c.name.eq_ignore_ascii_case(&format!("Client {n}")))).unwrap();
                    config.clients.push(ClientToken { name: format!("Client {n}"), token, ..Default::default() });
                }
                Err(e) => failure = Some(e),
            }
        }
        if ui.add_enabled(config.clients.len()<64,egui::Button::new("Create conversation LLM client")).on_disabled_hover_text("At most 64 named clients are supported.").on_hover_text("Read, chat and speech only; drafts for DM approval, 60 commands/minute. Apply activates the client. Copy its separate token to the agent.").clicked() {
            match mg_live::service::generate_token() {
                Ok(token)=>{let n=(1..).find(|n|!config.clients.iter().any(|c|c.name.eq_ignore_ascii_case(&format!("Conversation {n}")))).unwrap();config.clients.push(ClientToken::conversation(format!("Conversation {n}"),token));}
                Err(e)=>failure=Some(e),
            }
        }
        if let Some(e) = failure {
            ui.data_mut(|d| d.insert_temp(egui::Id::new("live-client-token-error"), e));
        }
        if let Some(e) = ui.data(|d| d.get_temp::<String>(egui::Id::new("live-client-token-error"))) {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    });
    if let Some(error) = error(config) {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if let Some(dir) = crate::recovery::data_dir().map(|p| p.join("live"))
        && ui
            .link("Open audit folder")
            .on_hover_text(dir.join("audit.jsonl").display().to_string())
            .clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(crate::script_view::file_url(&dir)));
    }
}
