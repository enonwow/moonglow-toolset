//! The GUI's single runtime boundary. Reports keep the same LiveStore shape.
//!
//! Moonglow reaches a game server only through a gateway: `GatewayRuntime` owns the link and
//! `Moonglow::live` holds the mirror it fills. A store attached without a gateway (embedding
//! and tests) is read and commanded directly. GUI code uses this view so no widget has its own
//! path to the server.
use crate::Moonglow;
use mg_live::store::{Command, CommandError, LiveStore};
use mg_live_link::{LinkService, LinkStatus, PendingCall};
use serde_json::{Value, json};
use std::sync::{Arc, mpsc};

pub(crate) struct GatewayRuntime {
    pub pending: Option<mpsc::Receiver<Result<LinkService, String>>>,
    pub link: Option<LinkService>,
    pub error: Option<String>,
    pub server_id: String,
    watches: Option<Vec<String>>,
    focus: Option<Vec<(String, String)>>,
    connection: u64,
    calls: Vec<PendingCall>,
    /// Last connection state written to the log, so each drop and recovery is logged once.
    logged: Option<(bool, Option<mg_live_link::LinkError>)>,
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
            watches: None,
            focus: None,
            connection: 0,
            calls: Vec::new(),
            logged: None,
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
    /// Connected to a gateway, or (embedding and tests) driving a store attached directly.
    pub fn active(&self) -> bool {
        self.app.live_gateway.is_some()
            || (self.app.live.is_some() && self.app.settings.live.enabled)
    }
    pub(crate) fn server_id(&self) -> Option<&str> {
        self.app.live_gateway.as_ref().map(|r| r.server_id.as_str())
    }
    pub fn is_gateway(&self) -> bool {
        self.app.live_gateway.is_some() || self.app.live.as_ref().is_some_and(|s| s.is_mirror())
    }
    pub(crate) fn link(&self) -> Option<&LinkService> {
        self.app.live_gateway.as_ref()?.link.as_ref()
    }
    pub fn store(&self) -> Option<Arc<LiveStore>> {
        self.app.live.clone()
    }
    pub(crate) fn panel_fraction(&self) -> f32 {
        // Focus also contains actor chips; leave its two columns and composer usable in the
        // default dock.
        0.4
    }
    pub fn connection_status(&self) -> Value {
        if let Some(gateway) = &self.app.live_gateway {
            gateway.link.as_ref().map_or_else(|| json!({"connected":false,
                "state":if gateway.error.is_some(){"stopped"}else{"connecting"},"error":gateway.error}), |link| {
                let mut status = link.connection_status();
                status["state"] = json!(status["state"].as_str().unwrap_or("connecting").to_ascii_lowercase());
                status
            })
        } else {
            json!({"connected":false,"state":"stopped","error":null})
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
    /// "name (ai:…)" for an AI the gateway has announced. The id stays visible: an owner
    /// chooses the name, so it alone must not pass for another identity.
    pub(crate) fn ai_label(&self, id: &str) -> Option<String> {
        let snapshot = self.link()?.snapshot();
        let actor = snapshot.actors.iter().find(|a| a["id"].as_str() == Some(id))?;
        let name: String =
            actor["name"].as_str().filter(|n| !n.is_empty())?.chars().take(80).collect();
        (actor["kind"].as_str() == Some("ai")).then(|| format!("{name} ({id})"))
    }
    pub(crate) fn presence(&self, id: &str) -> Vec<FocusActor> {
        let Some(link) = self.link() else { return Vec::new() };
        focus_names(&link.snapshot(), id)
    }
    pub(crate) fn presence_map(&self) -> std::collections::BTreeMap<String, Vec<FocusActor>> {
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
    /// With the connection count: a restarted gateway numbers its draft revisions afresh.
    pub(crate) fn draft_revision(&self) -> (u64, u64) {
        self.link().map_or_else(
            || (0, self.app.live.as_ref().map_or(0, |s| s.draft_revision())),
            |s| (s.snapshot().connections, s.draft_revision()),
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FocusActor {
    pub id: String,
    pub label: String,
    pub avatar: Option<crate::live_avatar::Ref>,
}

fn focus_names(snapshot: &mg_live_link::LinkSnapshot, object: &str) -> Vec<FocusActor> {
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
            Some(FocusActor {
                id: id.clone(),
                label: format!("{name} · {kind}"),
                avatar: crate::live_avatar::Ref::new(
                    actor["avatar_path"].as_str(),
                    actor["avatar"].as_str(),
                ),
            })
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
        app.end_gateway();
    }
    let Some(runtime) = &mut app.live_gateway else {
        crate::live_avatar::set_client(ctx, None);
        return;
    };
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
    let Some(link) = &runtime.link else {
        crate::live_avatar::set_client(ctx, None);
        return;
    };
    crate::live_avatar::set_client(ctx, Some(link.avatar_client()));
    let snapshot = link.snapshot();
    // Retries flip between connecting states; log only a new error and a recovery.
    let state = (snapshot.status == LinkStatus::Connected, snapshot.error);
    if runtime.logged.as_ref() != Some(&state) {
        match state {
            (_, Some(error)) => {
                app.log.warn(format!("Live: gateway {:?}: {error}", snapshot.status))
            }
            (true, None) if runtime.logged.is_some() => app.log.info("Live: gateway connected"),
            _ => {}
        }
        runtime.logged = Some(state);
    }
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

pub(crate) fn chips(app: &Moonglow, ui: &mut egui::Ui, id: &str) {
    for actor in app.live_backend().presence(id) {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(44, 67, 86))
            .corner_radius(5.)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if actor.avatar.is_some() {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(18.), egui::Sense::hover());
                        avatar_chip(ui, rect, &actor, id);
                    }
                    ui.label(
                        egui::RichText::new(&actor.label)
                            .font(presence_font(ui))
                            .color(egui::Color32::WHITE),
                    )
                    .on_hover_text("Has this object in Focus. Presence does not lock the object.");
                });
            });
    }
}

