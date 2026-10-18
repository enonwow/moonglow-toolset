//! The GUI's single runtime boundary. Reports keep the same LiveStore shape.
//!
//! `Moonglow::live_service` remains the local embedding/test injection point;
//! GUI code uses this view so choosing a server never duplicates a widget path.
use crate::Moonglow;
use mg_live::store::{Command, CommandError, LiveStore};
use mg_live_link::{LinkService, LinkStatus, PendingCall};
use serde_json::{Value, json};
use std::sync::{Arc, mpsc};

pub(crate) const SHARED_REASON: &str =
    "The local global LLM switch does not apply to gateway connections.";

pub(crate) struct GatewayRuntime {
    pub pending: Option<mpsc::Receiver<Result<LinkService, String>>>,
    pub link: Option<LinkService>,
    pub error: Option<String>,
    pub config: mg_live::LiveConfig,
    pub server_id: String,
    watches: Option<Vec<String>>,
    focus: Option<Vec<(String, String)>>,
    connection: u64,
    calls: Vec<PendingCall>,
}
impl GatewayRuntime {
    pub(crate) fn starting(
        server_id: String,
        receiver: mpsc::Receiver<Result<LinkService, String>>,
    ) -> Self {
        Self {
            pending: Some(receiver),
            link: None,
            error: None,
            server_id,
            config: mg_live::LiveConfig {
                enabled: true,
                allow_agents: false,
                ..Default::default()
            },
            watches: None,
            focus: None,
            connection: 0,
            calls: Vec::new(),
        }
    }
}

pub struct LiveBackend<'a> {
    app: &'a Moonglow,
}
impl std::fmt::Debug for LiveBackend<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveBackend")
            .field("gateway", &self.is_gateway())
            .field("active", &self.active())
            .finish()
    }
}
impl Moonglow {
    pub fn live_backend(&self) -> LiveBackend<'_> {
        LiveBackend { app: self }
    }
}
impl<'a> LiveBackend<'a> {
    pub(crate) fn context(&self) -> Option<std::path::PathBuf> {
        if self.is_gateway() {
            let id = self
                .app
                .live_gateway
                .as_ref()
                .map(|r| r.server_id.clone())
                .or_else(|| self.app.settings.live_servers.resolve().map(|r| r.server.id))?;
            let hash = id
                .bytes()
                .fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
            Some(std::path::PathBuf::from(format!("gateway-{hash:016x}")))
        } else {
            self.app.module_path()
        }
    }
    pub fn active(&self) -> bool {
        self.app.live_gateway.is_some() || self.app.live_service.is_some()
    }
    pub(crate) fn server_id(&self) -> Option<&str> {
        self.app.live_gateway.as_ref().map(|r| r.server_id.as_str())
    }
    pub fn is_gateway(&self) -> bool {
        self.app.live_gateway.is_some() || self.app.live.as_ref().is_some_and(|s| s.is_mirror())
    }
    pub fn config(&self) -> &'a mg_live::LiveConfig {
        if let Some(gateway) = &self.app.live_gateway {
            &gateway.config
        } else {
            self.app.live_service.as_ref().map_or(&self.app.settings.live, |s| s.config())
        }
    }
    pub(crate) fn link(&self) -> Option<&LinkService> {
        self.app.live_gateway.as_ref()?.link.as_ref()
    }
    pub fn store(&self) -> Option<Arc<LiveStore>> {
        self.app.live.clone()
    }
    pub fn shared_reason(&self) -> Option<&'static str> {
        self.is_gateway().then_some(SHARED_REASON)
    }
    pub fn allow_agents(&self) -> bool {
        !self.is_gateway() && self.config().allow_agents
    }
    pub(crate) fn panel_fraction(&self) -> f32 {
        // Gateway Focus also contains actor chips; leave its two columns and
        // composer usable in the default dock. Keep the local layout unchanged.
        if self.is_gateway() { 0.4 } else { 0.62 }
    }
    pub fn connection_status(&self) -> Value {
        if let Some(gateway) = &self.app.live_gateway {
            gateway.link.as_ref().map_or_else(|| json!({"connected":false,
                "state":if gateway.error.is_some(){"stopped"}else{"connecting"},"error":gateway.error}), |link| {
                let mut status = link.connection_status();
                status["state"] = json!(status["state"].as_str().unwrap_or("connecting").to_ascii_lowercase());
                status
            })
        } else if self.is_gateway() {
            json!({"connected":false,"state":"stopped","error":null})
        } else {
            self.app
                .live_service
                .as_ref()
                .map_or(Value::Null, mg_live::LiveService::connection_status)
        }
    }
    pub fn enqueue(&self, command: Command) -> Result<String, CommandError> {
        if let Some(link) = self.link() {
            link.enqueue(command)
        } else if self.is_gateway() {
            Err("Gateway is connecting".into())
        } else {
            self.app.live.as_ref().ok_or("Live is unavailable")?.enqueue(command, mg_live::now_ms())
        }
    }
    pub(crate) fn agents_revision(&self) -> u64 {
        self.link().map_or_else(
            || self.app.live_service.as_ref().map_or(0, |s| s.agents_revision()),
            |s| s.agents_revision(),
        )
    }
    pub(crate) fn agents(&self, now: u64) -> Vec<mg_live::AgentPresence> {
        // Gateway focus presence has its own actor identities, not local HTTP activity.
        self.app.live_service.as_ref().map_or_else(Vec::new, |s| s.agents(now))
    }
    pub(crate) fn presence(&self, id: &str) -> Vec<String> {
        let Some(link) = self.link() else { return Vec::new() };
        focus_names(&link.snapshot(), id)
    }
    pub(crate) fn presence_map(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        let Some(link) = self.link() else {
            return Default::default();
        };
        let snapshot = link.snapshot();
        snapshot.focus.keys().map(|id| (id.clone(), focus_names(&snapshot, id))).collect()
    }
    pub(crate) fn drafts(&self) -> Vec<mg_live::store::assistant::Draft> {
        self.link().map_or_else(
            || self.app.live.as_ref().map_or_else(Vec::new, |s| s.drafts()),
            |s| s.drafts().unwrap_or_default(),
        )
    }
    pub(crate) fn draft_revision(&self) -> u64 {
        self.link().map_or_else(
            || self.app.live.as_ref().map_or(0, |s| s.draft_revision()),
            |s| s.draft_revision(),
        )
    }
}

