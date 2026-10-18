//! The connection portion of Options > Live. Network and credential I/O stays off the UI thread.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use mg_live_link::credentials::{Backend, OsKeyring, StoreError, TokenStore};
use mg_live_link::{Error, Gateway, Login, SessionToken, TrustPrompt};

use super::OptionsDraft;
use crate::settings::LiveServer;

#[derive(Clone)]
struct Environment {
    backend: Arc<dyn Backend>,
    data_dir: PathBuf,
}

#[derive(Clone)]
struct SharedBackend(Arc<dyn Backend>);

impl Backend for SharedBackend {
    fn get(&self, id: &str) -> Result<Option<SessionToken>, StoreError> {
        self.0.get(id)
    }
    fn set(&self, id: &str, token: &SessionToken) -> Result<(), StoreError> {
        self.0.set(id, token)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.0.delete(id)
    }
}

enum Event {
    Browser(String),
    Account(bool),
    Trust(TrustPrompt),
    Failed(String),
    Removed,
    SignedOut,
}

#[derive(Default)]
struct Account {
    server: Option<LiveServer>,
    pending: Option<mpsc::Receiver<Event>>,
    signed_in: bool,
    status: String,
    trust: Option<TrustPrompt>,
    removed: bool,
}

fn state_id() -> egui::Id {
    egui::Id::new("live-gateway-account")
}
fn environment_id() -> egui::Id {
    egui::Id::new("live-gateway-credentials")
}

impl OptionsDraft {
    /// Supply an isolated backend for an embedding host or UI tests, before drawing Live.
    pub fn set_live_credential_backend(
        ctx: &egui::Context,
        backend: Arc<dyn Backend>,
        data_dir: PathBuf,
    ) {
        ctx.data_mut(|d| d.insert_temp(environment_id(), Environment { backend, data_dir }));
    }

    /// Draw the gateway connection panel independently of the legacy local controls.
    pub fn live_servers_ui(&mut self, ui: &mut egui::Ui) {
        self.live_servers_ui_status(ui, None);
    }

