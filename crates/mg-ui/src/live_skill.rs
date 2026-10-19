//! Radius skill checks and their per-player outcomes. Rolls happen in the game.
use crate::{
    Moonglow,
    live::Op,
    live_targets::{selection, send},
};
use serde_json::Value;

#[derive(Debug)]
pub struct LiveSkill {
    pub skills: Vec<(u16, String)>,
    pub skill: u16,
    pub dc: u8,
    pub bonus: i16,
    pub show_dc: bool,
    pub timeout: u16,
    pub radius: f32,
    /// None follows the selection: a PC means one player, an NPC means radius.
    pub single: Option<bool>,
    pub success_text: String,
    pub fail_text: String,
    pub result: Option<Value>,
    pub notice: Option<String>,
    pending: Option<(String, String, std::path::PathBuf)>,
    revision: Option<(usize, u64)>,
    session: Option<(Option<std::path::PathBuf>, String)>,
}
impl Default for LiveSkill {
    fn default() -> Self {
        Self {
            skills: Vec::new(),
            skill: 1,
            dc: 15,
            bonus: 0,
            show_dc: true,
            timeout: 120,
            radius: 10.,
            single: None,
            success_text: String::new(),
            fail_text: String::new(),
            result: None,
            notice: None,
            pending: None,
            revision: None,
            session: None,
        }
    }
}

impl LiveSkill {
    fn load(&mut self, app: &Moonglow) {
        let Some(game) = &app.game else { return };
        let revision =
            (std::sync::Arc::as_ptr(game) as usize, app.ws.as_ref().map_or(0, |w| w.revision()));
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.skills.clear();
        if let Ok(rows) = game
            .choices("skills", mg_rules::ChoiceColumns { name: Some("Name"), label: Some("Label") })
        {
            self.skills = rows
                .into_iter()
                .filter_map(|r| Some((u16::try_from(r.row).ok()?, r.text)))
                .collect();
        }
    }
    fn send(&mut self, app: &mut Moonglow) -> Result<(), String> {
        let selected = selection(app)?;
        let object = selected.objects.first().map(|(_, o)| o);
        if !self.skills.iter().any(|(row, _)| *row == self.skill) {
            return Err("Select a skill from skills.2da.".into());
        }
        let module = app.module_path().ok_or("Open a saved module.")?;
        let single = self.single.unwrap_or(object.is_some_and(|o| o.pc));
        let op = Op::SkillCheck {
            target: if single { Some(object.ok_or("Select a player.")?.id.clone()) } else { None },
            x: None,
            y: None,
            radius: (!single).then_some(self.radius),
            skill: self.skill,
            dc: self.dc,
            bonus: self.bonus,
            show_dc: self.show_dc,
            timeout: self.timeout,
            success_text: self.success_text.clone(),
            fail_text: self.fail_text.clone(),
        };
        if !single {
            crate::live_aim::arm(
                app,
                selected.area,
                op,
                "Skill check in radius",
                crate::live_aim::Source::Skill,
            )?;
            self.notice = Some("Aim the circle, then click the map to check players in range. Esc/right click cancels.".into());
            return Ok(());
        }
        let id = send(app, selected.area, op)?;
        self.pending = Some((id, selected.boot, module));
        self.result = None;
        self.notice = Some("Waiting for player rolls...".into());
        Ok(())
    }
    pub(crate) fn aim_sent(
        &mut self,
        id: String,
        boot: String,
        module: Option<std::path::PathBuf>,
    ) {
        self.pending = module.map(|module| (id, boot, module));
        self.result = None;
        self.notice = Some("Waiting for player rolls...".into());
    }
}

