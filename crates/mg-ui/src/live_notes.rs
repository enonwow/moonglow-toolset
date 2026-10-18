//! Private, module/server-scoped DM notes shared by Selection and Focus.
use crate::Moonglow;
#[derive(Clone, Default)]
struct Editor {
    text: String,
    dirty: bool,
    revision: Option<u64>,
    notice: Option<String>,
}
pub(crate) fn editor(app: &mut Moonglow, ui: &mut egui::Ui, id: &str) {
    if let Some(reason) = app.live_backend().shared_reason() {
        ui.add_enabled(false, egui::Button::new("Shared book")).on_disabled_hover_text(reason);
        ui.weak(reason);
        return;
    }
    let Some(store) = app.live.clone() else { return };
    let key = match store.object_note_key(id) {
        Ok(k) => k,
        Err(e) => {
            ui.weak(e);
            return;
        }
    };
    let config = app.live_backend().config();
    let cache = egui::Id::new((
        "live-note-editor",
        app.module_path(),
        &config.redis_host,
        config.redis_port,
        &config.prefix,
        key,
    ));
    let revision = store.assistant_revision().0;
    let mut edit = ui.ctx().data_mut(|d| d.get_temp::<Editor>(cache)).unwrap_or_default();
    if !edit.dirty && edit.revision != Some(revision) {
        match store.object_note(id) {
            Ok(text) => edit.text = text,
            Err(e) => {
                ui.weak(e);
                return;
            }
        }
        edit.revision = Some(revision);
    }
    ui.weak("Private DM note. Saved for this module and server; never sent to the game.");
    let label = ui.label("Note text");
    edit.dirty |= ui
        .add(
            egui::TextEdit::multiline(&mut edit.text)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .char_limit(4000),
        )
        .labelled_by(label.id)
        .changed();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(edit.dirty, egui::Button::new("Save note"))
            .on_disabled_hover_text("No unsaved changes")
            .clicked()
        {
            match store.save_object_note(id, &edit.text) {
                Ok(()) => {
                    edit.dirty = false;
                    edit.notice = Some("Note saved".into())
                }
                Err(e) => edit.notice = Some(e),
            }
        }
        if ui
            .add_enabled(edit.dirty, egui::Button::new("Reload note"))
            .on_hover_text("Discard unsaved note edits")
            .clicked()
        {
            edit.dirty = false;
            edit.revision = None;
        }
        if let Some(n) = &edit.notice {
            ui.weak(n);
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(cache, edit));
}
