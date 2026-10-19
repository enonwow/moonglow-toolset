//! Live inventories are queried from the game, never written into the module.
use crate::{
    Moonglow,
    live::Op,
    live_targets::{selection, send},
};
use mg_core::ResRef;
use serde_json::Value;
#[path = "live_inventory_grid.rs"]
mod grid;
#[path = "live_item_picker.rs"]
mod picker;

#[derive(Debug, Default)]
pub struct LiveInventory {
    pub resref: String,
    pub result: Option<Value>,
    pub notice: Option<String>,
    pub gold_amount: u32,
    pub equip_slot: u8,
    order: Vec<String>,
    context: Option<(Option<std::path::PathBuf>, String, ResRef, String)>,
    pending: Option<(String, bool)>,
    auto_requested: bool,
    selected: Option<String>,
    picker: picker::Picker,
    icons: std::collections::HashMap<String, Vec<crate::images::Picture>>,
}
impl LiveInventory {
    pub(crate) fn reopen(&mut self) {
        self.auto_requested = false;
    }
    fn queue(&mut self, app: &mut Moonglow, op: Op, reading: bool) -> Result<(), String> {
        let selected = selection(app)?;
        let id = send(app, selected.area, op)?;
        self.pending = Some((id, reading));
        self.notice = Some("Waiting for the game...".into());
        Ok(())
    }
    fn refresh(&mut self, app: &mut Moonglow, target: &str) -> Result<(), String> {
        self.queue(app, Op::ViewInventory { target: target.into() }, true)
    }
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    let mut state = std::mem::take(&mut app.live_inventory);
    let selected = selection(app);
    let selected = selected.as_ref().ok().and_then(|s| s.objects.first().map(|o| (s, o)));
    let context =
        selected.map(|(s, (_, o))| (app.module_path(), s.boot.clone(), s.area, o.id.clone()));
    if context != state.context {
        state.context = context;
        state.result = None;
        state.pending = None;
        state.auto_requested = false;
        state.selected = None;
        state.order.clear();
        state.notice = None;
        state.icons.clear();
        state.picker.open = false;
    }
    let Some((_, (kind, object))) = selected else {
        ui.heading("Live Inventory");
        ui.separator();
        ui.label("Select a creature or player in a fresh Live area.");
        app.live_inventory = state;
        return;
    };
    if *kind != mg_area::ObjectKind::Creature {
        ui.heading("Live Inventory");
        ui.separator();
        ui.label("Inventory requires a creature or player selection.");
        app.live_inventory = state;
        return;
    }
    let target = object.id.clone();
    ui.heading(format!("Inventory: {}", object.name));
    ui.separator();
    if let Some((id, reading)) = state.pending.clone() {
        if let Some(result) = app.live.as_ref().and_then(|s| s.result(&id)) {
            state.pending = None;
            state.notice = Some(if result.ok { "Inventory received.".into() } else { result.err });
            if result.ok && reading {
                state.result = result.value;
                state.icons.clear();
                state.selected = None;
            } else if result.ok
                && let Err(error) = state.refresh(app, &target)
            {
                state.notice = Some(error);
            }
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    let players = app.live.as_ref().is_some_and(|s| s.policy().players);
    let protected = object.pc || object.dm || object.possessed;
    if !state.auto_requested && state.pending.is_none() && (!protected || players) {
        state.auto_requested = true;
        if let Err(error) = state.refresh(app, &target) {
            state.notice = Some(error);
        }
    }
    let ready = state.pending.is_none() && (!protected || players);
    let reason = if state.pending.is_some() {
        "Wait for the current inventory request to finish."
    } else {
        "Player operations are disabled by the gateway's policy for this target."
    };
    if protected && !players {
        ui.weak(
            "Player operations are disabled by the gateway's policy: this inventory is read-only.",
        );
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(ready, egui::Button::new("Refresh inventory"))
            .on_disabled_hover_text(reason)
            .on_hover_text("Read the latest inventory from the server.")
            .clicked()
            && let Err(error) = state.refresh(app, &target)
        {
            state.notice = Some(error);
        }
        if ui
            .add_enabled(ready, egui::Button::new("Give item..."))
            .on_disabled_hover_text(reason)
            .on_hover_text("Choose an item from the loaded module and standard palettes.")
            .clicked()
        {
            state.picker.load(app);
            state.picker.open = true;
        }
    });
    if let Some(resref) = state.picker.ui(app, ui.ctx(), &object.name, &mut state.resref, ready)
        && let Err(error) = state.queue(app, Op::GiveItem { target: target.clone(), resref }, false)
    {
        state.notice = Some(error);
    }
    if let Some(note) = &state.notice {
        ui.horizontal(|ui| {
            if state.pending.as_ref().is_some_and(|(_, reading)| *reading) {
                ui.spinner();
                ui.label("Loading inventory...");
            } else {
                ui.label(note);
            }
        });
    }
    let rows =
        state.result.as_ref().and_then(|v| v["items"].as_array()).cloned().unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.strong(format!(
            "Gold: {}",
            object.gold.map_or_else(|| "unknown".into(), |n| n.to_string())
        ));
        ui.add(egui::DragValue::new(&mut state.gold_amount).range(0..=1_000_000));
        if ui
            .add_enabled(ready && players && state.gold_amount > 0, egui::Button::new("Give gold"))
            .on_disabled_hover_text(if !ready {
                reason
            } else if !players {
                "Player operations are disabled by the gateway's policy."
            } else {
                "Enter a gold amount greater than zero."
            })
            .clicked()
        {
            let op = Op::PcGiveGold { target: target.clone(), amount: state.gold_amount };
            if let Err(e) = state.queue(app, op, false) {
                state.notice = Some(e);
            }
        }
        if ui
            .add_enabled(ready && state.gold_amount > 0, egui::Button::new("Take gold"))
            .on_disabled_hover_text(if !ready {
                reason
            } else {
                "Enter a gold amount greater than zero."
            })
            .clicked()
        {
            let op = Op::TakeGold { target: target.clone(), amount: state.gold_amount };
            if let Err(e) = state.queue(app, op, false) {
                state.notice = Some(e);
            }
        }
    });
    let height = ui.available_height().max(160.);
    let action = egui::ScrollArea::vertical()
        .id_salt("live-inventory-grid")
        .max_height(height)
        .show(ui, |ui| grid::show(app, ui, &mut state, &rows, &target, ready))
        .inner;
    if let Some(action) = action {
        let op = match action {
            grid::Drop::Use { item, property, recipient } => {
                Some(Op::UseItem { target: target.clone(), item, property, recipient })
            }
            grid::Drop::Transfer { item, recipient } => {
                Some(Op::TransferItem { target: target.clone(), item, recipient })
            }
            grid::Drop::Equip { item, slot } => {
                Some(Op::EquipItem { target: target.clone(), item, slot })
            }
            grid::Drop::Backpack { item, before } => {
                for row in rows.iter().filter(|r| r["slot"].as_i64().unwrap_or(-1) < 0) {
                    if let Some(id) = row["id"].as_str()
                        && !state.order.iter().any(|r| r == id)
                    {
                        state.order.push(id.into());
                    }
                }
                state.order.retain(|id| id != &item);
                let at = before
                    .as_ref()
                    .and_then(|before| state.order.iter().position(|id| id == before))
                    .unwrap_or(state.order.len());
                state.order.insert(at, item.clone());
                let slot = rows
                    .iter()
                    .find(|r| r["id"].as_str() == Some(&item))
                    .and_then(|r| r["slot"].as_u64());
                slot.filter(|n| *n <= 13).map(|slot| Op::UnequipItem {
                    target: target.clone(),
                    item,
                    slot: slot as u8,
                })
            }
            grid::Drop::Ground { item } => Some(Op::DropItem { target: target.clone(), item }),
            grid::Drop::Take { item } => {
                Some(Op::TakeItem { target: target.clone(), item: Some(item), resref: None })
            }
        };
        if let Some(op) = op
            && let Err(e) = state.queue(app, op, false)
        {
            state.notice = Some(e);
        }
    }
    if state.result.is_some() && rows.is_empty() {
        ui.label("Inventory is empty.");
        ui.weak("Use Give item to choose an item for this creature.");
    }
    if state.result.as_ref().is_some_and(|v| v["truncated"] == true) {
        ui.weak("Only the first 128 items / 15 KB are shown.");
    }
    app.live_inventory = state;
}
fn slot_name(slot: i64) -> &'static str {
    match slot {
        0 => "Head",
        1 => "Chest",
        2 => "Boots",
        3 => "Arms",
        4 => "Right hand",
        5 => "Left hand",
        6 => "Cloak",
        7 => "Left ring",
        8 => "Right ring",
        9 => "Neck",
        10 => "Belt",
        11 => "Arrows",
        12 => "Bullets",
        13 => "Bolts",
        14..=16 => "Creature weapon",
        17 => "Creature armour",
        _ => "Unknown",
    }
}
