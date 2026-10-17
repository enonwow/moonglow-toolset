//! Targeted player dialogs with an explicit send/confirmation step.
use crate::{Moonglow, area_view::AreaView, live::Op};
use mg_live::store::MessageMode;

#[derive(Debug)]
pub(crate) enum Form {
    Message { text: String, mode: MessageMode },
    Journal { quest: String, state: u32 },
    Boot { reason: String },
}
#[derive(Debug)]
pub(crate) struct Dialog {
    pub target: String,
    pub name: String,
    pub boot: String,
    pub form: Form,
}
pub(crate) fn window(app: &mut Moonglow, view: &mut AreaView, ctx: &egui::Context) {
    let Some(mut dialog) = view.live.player_dialog.take() else { return };
    if app.live.as_ref().is_none_or(|s| s.boot() != dialog.boot) {
        return;
    }
    let mut open = true;
    let mut op = None;
    let mut cancel = false;
    let title = match dialog.form {
        Form::Message { .. } => "Message player",
        Form::Journal { .. } => "Player journal",
        Form::Boot { .. } => "Boot player?",
    };
    egui::Window::new(title)
        .id(egui::Id::new(("live-player-dialog", view.area)))
        .open(&mut open)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.strong(format!("{} · {}", dialog.name, dialog.target));
            match &mut dialog.form {
                Form::Message { text, mode } => {
                    ui.horizontal(|ui| {
                        for (label, value) in [
                            ("Chat", MessageMode::Chat),
                            ("Floating", MessageMode::Floating),
                            ("NUI", MessageMode::Nui),
                        ] {
                            ui.selectable_value(mode, value, label);
                        }
                    });
                    let label = ui.label("Message text");
                    ui.text_edit_multiline(text).labelled_by(label.id);
                    if ui
                        .add_enabled(!text.trim().is_empty(), egui::Button::new("Send Message"))
                        .clicked()
                    {
                        op = Some(Op::PcMessage {
                            target: dialog.target.clone(),
                            text: text.clone(),
                            mode: *mode,
                        });
                    }
                }
                Form::Journal { quest, state } => {
                    let label = ui.label("Quest tag");
                    ui.text_edit_singleline(quest).labelled_by(label.id);
                    let label = ui.label("Journal state");
                    ui.add(egui::DragValue::new(state).range(0..=i32::MAX as u32))
                        .labelled_by(label.id);
                    if ui
                        .add_enabled(
                            !quest.trim().is_empty(),
                            egui::Button::new("Set Journal State"),
                        )
                        .clicked()
                    {
                        op = Some(Op::PcJournal {
                            target: dialog.target.clone(),
                            quest: quest.clone(),
                            state: *state,
                        });
                    }
                }
                Form::Boot { reason } => {
                    ui.label("Disconnect this player from the running server?");
                    let label = ui.label("Reason");
                    ui.text_edit_singleline(reason).labelled_by(label.id);
                    if ui.button("Confirm Boot").clicked() {
                        op = Some(Op::PcBoot {
                            target: dialog.target.clone(),
                            reason: reason.clone(),
                        });
                    }
                }
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    if let Some(op) = op {
        crate::live_view::queue(app, view, op);
    } else if open && !cancel {
        view.live.player_dialog = Some(dialog);
    }
}