    pub(crate) fn live_servers_ui_status(
        &mut self,
        ui: &mut egui::Ui,
        runtime: Option<(&str, &str)>,
    ) {
        let shared = ui
            .ctx()
            .data_mut(|d| d.get_temp_mut_or_default::<Arc<Mutex<Account>>>(state_id()).clone());
        let environment =
            ui.ctx().data(|d| d.get_temp::<Environment>(environment_id())).or_else(|| {
                crate::recovery::data_dir()
                    .map(|data_dir| Environment { backend: Arc::new(OsKeyring), data_dir })
            });
        let mut account = shared.lock().unwrap();
        if let Some(receiver) = &account.pending {
            let events: Vec<_> = receiver.try_iter().collect();
            for event in events {
                match event {
                    Event::Browser(url) => {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        account.status = "Waiting for Discord sign-in in your browser…".into();
                    }
                    Event::Account(signed_in) => {
                        account.signed_in = signed_in;
                        account.status = "Disconnected".into();
                        account.pending = None;
                    }
                    Event::SignedOut => {
                        if let Some(server) = &account.server {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("live-signed-out"), server.id.clone())
                            });
                        }
                        account.signed_in = false;
                        account.status = "Disconnected".into();
                        account.pending = None;
                    }
                    Event::Trust(prompt) => {
                        account.trust = Some(prompt);
                        account.pending = None;
                        account.status = "Certificate needs your approval".into();
                    }
                    Event::Removed => {
                        if let Some(server) = &account.server {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("live-signed-out"), server.id.clone())
                            });
                        }
                        account.removed = true;
                        account.pending = None;
                    }
                    Event::Failed(error) => {
                        account.status = error;
                        account.pending = None;
                    }
                }
            }
        }
        let busy = account.pending.is_some();
        ui.heading("Live servers");
        ui.checkbox(&mut self.live.enabled, "Enable live mode");
        let overridden = self
            .live_servers
            .resolve()
            .filter(|r| r.server.id == "environment" || r.token.is_some());
        if let Some(resolved) = &overridden {
            ui.weak(format!("Gateway environment overrides are active: {}", resolved.server.url));
        }
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(170.);
                ui.add_enabled_ui(!busy, |ui| {
                    let mut select = None;
                    for server in &self.live_servers.servers {
                        if ui.selectable_label(server.selected, &server.name).clicked() { select = Some(server.id.clone()); }
                    }
                    if let Some(id) = select { self.live_servers.select(&id); }
                    ui.horizontal(|ui| {
                        if ui.button("Add server").clicked() { self.live_servers.add(); }
                        if ui.add_enabled(self.live_servers.selected().is_some(), egui::Button::new("Remove server")).clicked()
                            && let Some(server) = self.live_servers.selected().cloned() {
                            // Removing a saved entry also removes its local credential, on a worker.
                            if let Some(environment) = &environment {
                                start(&mut account, environment.clone(), server, Operation::Remove, ui.ctx());
                            }
                        }
                    });
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                let Some(server) = self.live_servers.servers.iter_mut().find(|s| s.selected) else {
                    ui.weak("Add a server to configure Live."); return;
                };
                let same = account.server.as_ref().is_some_and(|s| s.id == server.id && s.url == server.url && s.cert_pin == server.cert_pin);
                if !same && !busy && overridden.is_some() {
                    account.server = Some(server.clone());
                    account.signed_in = false;
                    account.status = "Disconnected".into();
                } else if !same && !busy {
                    account.signed_in = false;
                    account.trust = None;
                    if let Some(environment) = &environment {
                        start(&mut account, environment.clone(), server.clone(), Operation::Load, ui.ctx());
                    } else { account.status = "Application data directory unavailable".into(); }
                }
                let busy = account.pending.is_some();
                egui::Grid::new("gateway-fields").num_columns(2).spacing([10., 8.]).show(ui, |ui| {
                    let label = ui.label("Name");
                    ui.add_enabled(!busy, egui::TextEdit::singleline(&mut server.name).desired_width(280.)).labelled_by(label.id);
                    ui.end_row();
                    let label = ui.label("Gateway URL");
                    let changed = ui.add_enabled(!busy && !account.signed_in, egui::TextEdit::singleline(&mut server.url).desired_width(280.)).labelled_by(label.id).changed();
                    if changed { server.cert_pin = None; }
                    ui.end_row();
                    let status = runtime.filter(|(id,_)| *id == server.id && !busy && account.status == "Disconnected")
                        .map_or(account.status.as_str(), |(_,status)| status);
                    ui.label("Status"); ui.label(status); ui.end_row();
                    ui.label("Account");
                    ui.vertical(|ui| {
                        ui.label(if let Some(resolved) = &overridden {
                            if resolved.token.is_some() { "Development token from environment" } else { "Set MOONGLOW_GATEWAY_TOKEN for the development gateway" }
                        } else if account.signed_in { "Session saved on this device" } else { "Not signed in" });
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!busy && !account.signed_in && environment.is_some() && overridden.is_none(), egui::Button::new("Sign in with Discord")).clicked() {
                                start(&mut account, environment.clone().unwrap(), server.clone(), Operation::Login, ui.ctx());
                            }
                            if ui.add_enabled(!busy && account.signed_in && environment.is_some() && overridden.is_none(), egui::Button::new("Sign out")).clicked() {
                                start(&mut account, environment.clone().unwrap(), server.clone(), Operation::Logout, ui.ctx());
                            }
                        });
                    });
                    ui.end_row();
                    ui.label("HAK / module cache"); crate::live_resources::options(ui, &mut self.live_cache_mib); ui.end_row();
                    ui.label("My AI");
                    if ui.add_enabled(runtime.is_some_and(|(id, state)| id == server.id && state == "connected"), egui::Button::new("Manage AI"))
                        .on_disabled_hover_text("Connect to this server to manage your AIs.").clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("live-open-ai"), server.id.clone()));
                    }
                    ui.end_row();
                });
                if let Some(prompt) = account.trust.clone() {
                    ui.separator();
                    ui.label(format!("Verify the certificate with the server administrator: {}", prompt.origin));
                    ui.monospace(&prompt.sha256);
                    ui.horizontal(|ui| {
                        if ui.button("Trust certificate and sign in").clicked() {
                            server.cert_pin = Some(prompt.sha256);
                            account.trust = None;
                            start(&mut account, environment.clone().unwrap(), server.clone(), Operation::Login, ui.ctx());
                        }
                        if ui.button("Reject certificate").clicked() {
                            account.trust = None;
                            account.status = "Certificate rejected".into();
                        }
                    });
                }
                ui.add_space(10.);
                ui.weak("Sign-in saves a session on this device. Enable live mode and Apply to connect.");
                ui.weak("Account actions take effect immediately. Apply saves the server list.");
            });
        });
        // Removal finishes only after storage confirms deletion.
        if account.removed {
            account.removed = false;
            if let Some(server) = &account.server {
                self.live_servers.remove(&server.id);
            }
            account.server = None;
            account.status.clear();
        }
    }
}

