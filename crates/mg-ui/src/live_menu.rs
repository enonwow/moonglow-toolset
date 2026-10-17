//! Small value inputs in Live context menus; nothing sends while a field is edited.
use crate::live::Op;

pub(crate) fn number(
    ui: &mut egui::Ui,
    target: &str,
    label: &str,
    default: u32,
    max: u32,
    button: &str,
) -> Option<u32> {
    let id = ui.make_persistent_id((target, label));
    let mut value = ui.data(|d| d.get_temp(id)).unwrap_or(default);
    let text = ui.label(label);
    ui.add(egui::DragValue::new(&mut value).range(0..=max)).labelled_by(text.id);
    ui.data_mut(|d| d.insert_temp(id, value));
    ui.button(button).clicked().then_some(value)
}
/// Exact target only: the nearby-NPC radius action is a separate menu item.
pub(crate) fn stop_attack(ui: &mut egui::Ui, target: &str, enabled: bool) -> Option<Op> {
    ui.add_enabled(enabled, egui::Button::new("Stop Attack"))
        .on_disabled_hover_text("Requires a living creature. Players also require Allow actions on players and the server player switch.")
        .clicked()
        .then(|| Op::StopCombat { target: Some(target.into()), x: None, y: None, radius: None })
}

pub(crate) fn heal(ui: &mut egui::Ui, target: &str, pc: bool) -> Option<Op> {
    let mut op = None;
    values(ui, "Heal", |ui| {
        let amount = number(ui, target, "Heal amount", 10, 100_000, "Heal Amount")
            .map(Some)
            .or_else(|| ui.button("Full Heal").clicked().then_some(None));
        if let Some(amount) = amount {
            op = Some(if pc {
                Op::PcHeal { target: target.into(), amount }
            } else {
                Op::Heal { target: target.into(), amount }
            });
        }
    });
    op
}
pub(crate) fn freeze(ui: &mut egui::Ui, target: &str, frozen: bool) -> Option<Op> {
    let mut op = None;
    if frozen {
        if ui.button("Unfreeze").clicked() {
            op = Some(Op::Unfreeze { target: target.into() });
        }
    } else {
        values(ui, "Freeze", |ui| {
            if let Some(seconds) =
                number(ui, target, "Freeze seconds", 30, 300, "Freeze Now").filter(|s| *s > 0)
            {
                op = Some(Op::Freeze { target: target.into(), seconds: seconds as f32 });
            }
            for seconds in [10., 30., 60., 300.] {
                if ui.button(format!("{seconds} s")).clicked() {
                    op = Some(Op::Freeze { target: target.into(), seconds });
                }
            }
        });
    }
    op
}
/// Explicit removals use positive amounts; editing never issues a command.
pub(crate) fn economy(ui: &mut egui::Ui, target: &str, level: Option<u8>) -> Option<Op> {
    let mut chosen = None;
    for (label, field, max, gold) in
        [("Take Gold", "Gold to take", 1_000_000, true), ("Take XP", "XP to take", 100_000, false)]
    {
        values(ui, label, |ui| {
            let id = ui.make_persistent_id((target, field));
            let mut amount: u32 = ui.data(|d| d.get_temp(id)).unwrap_or(100);
            let text = ui.label(field);
            ui.add(egui::DragValue::new(&mut amount).range(1..=max)).labelled_by(text.id);
            ui.data_mut(|d| d.insert_temp(id, amount));
            if ui.button(format!("{label} Now")).clicked() {
                chosen = Some(if gold {
                    Op::TakeGold { target: target.into(), amount }
                } else {
                    Op::TakeXp { target: target.into(), amount }
                });
            }
        });
    }
    if ui
        .add_enabled(level.is_none_or(|level| level > 1), egui::Button::new("Remove Level"))
        .on_disabled_hover_text("A creature cannot go below level 1.")
        .on_hover_text("Ask the server to remove one level from each eligible creature.")
        .clicked()
    {
        chosen = Some(Op::RemoveLevel { target: target.into() });
    }
    chosen
}

/// Editable menus stay open while typing; the caller closes after sending.
pub(crate) fn values(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    egui::containers::menu::SubMenuButton::new(label)
        .config(
            egui::containers::menu::MenuConfig::default()
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
        )
        .ui(ui, content);
}
