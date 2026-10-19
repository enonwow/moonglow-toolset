//! Private, server-scoped DM notes shared by Selection and Focus.
use crate::Moonglow;

pub(crate) fn editor(app: &mut Moonglow, ui: &mut egui::Ui, id: &str) {
    crate::live_book::ui(app, ui, crate::live_book::Target::Note(id.into()));
}