/// Shares Options' isolated credential backend. The environment token is read-only
/// and never written to the OS store or fallback file.
pub(crate) fn start_gateway(
    ctx: &egui::Context,
    resolved: crate::settings::ResolvedGateway,
) -> mpsc::Receiver<Result<mg_live_link::LinkService, String>> {
    struct ReadBackend {
        inner: Arc<dyn Backend>,
        token: Option<SessionToken>,
    }
    impl Backend for ReadBackend {
        fn get(&self, id: &str) -> Result<Option<SessionToken>, StoreError> {
            if self.token.is_some() { Ok(self.token.clone()) } else { self.inner.get(id) }
        }
        fn set(&self, _: &str, _: &SessionToken) -> Result<(), StoreError> {
            Err(StoreError::Denied)
        }
        fn delete(&self, _: &str) -> Result<(), StoreError> {
            Err(StoreError::Denied)
        }
    }
    let environment = ctx.data(|d| d.get_temp::<Environment>(environment_id())).or_else(|| {
        crate::recovery::data_dir()
            .map(|data_dir| Environment { backend: Arc::new(OsKeyring), data_dir })
    });
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = (|| {
            let environment = environment.ok_or("Credential storage unavailable")?;
            let gateway = Gateway::new(&resolved.server.url, resolved.server.cert_pin.as_deref())
                .map_err(|e| e.to_string())?;
            let config = mg_live_link::LinkConfig::new(resolved.server.id, gateway);
            let tokens = TokenStore::new(
                ReadBackend { inner: environment.backend, token: resolved.token },
                &environment.data_dir,
            );
            let wake = ctx.clone();
            mg_live_link::LinkService::start(
                config,
                &tokens,
                Arc::new(move || crate::live_repaint::wake(&wake)),
            )
            .map_err(|error| match error {
                mg_live_link::StartError::Trust(Error::TrustRequired(_)) => {
                    "Certificate approval required: sign in from Options > Live first.".into()
                }
                error => error.to_string(),
            })
        })();
        let _ = tx.send(result);
        crate::live_repaint::wake(&ctx);
    });
    rx
}

#[derive(Clone, Copy)]
enum Operation {
    Load,
    Login,
    Logout,
    Remove,
}

fn start(
    account: &mut Account,
    environment: Environment,
    server: LiveServer,
    op: Operation,
    ctx: &egui::Context,
) {
    let (tx, rx) = mpsc::channel();
    account.pending = Some(rx);
    account.server = Some(server.clone());
    account.trust = None;
    account.status = match op {
        Operation::Login => "Starting sign-in…",
        _ => "Checking account…",
    }
    .into();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let store = TokenStore::new(SharedBackend(environment.backend), &environment.data_dir);
        let result = match op {
            Operation::Load => {
                store.get(&server.id).map(|t| t.is_some()).map_err(|e| e.to_string())
            }
            Operation::Logout | Operation::Remove => {
                store.delete(&server.id).map(|()| false).map_err(|e| e.to_string())
            }
            Operation::Login => {
                let login = Gateway::new(&server.url, server.cert_pin.as_deref())
                    .and_then(|g| Login::begin(g, Duration::from_secs(120)));
                match login {
                    Ok(login) => {
                        if tx.send(Event::Browser(login.browser_url().into())).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                        login.finish().map_err(|e| e.to_string()).and_then(|token| {
                            store.set(&server.id, &token).map(|()| true).map_err(|e| e.to_string())
                        })
                    }
                    Err(Error::TrustRequired(prompt)) => {
                        let _ = tx.send(Event::Trust(prompt));
                        ctx.request_repaint();
                        return;
                    }
                    Err(error) => Err(error.to_string()),
                }
            }
        };
        let event = match result {
            Ok(_) if matches!(op, Operation::Remove) => Event::Removed,
            Ok(_) if matches!(op, Operation::Logout) => Event::SignedOut,
            Ok(signed_in) => Event::Account(signed_in),
            Err(error) => Event::Failed(error),
        };
        let _ = tx.send(event);
        ctx.request_repaint();
    });
}

/// Resolve download credentials on the job worker, sharing the login store.
pub(crate) fn resource_access(
    ctx: &egui::Context,
    resolved: crate::settings::ResolvedGateway,
) -> Result<(Gateway, SessionToken), String> {
    let environment = ctx
        .data(|d| d.get_temp::<Environment>(environment_id()))
        .or_else(|| {
            crate::recovery::data_dir()
                .map(|data_dir| Environment { backend: Arc::new(OsKeyring), data_dir })
        })
        .ok_or("Credential storage unavailable")?;
    let token = match resolved.token {
        Some(token) => token,
        None => TokenStore::new(SharedBackend(environment.backend), &environment.data_dir)
            .get(&resolved.server.id)
            .map_err(|e| e.to_string())?
            .ok_or("Sign in to download resources")?,
    };
    let gateway = Gateway::new(&resolved.server.url, resolved.server.cert_pin.as_deref())
        .map_err(|e| e.to_string())?;
    Ok((gateway, token))
}

pub(crate) fn resource_data_dir(ctx: &egui::Context) -> Option<PathBuf> {
    ctx.data(|d| d.get_temp::<Environment>(environment_id()))
        .map(|e| e.data_dir)
        .or_else(crate::recovery::data_dir)
}
