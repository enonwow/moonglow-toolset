//! One explicit confirmation for the snapshotted eligible Live selection.
use crate::{
    Moonglow,
    area_view::AreaView,
    live::{LiveObject, Op},
};
pub(crate) const ENCOUNTER_REASON: &str = "Encounter removal is unavailable on this server.";
pub(crate) fn removable(kind: mg_area::ObjectKind, o: &LiveObject) -> bool {
    kind != mg_area::ObjectKind::Encounter && crate::live_targets::allowed(o, false)
}

#[derive(Debug)]
pub(crate) struct Confirmation {
    boot: String,
    ids: Vec<String>,
    skipped: usize,
}

pub(crate) fn request(app: &Moonglow, view: &mut AreaView) {
    let Some(store) = &app.live else { return };
    let ids: Vec<_> = view
        .live
        .selection_ids()
        .filter(|id| store.locate(id).is_some_and(|(_, kind, o)| removable(kind, &o)))
        .map(str::to_owned)
        .collect();
    if ids.is_empty() {
        view.notice = Some(
            if view.live.selection_ids().any(|id| {
                store.locate(id).is_some_and(|(_, k, _)| k == mg_area::ObjectKind::Encounter)
            }) {
                ENCOUNTER_REASON.into()
            } else {
                "Select non-player objects to remove.".into()
            },
        );
        return;
    }
    let skipped = view.live.selection_ids().count().saturating_sub(ids.len());
    view.live.removal = Some(Confirmation { boot: store.boot(), ids, skipped });
}

pub(crate) fn window(app: &mut Moonglow, view: &mut AreaView, ctx: &egui::Context) -> bool {
    let Some(draft) = view.live.removal.take() else { return false };
    let valid = app.live.as_ref().is_some_and(|store| {
        store.boot() == draft.boot
            && draft
                .ids
                .iter()
                .all(|id| store.locate(id).is_some_and(|(_, kind, o)| removable(kind, &o)))
    });
    let mut open = true;
    let mut confirm = false;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    egui::Window::new("Remove from game (no body)")
        .id(egui::Id::new(("live-remove", view.area)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!("Remove {} selected objects?", draft.ids.len()));
            ui.label("No corpse or loot remains. This cannot be undone.");
            if draft.skipped > 0 {
                ui.weak(format!(
                    "{} excluded: players, DMs, possessed objects or encounters.",
                    draft.skipped
                ));
            }
            if !valid {
                ui.label("The session or selected objects changed. Cancel and select again.");
            }
            ui.horizontal(|ui| {
                cancel |= ui.button("Cancel").clicked();
                confirm = ui
                    .add_enabled(
                        valid,
                        egui::Button::new(format!("Remove {} objects", draft.ids.len())),
                    )
                    .clicked();
            });
        });
    if confirm {
        for target in draft.ids {
            crate::live_view::queue(app, view, Op::Destroy { target });
        }
    } else if open && !cancel {
        view.live.removal = Some(draft);
    }
    true
}