pub(crate) fn update(app: &mut Moonglow, ctx: &egui::Context) {
    let session = (app.module_path(), app.live.as_ref().map(|s| s.boot()).unwrap_or_default());
    if app.live_skill.session.as_ref() != Some(&session) {
        app.live_skill.result = None;
        app.live_skill.session = Some(session);
    }
    let Some((id, boot, module)) = &app.live_skill.pending else { return };
    if app.module_path().as_ref() != Some(module)
        || !crate::live_targets::enabled(app)
        || app.live.as_ref().is_none_or(|s| s.boot() != *boot)
    {
        app.live_skill.pending = None;
        app.live_skill.notice = Some("Skill check cancelled: session or module changed.".into());
        return;
    }
    if let Some(result) = app.live.as_ref().and_then(|s| s.result(id)) {
        app.live_skill.notice = Some(if result.ok && result.pending {
            "Waiting for players to click Roll...".into()
        } else if result.ok {
            "Skill check finished.".into()
        } else {
            result.err
        });
        app.live_skill.result = result.value;
        if result.pending {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        } else {
            app.live_skill.pending = None;
        }
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Full per-player text in the existing command-result event, also used by Copy.
pub fn log_result(value: &Value, who: &dyn Fn(&str) -> String) -> Option<String> {
    let rows = value["players"].as_array()?;
    let mut out = vec!["Skill check".to_string()];
    for row in rows {
        let id = row["id"].as_str()?;
        out.push(format!("{} ({id}): {}", who(id), roll_summary(value, row)));
    }
    if rows.is_empty() {
        out.push("No eligible players in range.".into());
    }
    if value["truncated"] == true {
        out.push("Only the first 64 players were invited.".into());
    }
    Some(out.join("\n"))
}

fn roll_summary(value: &Value, row: &Value) -> String {
    match row["status"].as_str() {
        Some("waiting") => return "Waiting for Roll".into(),
        Some("no_roll") => return "No roll (timed out)".into(),
        Some("busy") => return "Already has a pending skill check".into(),
        _ => {}
    }
    let outcome = if row["passed"] == true { "Success" } else { "Failure" };
    if let (Some(roll), Some(modifier), Some(total)) =
        (row["roll"].as_i64(), row["modifier"].as_i64(), row["total"].as_i64())
    {
        format!(
            "{}: {roll} + {modifier} + {} (DM) = {total} vs DC {}: {outcome}",
            value["name"].as_str().unwrap_or("Skill"),
            value["bonus"],
            value["dc"]
        )
    } else {
        outcome.into()
    }
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    let mut state = std::mem::take(&mut app.live_skill);
    state.load(app);
    let selected = selection(app);
    let centre = selected.as_ref().ok().and_then(|s| s.objects.first());
    ui.heading("Skill check");
    ui.separator();
    let single = state.single.unwrap_or(centre.is_some_and(|(_, o)| o.pc));
    ui.horizontal(|ui| {
        if ui.selectable_label(!single, "Players in radius").clicked() {
            state.single = Some(false);
        }
        if ui
            .add_enabled(
                centre.is_some_and(|(_, o)| o.pc),
                egui::Button::selectable(single, "Selected player"),
            )
            .on_disabled_hover_text("Select a player on the Live map first.")
            .clicked()
        {
            state.single = Some(true);
        }
    });
    if !single {
        ui.label("Aim a circle on the map; click to choose its centre.");
    } else if let Some((_, o)) = centre {
        ui.label(format!("Player: {}", o.name));
    } else {
        ui.label(
            selected
                .as_ref()
                .err()
                .map(String::as_str)
                .unwrap_or("Select a live object for the centre."),
        );
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Skill");
        egui::ComboBox::from_id_salt("live-skill-choice")
            .selected_text(
                state
                    .skills
                    .iter()
                    .find(|(r, _)| *r == state.skill)
                    .map(|(_, n)| n.as_str())
                    .unwrap_or("Select skill"),
            )
            .show_ui(ui, |ui| {
                for (row, name) in &state.skills {
                    ui.selectable_value(&mut state.skill, *row, name);
                }
            });
        ui.add(egui::DragValue::new(&mut state.dc).range(0..=255).prefix("DC "));
        let radius_label = ui.label("Radius");
        ui.add_enabled(
            !single,
            egui::DragValue::new(&mut state.radius).range(0.1..=30.).suffix(" m"),
        )
        .labelled_by(radius_label.id)
        .on_disabled_hover_text("Radius applies only to Players in radius.");
    });
    ui.horizontal_wrapped(|ui| {
        let label = ui.label("DM bonus");
        ui.add(egui::DragValue::new(&mut state.bonus).range(-100..=100)).labelled_by(label.id);
        ui.checkbox(&mut state.show_dc, "Show DC to player");
    });
    ui.horizontal(|ui| {
        let label = ui.label("Roll timeout");
        ui.add(egui::DragValue::new(&mut state.timeout).range(1..=600).suffix(" s"))
            .labelled_by(label.id);
    });
    ui.label("Success text (optional)");
    ui.add(
        egui::TextEdit::multiline(&mut state.success_text)
            .char_limit(200)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    ui.label("Failure text (optional)");
    ui.add(
        egui::TextEdit::multiline(&mut state.fail_text)
            .char_limit(200)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    let enabled = app.live.as_ref().is_some_and(|s| s.policy().players);
    if !enabled {
        ui.weak("Player operations are disabled by the gateway's policy.");
    }
    ui.collapsing("How player rolls work", |ui| {
        ui.label("Players click Roll in their Skill check window. The DM bonus adds to d20 and their full skill modifier.");
        ui.label("Empty texts show only the result; unanswered requests become No roll after the timeout.");
    });
    let reason = if !enabled {
        Some("Player operations are disabled by the gateway's policy.")
    } else if let Err(e) = &selected {
        Some(e.as_str())
    } else if single && !centre.is_some_and(|(_, o)| o.pc) {
        Some("Select a player on the Live map first.")
    } else if state.pending.is_some() {
        Some("Wait for the current skill check to finish.")
    } else if !state.skills.iter().any(|(row, _)| *row == state.skill) {
        Some("Choose an available skill first.")
    } else {
        None
    };
    ui.separator();
    ui.weak(if single {
        "Send invites the selected player to roll."
    } else {
        "Next: Send skill check, then click the map to choose the circle's centre."
    });
    if ui
        .add_enabled(reason.is_none(), egui::Button::new("Send skill check"))
        .on_disabled_hover_text(reason.unwrap_or_default())
        .on_hover_text(if single {
            "Invite the selected player to roll now."
        } else {
            "Start aiming a radius on the map. Click to send; Esc cancels."
        })
        .clicked()
        && let Err(e) = state.send(app)
    {
        state.notice = Some(e);
    }
    if let Some(note) = &state.notice {
        ui.label(note);
    }
    if let Some(result) = &state.result {
        ui.separator();
        if let Some(rows) = result["players"].as_array() {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("skill-outcomes")
                    .striped(true)
                    .max_col_width(ui.available_width() * 0.65)
                    .show(ui, |ui| {
                        ui.strong("Player");
                        ui.strong("Roll / result");
                        ui.end_row();
                        for row in rows {
                            let id = row["id"].as_str().unwrap_or("?");
                            let name = app
                                .live
                                .as_ref()
                                .and_then(|s| s.locate(id))
                                .map(|(_, _, o)| o.name)
                                .unwrap_or_else(|| id.into());
                            ui.label(format!("{name} ({id})"));
                            ui.add(egui::Label::new(roll_summary(result, row)).wrap())
                                .on_hover_text(
                                    if row["nui_token"].as_i64().is_some_and(|t| t > 0) {
                                        "Player window"
                                    } else {
                                        "NUI unavailable: chat notice; no automatic roll"
                                    },
                                );
                            ui.end_row();
                        }
                    });
                if rows.is_empty() {
                    ui.label("No eligible players in range.");
                }
                if result["truncated"] == true {
                    ui.label("Limited to 64 players; narrow the radius for another check.");
                }
            });
        }
    }
    app.live_skill = state;
}