fn focus_names(snapshot: &mg_live_link::LinkSnapshot, object: &str) -> Vec<String> {
    snapshot
        .focus
        .get(object)
        .into_iter()
        .flatten()
        .take(8)
        .filter_map(|id| {
            let actor = snapshot.actors.iter().find(|a| a["id"].as_str() == Some(id))?;
            let kind = match actor["kind"].as_str()? {
                "ai" => "AI",
                "dm" => "DM",
                _ => return None,
            };
            let name: String =
                actor["name"].as_str().unwrap_or("Participant").chars().take(80).collect();
            Some(format!("{name} · {kind}"))
        })
        .collect()
}

/// Poll startup/results without blocking egui; publish absolute subscriptions once per change.
pub(crate) fn update(app: &mut Moonglow, ctx: &egui::Context) {
    let signed_out = ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::new("live-signed-out")));
    if signed_out
        .as_ref()
        .is_some_and(|id| app.live_gateway.as_ref().is_some_and(|g| &g.server_id == id))
    {
        app.settings.live.enabled = false;
        if let Some(draft) = &mut app.options {
            draft.live.enabled = false;
        }
        app.live_gateway.take();
    }
    let Some(runtime) = &mut app.live_gateway else { return };
    if let Some(receiver) = &runtime.pending {
        match receiver.try_recv() {
            Ok(Ok(link)) => {
                app.live = Some(link.store());
                runtime.link = Some(link);
                runtime.pending = None;
            }
            Ok(Err(error)) => {
                runtime.error = Some(error.clone());
                app.live_error = Some(error);
                runtime.pending = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                runtime.error = Some("Gateway startup stopped".into());
                runtime.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
    let Some(link) = &runtime.link else { return };
    let snapshot = link.snapshot();
    if snapshot.connections != runtime.connection {
        runtime.connection = snapshot.connections;
        runtime.watches = None;
        runtime.focus = None;
    }
    runtime.calls.retain(|call| match call.try_recv() {
        Ok(None) => true,
        Ok(Some(_)) => false,
        Err(error) => {
            app.live_panel.notice = Some(format!("Gateway: {error}"));
            false
        }
    });
    if snapshot.status == LinkStatus::Connected && runtime.calls.len() < 8 {
        let mut focus: Vec<_> =
            if app.settings.no_live_focus || app.dock.find_tab(&crate::Tab::LiveFocus).is_none() {
                Vec::new()
            } else {
                app.live_focus
                    .entries
                    .iter()
                    .flat_map(|e| &e.members)
                    .filter(|m| m.reported)
                    .map(|m| (m.area.clone(), m.id.clone()))
                    .collect()
            };
        focus.sort();
        focus.dedup();
        focus.truncate(256);
        if runtime.focus.as_ref() != Some(&focus)
            && let Ok(call) = link.set_focus(&focus)
        {
            runtime.calls.push(call);
            runtime.focus = Some(focus);
        }
    }
    for (code, _) in link.take_notices() {
        // Arbitrary server text is not copied into persistent application logs.
        app.live_panel.notice = Some(format!("Gateway notice: {code}"));
    }
}

pub(crate) fn watch(app: &mut Moonglow, areas: &[String]) -> Result<(), String> {
    if let Some(runtime) = &mut app.live_gateway {
        if runtime.watches.as_deref() != Some(areas)
            && let Some(link) = &runtime.link
        {
            // The link remembers desired watches while offline for its next hello.
            match link.set_watched_areas(areas) {
                Ok(call) => {
                    runtime.calls.push(call);
                    runtime.watches = Some(areas.to_vec());
                }
                Err(mg_live_link::LinkError::Transport) => {
                    runtime.watches = Some(areas.to_vec());
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    } else {
        app.live.as_ref().map_or(Ok(()), |s| {
            s.set_watched_areas(areas, mg_live::now_ms()).map_err(|e| e.to_string())
        })
    }
}

pub(crate) fn set_allowed(app: &mut Moonglow, allowed: bool) -> Result<(), String> {
    if app.live_backend().is_gateway() {
        return Err(SHARED_REASON.into());
    }
    if let Some(service) = &mut app.live_service {
        service.set_allow_agents(allowed)?;
    }
    Ok(())
}

pub(crate) fn chips(app: &Moonglow, ui: &mut egui::Ui, id: &str) {
    for actor in app.live_backend().presence(id) {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(44, 67, 86))
            .corner_radius(5.)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(actor).font(presence_font(ui)).color(egui::Color32::WHITE),
                )
                .on_hover_text("Has this object in Focus. Presence does not lock the object.");
            });
    }
}

/// Keep map and Focus chips readable even in the compact application theme.
pub(crate) fn presence_font(ui: &egui::Ui) -> egui::FontId {
    let mut font = egui::TextStyle::Small.resolve(ui.style());
    font.size = font.size.max(11.);
    font
}

pub(crate) fn status_ui(app: &Moonglow, ui: &mut egui::Ui) {
    if !app.live_backend().is_gateway() {
        return;
    }
    let status = app.live_backend().connection_status();
    ui.label(format!("Gateway: {}", status["state"].as_str().unwrap_or("connecting")));
    if let Some(error) = status["error"].as_str() {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    if let Some(store) = &app.live {
        let status = store.status(mg_live::now_ms());
        let unknown = status["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["result"]["code"] == "unknown")
            .count();
        if unknown > 0 {
            ui.label(format!(
                "Unknown results: {unknown} — commands may have run; inspect before retrying."
            ));
        }
    }
}

pub(crate) fn menu_status(app: &mut Moonglow, ui: &mut egui::Ui) {
    status_ui(app, ui);
    if app.live_backend().is_gateway() && ui.button("AI agents").clicked() {
        app.live_ai.open = true;
        ui.close();
    }
    if app.live_backend().is_gateway() && ui.button("Reconnect gateway").clicked() {
        app.settings.live.enabled = true;
        app.live_needs_apply = true;
    }
}

impl Moonglow {
    /// Apply saved Live options, with explicit environment overrides, on the UI thread.
    pub fn apply_live(&mut self, ctx: &egui::Context) {
        self.live_needs_apply = false;
        self.live_agents = Default::default();
        self.live_error = None;
        let was_running = self.live_backend().active();
        self.live_service.take();
        self.live_gateway.take();
        if let Some(resolved) =
            self.settings.live_servers.resolve().filter(|r| r.server.id != "local-dev")
        {
            if self.settings.live.enabled
                || (self.live.is_none() && resolved.server.id == "environment")
            {
                let server_id = resolved.server.id.clone();
                let receiver = crate::options::start_gateway(ctx, resolved);
                self.live = Some(Arc::new(LiveStore::new_mirror()));
                self.live_gateway = Some(GatewayRuntime::starting(server_id, receiver));
            }
            return;
        }
        if self.live.as_ref().is_some_and(|s| s.is_mirror()) {
            self.live = None;
        }
        let mut start = || -> Result<Option<mg_live::LiveService>, String> {
            let mut config = self.settings.live.clone().from_env()?;
            // The environment turns Live on when Moonglow starts (no store yet); after
            // that Live › Stop Live and Options decide.
            config.enabled &= self.settings.live.enabled || self.live.is_none();
            if !config.enabled {
                return Ok(None);
            }
            crate::settings::restore_live_clients(&mut config).map_err(|e| e.to_string())?;
            let store = self.live.get_or_insert_with(Default::default).clone();
            let ctx = ctx.clone();
            let mut service = mg_live::LiveService::start(
                config,
                store,
                std::sync::Arc::new(move || crate::live_repaint::wake(&ctx)),
            )?;
            if let Some(data_dir) = self.recovery_dir.as_deref().and_then(std::path::Path::parent) {
                service.publish_descriptor(data_dir)?;
            }
            Ok(Some(service))
        };
        match start() {
            Ok(Some(service)) => {
                let config = service.config();
                // Save a newly generated credential, but never persist an env override.
                if self.settings.live.token.is_empty()
                    && std::env::var_os("MOONGLOW_LIVE_TOKEN").is_none()
                {
                    self.settings.live.token = config.token.clone();
                    if let Some(draft) = &mut self.options {
                        draft.live.token = config.token.clone();
                    }
                }
                self.log.info(format!(
                    "Live: API on {}, Redis {}:{} ({})",
                    service.status()["api"].as_str().unwrap_or_default(),
                    config.redis_host,
                    config.redis_port,
                    config.prefix,
                ));
                self.live_service = Some(service);
            }
            Ok(None) => {
                if was_running {
                    self.log.info("Live: stopped");
                }
            }
            Err(error) => {
                self.log.error(format!("Live: {error}"));
                self.live_error = Some(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_consent_is_off_and_sign_out_keeps_only_the_passive_snapshot() {
        let mut app = Moonglow::new(None, Box::new(crate::NoDialogs::default()));
        let id = app.settings.live_servers.add();
        app.settings.live.enabled = true;
        app.settings.live.allow_agents = true; // A previous local service's consent.
        assert!(app.live_backend().allow_agents());
        assert_eq!(app.live_backend().panel_fraction(), 0.62);
        let (_tx, rx) = mpsc::channel();
        app.live_gateway = Some(GatewayRuntime::starting(id.clone(), rx));
        let store = Arc::new(LiveStore::new_mirror());
        app.live = Some(store.clone());
        let identity = app.live_backend().context();
        assert!(!app.live_backend().allow_agents());
        assert_eq!(app.live_backend().panel_fraction(), 0.4);
        assert_eq!(app.live_backend().connection_status()["state"], "connecting");
        let ctx = egui::Context::default();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("live-signed-out"), id));
        update(&mut app, &ctx);
        assert!(!app.live_backend().active());
        assert!(!app.live_backend().allow_agents());
        assert_eq!(app.live_backend().connection_status()["state"], "stopped");
        assert_eq!(app.live_backend().context(), identity);
        assert_eq!(app.live_backend().shared_reason(), Some(SHARED_REASON));
        assert!(Arc::ptr_eq(app.live.as_ref().unwrap(), &store));
    }

    #[test]
    fn presence_ignores_unknown_actors_and_bounds_untrusted_labels() {
        let snapshot = mg_live_link::LinkSnapshot {
            actors: vec![
                json!({"id":"dm:a","kind":"dm","name":"Guide"}),
                json!({"id":"ai:b","kind":"ai","name":"x".repeat(200)}),
                json!({"id":"bad","kind":"other","name":"Ignore"}),
            ],
            focus: std::collections::BTreeMap::from([(
                "object".into(),
                vec!["dm:a".into(), "ai:b".into(), "bad".into(), "missing".into()],
            )]),
            ..Default::default()
        };
        assert_eq!(
            focus_names(&snapshot, "object"),
            ["Guide · DM".into(), format!("{} · AI", "x".repeat(80))]
        );
        assert!(focus_names(&snapshot, "absent").is_empty());
    }
}
