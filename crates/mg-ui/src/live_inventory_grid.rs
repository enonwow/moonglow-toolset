//! Live inventory presentation. Backpack order is local; native slots are read back.
use super::{LiveInventory, slot_name};
use crate::Moonglow;
use serde_json::Value;

#[derive(Clone)]
struct Dragged {
    holder: String,
    item: String,
}

pub(super) enum Drop {
    Equip { item: String, slot: u8 },
    Use { item: String, property: u8, recipient: Option<String> },
    Transfer { item: String, recipient: String },
    Backpack { item: String, before: Option<String> },
    Ground { item: String },
    Take { item: String },
}

struct CellContext<'a> {
    target: &'a str,
    recipients: &'a [(String, String)],
    ready: bool,
    reason: &'a str,
}

fn cell(
    ui: &mut egui::Ui,
    state: &mut LiveInventory,
    context: &CellContext<'_>,
    row: Option<&Value>,
    label: &str,
    action: &mut Option<Drop>,
) -> egui::Response {
    let CellContext { target, recipients, ready, reason } = *context;
    let name = row.and_then(|r| r["name"].as_str()).unwrap_or("empty");
    let id = row.and_then(|r| r["id"].as_str());
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(52., 60.), egui::Sense::click_and_drag());
    let selected = id.is_some() && id == state.selected.as_deref();
    let fill =
        if selected { ui.visuals().selection.bg_fill } else { ui.visuals().extreme_bg_color };
    ui.painter().rect(
        rect,
        3.,
        fill,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let image_rect = egui::Rect::from_min_size(rect.min + egui::vec2(4., 3.), egui::vec2(44., 42.));
    if let Some(icon) = id.and_then(|id| state.icons.get(id)) {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(image_rect));
        crate::images::icon_box(&mut child, icon, image_rect.size(), name);
    }
    ui.painter().text(
        rect.center_bottom() - egui::vec2(0., 3.),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(9.),
        ui.visuals().text_color(),
    );
    if let Some(stack) = row.and_then(|r| r["stack"].as_u64()).filter(|n| *n > 1) {
        ui.painter().text(
            rect.right_top() + egui::vec2(-3., 3.),
            egui::Align2::RIGHT_TOP,
            stack,
            egui::FontId::proportional(13.),
            egui::Color32::WHITE,
        );
    }
    let accessible = format!("{label}: {name}");
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ready, &accessible));
    if ready && let Some(id) = id {
        if response.clicked() || response.secondary_clicked() {
            state.selected = Some(id.into());
        }
        response.dnd_set_drag_payload(Dragged { holder: target.into(), item: id.into() });
    }
    if let Some(row) = row
        && let Some(id) = id
    {
        response.context_menu(|ui| {
            ui.strong(name);
            ui.add_enabled_ui(ready, |ui| {
                let mask = row["equip_slots"].as_u64().unwrap_or(0);
                if mask & 0x3fff != 0 {
                    ui.menu_button("Equip", |ui| {
                        for slot in (0..=13).filter(|slot| mask & (1 << slot) != 0) {
                            if ui.button(slot_name(slot)).on_disabled_hover_text(reason).clicked() {
                                *action = Some(Drop::Equip { item: id.into(), slot: slot as u8 });
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_disabled_hover_text(reason);
                }
                if let Some(uses) = row["uses"].as_array().filter(|u| !u.is_empty()) {
                    ui.menu_button("Use", |ui| {
                        for ability in uses {
                            let Some(property) =
                                ability["property"].as_u64().and_then(|v| u8::try_from(v).ok())
                            else {
                                continue;
                            };
                            let label = ability["name"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .unwrap_or("Item ability");
                            if ui
                                .button(format!("{label} on self"))
                                .on_disabled_hover_text(reason)
                                .clicked()
                            {
                                *action =
                                    Some(Drop::Use { item: id.into(), property, recipient: None });
                                ui.close();
                            }
                            ui.menu_button(format!("{label} on..."), |ui| {
                                for (recipient, name) in recipients {
                                    if ui.button(name).on_disabled_hover_text(reason).clicked() {
                                        *action = Some(Drop::Use {
                                            item: id.into(),
                                            property,
                                            recipient: Some(recipient.clone()),
                                        });
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_disabled_hover_text(reason);
                        }
                    })
                    .response
                    .on_disabled_hover_text(reason);
                }
                if row["plot"] != true && row["droppable"] != false {
                    ui.menu_button("Give to...", |ui| {
                        if recipients.is_empty() {
                            ui.weak("No other eligible creature in this area.");
                        }
                        for (recipient, name) in recipients {
                            if ui.button(name).on_disabled_hover_text(reason).clicked() {
                                *action = Some(Drop::Transfer {
                                    item: id.into(),
                                    recipient: recipient.clone(),
                                });
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_disabled_hover_text(reason);
                }
                if row["slot"].as_i64().is_some_and(|n| (0..=13).contains(&n))
                    && ui.button("Unequip").on_disabled_hover_text(reason).clicked()
                {
                    *action = Some(Drop::Backpack { item: id.into(), before: None });
                    ui.close();
                }
                ui.add_enabled_ui(row["plot"] != true, |ui| {
                    ui.menu_button("Drop...", |ui| {
                        ui.label("Drop this item at the creature's feet?");
                        if ui.button("Confirm drop").clicked() {
                            *action = Some(Drop::Ground { item: id.into() });
                            ui.close();
                        }
                    })
                    .response
                    .on_disabled_hover_text(if !ready {
                        reason
                    } else {
                        "Plot items cannot be dropped."
                    });
                    ui.menu_button("Take...", |ui| {
                        ui.label("Destroy this item permanently?");
                        if ui.button("Confirm take").clicked() {
                            *action = Some(Drop::Take { item: id.into() });
                            ui.close();
                        }
                    })
                    .response
                    .on_disabled_hover_text(if !ready {
                        reason
                    } else {
                        "Plot items cannot be destroyed."
                    });
                });
            });
        });
    }
    response.on_hover_ui(|ui| {
        ui.strong(name);
        ui.label(label);
        if let Some(row) = row {
            ui.label(row["resref"].as_str().unwrap_or_default());
            ui.label(format!("Stack: {}", row["stack"]));
            if row["plot"] == true {
                ui.weak("Plot item: cannot destroy or drop.");
            }
            if row["droppable"] == false {
                ui.weak("Ordinary creature orders cannot drop this item. A confirmed DM Drop can.");
            }
        }
    })
}

pub(super) fn show(
    app: &mut Moonglow,
    ui: &mut egui::Ui,
    state: &mut LiveInventory,
    rows: &[Value],
    target: &str,
    ready: bool,
) -> Option<Drop> {
    for row in rows {
        let id = row["id"].as_str().unwrap_or_default();
        if !state.icons.contains_key(id) {
            let look = row["look"].as_object().map(|look| {
                let mut root = mg_gff::Struct::new(0);
                for (key, value) in look {
                    if let Some(n) = value.as_i64().and_then(|n| i32::try_from(n).ok()) {
                        root.set(key.as_str(), mg_gff::Value::Int(n));
                    }
                }
                root
            });
            let root = look.or_else(|| {
                row["resref"]
                    .as_str()
                    .and_then(|r| r.parse().ok())
                    .and_then(|r| {
                        crate::area_tools::blueprint(
                            app,
                            mg_resman::ResKey::new(r, mg_core::ResType::UTI),
                        )
                    })
                    .map(|g| g.root)
            });
            let icons = root.map(|g| app.item_icon(ui.ctx(), &g)).unwrap_or_default();
            state.icons.insert(id.into(), icons);
        }
    }
    // Older reports can fall back to the loaded baseitems table. Unknown stays unequippable.
    let rows: Vec<_> = rows
        .iter()
        .cloned()
        .map(|mut row| {
            if row.get("equip_slots").is_none() {
                let base = row["look"]["BaseItem"].as_u64().or_else(|| {
                    let rr = row["resref"].as_str()?.parse().ok()?;
                    crate::area_tools::blueprint(
                        app,
                        mg_resman::ResKey::new(rr, mg_core::ResType::UTI),
                    )?
                    .root
                    .integer("BaseItem")
                    .and_then(|v| u64::try_from(v).ok())
                });
                let mask = base
                    .and_then(|base| {
                        app.game
                            .as_ref()?
                            .table("baseitems")
                            .ok()?
                            .get_int(base as usize, "EquipableSlots")
                    })
                    .unwrap_or(0);
                row["equip_slots"] = serde_json::json!(mask);
            }
            row
        })
        .collect();
    let players = app.live.as_ref().is_some_and(|s| s.policy().players);
    let recipients: Vec<_> = app
        .palette
        .area
        .and_then(|a| app.live.as_ref()?.area(&a.to_string()))
        .map(|a| {
            a.creatures
                .into_iter()
                .filter(|o| {
                    o.id != target && !o.dead && (players || !(o.pc || o.dm || o.possessed))
                })
                .map(|o| (o.id.clone(), format!("{} ({})", o.name, o.id)))
                .collect()
        })
        .unwrap_or_default();
    let reason = if state.pending.is_some() {
        "Wait for the current inventory request."
    } else {
        "Enable player operations in Options and on the server for this creature."
    };
    let context = CellContext { target, recipients: &recipients, ready, reason };
    let mut action = None;
    ui.strong("Equipment");
    egui::Grid::new("live-paperdoll").spacing(egui::vec2(7., 7.)).show(ui, |ui| {
        for line in [
            [Some(6), Some(0), Some(9)],
            [Some(4), Some(1), Some(5)],
            [Some(3), Some(10), Some(2)],
            [Some(7), None, Some(8)],
            [Some(11), Some(12), Some(13)],
        ] {
            for slot in line {
                if let Some(slot) = slot {
                    let row = rows.iter().find(|r| r["slot"].as_i64() == Some(slot));
                    let r = cell(ui, state, &context, row, slot_name(slot), &mut action);
                    if ready
                        && let Some(d) = r.dnd_release_payload::<Dragged>()
                        && d.holder == target
                        && rows.iter().find(|r| r["id"].as_str() == Some(&d.item)).is_some_and(
                            |r| r["equip_slots"].as_u64().unwrap_or(0) & (1 << slot) != 0,
                        )
                    {
                        action = Some(Drop::Equip { item: d.item.clone(), slot: slot as u8 });
                    }
                } else {
                    ui.allocate_space(egui::vec2(52., 60.));
                }
            }
            ui.end_row();
        }
    });
    let hidden: Vec<_> =
        rows.iter().filter(|r| r["slot"].as_i64().is_some_and(|n| n >= 14)).collect();
    if !hidden.is_empty() {
        ui.collapsing("Creature equipment (read-only)", |ui| {
            for row in hidden {
                ui.label(format!(
                    "{}: {}",
                    slot_name(row["slot"].as_i64().unwrap()),
                    row["name"].as_str().unwrap_or("Unnamed item")
                ));
            }
        });
    }
    ui.separator();
    ui.strong("Backpack");
    ui.weak("Right-click for actions. Drag to equip, unequip, or arrange.");
    let mut bag: Vec<_> = rows.iter().filter(|r| r["slot"].as_i64().unwrap_or(-1) < 0).collect();
    bag.sort_by_key(|r| {
        state
            .order
            .iter()
            .position(|id| Some(id.as_str()) == r["id"].as_str())
            .unwrap_or(usize::MAX)
    });
    let columns = ((ui.available_width() / 58.) as usize).clamp(3, 10);
    egui::Grid::new("live-backpack").spacing(egui::vec2(6., 6.)).show(ui, |ui| {
        let count = bag.len().max(columns * 2).div_ceil(columns) * columns;
        for index in 0..count {
            let row = bag.get(index).copied();
            let r = cell(ui, state, &context, row, "Backpack", &mut action);
            if ready
                && let Some(d) = r.dnd_release_payload::<Dragged>()
                && d.holder == target
            {
                action = Some(Drop::Backpack {
                    item: d.item.clone(),
                    before: row.and_then(|r| r["id"].as_str()).map(String::from),
                });
            }
            if (index + 1) % columns == 0 {
                ui.end_row();
            }
        }
    });
    action
}
