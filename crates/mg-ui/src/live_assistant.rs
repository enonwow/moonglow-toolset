//! Entry points to the gateway's DM guidance: the GM card, NPC cards and NPC history.
use crate::{Action, Moonglow, Tab};

pub(crate) fn card(app: &mut Moonglow, ui: &mut egui::Ui, target: Option<&str>) {
    crate::live_book::ui(
        app,
        ui,
        target.map_or(crate::live_book::Target::Gm, |id| crate::live_book::Target::Npc(id.into())),
    );
}
pub(crate) fn npc_button(app: &mut Moonglow, ui: &mut egui::Ui, id: &str) {
    let allowed = app.live.as_ref().is_some_and(|s| {
        s.locate(id).is_some_and(|(_, k, o)| {
            k == mg_area::ObjectKind::Creature && !o.pc && !o.dm && !o.possessed
        })
    });
    if allowed
        && ui
            .add(egui::Button::new("NPC card").small())
            .on_hover_text(
                "Edit this NPC's identity, knowledge, secrets and voice for the assistant.",
            )
            .clicked()
    {
        open_npc_card(app, id);
    }
}
pub(crate) fn open_npc_card(app: &mut Moonglow, id: &str) {
    app.actions.push(Action::OpenTab(Tab::LiveNpcCard(id.into())));
}
pub(crate) fn memory(_app: &mut Moonglow, ui: &mut egui::Ui, _id: &str) {
    ui.weak("Gateway conversation history is not available in this view yet.");
}