pub(crate) fn avatar_chip(ui: &egui::Ui, rect: egui::Rect, actor: &FocusActor, object: &str) {
    if crate::live_avatar::paint(ui, rect, actor.avatar.as_ref()) {
        ui.interact(
            rect,
            ui.id().with(("presence-avatar", object, &actor.id)),
            egui::Sense::hover(),
        )
        .widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Image,
                true,
                format!("Avatar for {}", actor.label),
            )
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
    if app.live_backend().is_gateway()
        && ui
            .add_enabled(app.live_backend().link().is_some(), egui::Button::new("AI agents"))
            .on_disabled_hover_text("Connect to the gateway first: Live › Connect to Gateway")
            .clicked()
    {
        app.live_ai.open = true;
        ui.close();
    }
    if app.live_backend().is_gateway() && ui.button("Reconnect gateway").clicked() {
        app.settings.live.enabled = true;
        app.live_needs_apply = true;
    }
}

/// Dropping a `LinkService` joins its worker, which can wait on a DNS lookup: never on the UI
/// thread.
fn drop_in_background<T: Send + 'static>(value: T) {
    std::thread::spawn(move || drop(value));
}
impl Moonglow {
    fn end_gateway(&mut self) {
        if let Some(runtime) = self.live_gateway.take() {
            drop_in_background(runtime);
        }
    }
    /// Apply the saved Live options, with the environment gateway override, on the UI thread.
    pub fn apply_live(&mut self, ctx: &egui::Context) {
        self.live_needs_apply = false;
        self.live_error = None;
        let was_running = self.live_backend().active();
        self.end_gateway();
        let Some(resolved) = self.settings.live_servers.resolve() else {
            if self.live.as_ref().is_some_and(|s| s.is_mirror()) {
                self.live = None;
            }
            if self.settings.live.enabled {
                self.live_error = Some("Choose a gateway server in Options › Live first.".into());
            } else if was_running {
                self.log.info("Live: stopped");
            }
            return;
        };
        // The environment starts Live when Moonglow starts (no store yet); after that
        // Live › Disconnect from Gateway and Options decide.
        if self.settings.live.enabled
            || (self.live.is_none() && resolved.server.id == "environment")
        {
            let server_id = resolved.server.id.clone();
            let receiver = crate::options::start_gateway(ctx, resolved);
            self.live = Some(Arc::new(LiveStore::new_mirror()));
            self.live_gateway = Some(GatewayRuntime::starting(server_id, receiver));
        } else if was_running {
            self.log.info("Live: stopped");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_retains_only_gateway_served_avatars_with_valid_urls() {
        let snapshot = mg_live_link::LinkSnapshot {
            actors: vec![
                json!({"id":"ai:a","kind":"ai","name":"Scribe","avatar":"https://example.invalid/a.png","avatar_path":"/avatars/ai:a"}),
                json!({"id":"dm:b","kind":"dm","name":"Guide","avatar":"file:///private.png","avatar_path":"/avatars/dm:b"}),
                json!({"id":"dm:c","kind":"dm","name":"Old","avatar":"https://example.invalid/c.png"}),
                json!({"id":"dm:d","kind":"dm","name":"Odd","avatar":"https://example.invalid/d.png","avatar_path":"https://example.invalid/d.png"}),
            ],
            focus: std::collections::BTreeMap::from([(
                "object".into(),
                vec!["ai:a".into(), "dm:b".into(), "dm:c".into(), "dm:d".into()],
            )]),
            ..Default::default()
        };
        let actors = focus_names(&snapshot, "object");
        let kept = crate::live_avatar::Ref::new(
            Some("/avatars/ai:a"),
            Some("https://example.invalid/a.png"),
        );
        assert!(kept.is_some());
        assert_eq!(actors[0].avatar, kept);
        // A bad URL, a gateway without avatar routes and a foreign path draw no image.
        assert!(actors[1..].iter().all(|a| a.avatar.is_none()));
    }

    #[test]
    fn a_dropped_gateway_runtime_is_released_off_the_ui_thread() {
        struct Slow(mpsc::Sender<std::thread::ThreadId>);
        impl Drop for Slow {
            fn drop(&mut self) {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let _ = self.0.send(std::thread::current().id());
            }
        }
        let (tx, rx) = mpsc::channel();
        let started = std::time::Instant::now();
        drop_in_background(Slow(tx));
        assert!(started.elapsed() < std::time::Duration::from_millis(100), "did not wait");
        let thread = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_ne!(thread, std::thread::current().id());
        fn send<T: Send>() {}
        send::<GatewayRuntime>();
    }

    #[test]
    fn sign_out_keeps_only_the_passive_snapshot() {
        let mut app = Moonglow::new(None, Box::new(crate::NoDialogs::default()));
        let id = app.settings.live_servers.add();
        app.settings.live.enabled = true;
        let (_tx, rx) = mpsc::channel();
        app.live_gateway = Some(GatewayRuntime::starting(id.clone(), rx));
        let store = Arc::new(LiveStore::new_mirror());
        app.live = Some(store.clone());
        let identity = app.live_backend().context();
        assert_eq!(app.live_backend().panel_fraction(), 0.4);
        assert_eq!(app.live_backend().connection_status()["state"], "connecting");
        let ctx = egui::Context::default();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("live-signed-out"), id));
        update(&mut app, &ctx);
        assert!(!app.live_backend().active());
        assert_eq!(app.live_backend().connection_status()["state"], "stopped");
        assert_eq!(app.live_backend().context(), identity);
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
            focus_names(&snapshot, "object").into_iter().map(|a| a.label).collect::<Vec<_>>(),
            ["Guide · DM".into(), format!("{} · AI", "x".repeat(80))]
        );
        assert!(focus_names(&snapshot, "absent").is_empty());
    }
}
