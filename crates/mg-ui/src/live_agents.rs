//! One consent control and an event-driven presence snapshot for every Live surface.
use crate::Moonglow;
use mg_live::{AgentPresence, agents::PRESENCE_MS};
use std::time::Duration;

pub(crate) const OFF_REASON: &str = "Start Live to allow LLM connections";

#[derive(Default)]
pub(crate) struct LiveAgents {
    revision: Option<u64>,
    agents: Vec<AgentPresence>,
}

impl LiveAgents {
    fn expires(&self) -> Option<u64> {
        self.agents.iter().map(|a| a.last_ms.saturating_add(PRESENCE_MS)).min()
    }

    fn refresh(&mut self, revision: u64, now: u64, read: impl FnOnce() -> Vec<AgentPresence>) {
        if self.revision != Some(revision) || self.expires().is_some_and(|at| now >= at) {
            self.agents = read();
            self.agents.retain(|a| now.saturating_sub(a.last_ms) < PRESENCE_MS);
            self.revision = Some(revision);
        }
    }

    pub(crate) fn connected(&self) -> bool {
        !self.agents.is_empty()
    }

    pub(crate) fn hint(&self, allowed: bool, now: u64) -> String {
        let access = if allowed { "on" } else { "off" };
        let mut text = format!("LLM access {access}");
        if !allowed || self.agents.is_empty() {
            text.push_str("; no LLM connected");
        } else {
            for a in &self.agents {
                text.push_str(&format!(
                    "\nconnected: {} (last: {}, {} s ago)",
                    a.client,
                    a.op,
                    now.saturating_sub(a.last_ms) / 1000,
                ));
            }
        }
        text
    }

    fn schedule(&self, ctx: &egui::Context, now: u64) {
        if let Some(at) = self.expires() {
            crate::live_repaint::after(ctx, Duration::from_millis(at.saturating_sub(now)));
        }
    }
}

pub(crate) fn update(app: &mut Moonglow, ctx: &egui::Context) {
    if !app.live_backend().active() {
        app.live_agents = LiveAgents::default();
        return;
    };
    if !app.live_backend().allow_agents() {
        app.live_agents = LiveAgents::default();
        return;
    }
    let now = mg_live::now_ms();
    let revision = app.live_backend().agents_revision();
    if app.live_agents.revision != Some(revision)
        || app.live_agents.expires().is_some_and(|at| now >= at)
    {
        let agents = app.live_backend().agents(now);
        app.live_agents.refresh(revision, now, || agents);
    }
    app.live_agents.schedule(ctx, now);
}

/// Only this path mutates consent: no service/Redis restart and no stale Options draft.
pub(crate) fn set_allowed(app: &mut Moonglow, ctx: &egui::Context, allowed: bool) {
    if !app.live_backend().active() {
        return;
    }
    let result = crate::live_backend::set_allowed(app, allowed);
    // A failed descriptor publication must leave the UI at the actual gate state.
    app.settings.live.allow_agents = app.live_backend().allow_agents();
    if let Some(draft) = &mut app.options {
        draft.live.allow_agents = app.settings.live.allow_agents;
    }
    if let Err(error) = result {
        app.log.error(format!("LLM access: {error}"));
        app.live_error = Some(error);
    }
    app.live_agents = LiveAgents::default();
    update(app, ctx);
    crate::live_repaint::wake(ctx);
}

pub(crate) fn hint(app: &Moonglow) -> String {
    app.live_agents
        .hint(app.live_backend().active() && app.live_backend().allow_agents(), mg_live::now_ms())
}

pub(crate) fn dot(ui: &mut egui::Ui, connected: bool, hint: &str) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(12., 16.), egui::Sense::hover());
    ui.painter().circle_filled(
        rect.center(),
        4.,
        if connected {
            egui::Color32::from_rgb(80, 210, 110)
        } else {
            egui::Color32::from_rgb(235, 85, 85)
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Label,
            true,
            if connected { "LLM connected" } else { "No LLM connected" },
        )
    });
    response.on_hover_text(hint);
}

pub(crate) fn toolbar(app: &mut Moonglow, ui: &mut egui::Ui) {
    if app.live_backend().is_gateway() {
        if ui.button("My AI").clicked() {
            app.live_ai.open = true;
        }
        return;
    }
    let mut allowed = app.live_backend().allow_agents();
    let hint = hint(app);
    let changed = ui
        .add_enabled_ui(
            app.live_backend().active() && app.live_backend().shared_reason().is_none(),
            |ui| {
                ui.toggle_value(&mut allowed, "⚡ LLM")
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(
                        app.live_backend().shared_reason().unwrap_or(OFF_REASON),
                    )
                    .changed()
            },
        )
        .inner;
    if changed {
        set_allowed(app, ui.ctx(), allowed);
    }
    dot(ui, app.live_agents.connected(), &self::hint(app));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(at: u64) -> Vec<AgentPresence> {
        vec![AgentPresence { client: "Story helper".into(), last_ms: at, op: "live_find".into() }]
    }

    #[test]
    fn snapshot_is_read_only_on_revision_or_expiry_and_heartbeat_extends_deadline() {
        let mut state = LiveAgents::default();
        state.refresh(1, 100, || client(100));
        assert!(state.connected());
        state.refresh(1, 200, || panic!("unchanged frame must not clone presence"));
        assert_eq!(state.expires(), Some(60_100));
        state.refresh(2, 20_100, || client(20_100));
        state.refresh(2, 60_100, || panic!("old expiry cannot clear a renewed heartbeat"));
        assert_eq!(state.expires(), Some(80_100));
        state.refresh(2, 80_100, || client(20_100));
        assert!(!state.connected());
        state.refresh(2, 90_000, || panic!("expired empty snapshot does not poll"));
    }

    #[test]
    fn presence_schedules_expiry_once_without_an_idle_timer() {
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(Default::default(), |_| {}).drop_without_applying_deltas();
        }
        let mut state = LiveAgents::default();
        state.refresh(1, 100, || client(100));
        let output = ctx.run_ui(Default::default(), |ui| state.schedule(ui.ctx(), 100));
        assert_eq!(
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            Duration::from_secs(60)
        );
        output.drop_without_applying_deltas();
        state.refresh(1, 60_100, Vec::new);
        let output = ctx.run_ui(Default::default(), |ui| state.schedule(ui.ctx(), 60_100));
        assert_eq!(output.viewport_output[&egui::ViewportId::ROOT].repaint_delay, Duration::MAX);
        output.drop_without_applying_deltas();
    }

    #[test]
    fn fake_presence_app_off_on_connected_expired() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut h = Harness::builder().build_ui_state(
            |ui, (allowed, now, state): &mut (bool, u64, LiveAgents)| {
                ui.checkbox(allowed, "Allow LLM");
                dot(ui, *allowed && state.connected(), &state.hint(*allowed, *now));
            },
            (false, 100, LiveAgents::default()),
        );
        h.get_by_label("No LLM connected");
        h.get_by_label("Allow LLM").click();
        h.run();
        h.get_by_label("No LLM connected");
        h.state_mut().2.refresh(1, 100, || client(100));
        h.run();
        h.get_by_label("LLM connected");
        assert_eq!(
            h.state().2.hint(true, 5100),
            "LLM access on\nconnected: Story helper (last: live_find, 5 s ago)"
        );
        h.state_mut().1 = 60_100;
        h.state_mut().2.refresh(1, 60_100, Vec::new);
        h.run();
        h.get_by_label("No LLM connected");
    }
}
