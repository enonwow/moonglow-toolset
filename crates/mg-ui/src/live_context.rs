//! One menu structure for single objects and selections. Every send uses the Live queue.
use crate::live_targets::{allowed, creature as actor};
use crate::{
    Action, Moonglow, Tab,
    area_view::AreaView,
    live::{AreaLive, LiveObject, Op},
};
use mg_area::ObjectKind;

const CREATURE_REASON: &str = "Requires a living creature. Players need both player-operation switches; DMs and possessed creatures are excluded.";
const KILL: &str = "The creature dies, triggering its death event and leaving a corpse and loot according to its game settings.";
const REMOVE: &str = "Permanently removes the object from the running game without a corpse or loot. This cannot be undone.";

fn button(ui: &mut egui::Ui, label: &str, enabled: bool, reason: &str) -> bool {
    ui.add_enabled(enabled, egui::Button::new(label)).on_disabled_hover_text(reason).clicked()
}

pub(crate) fn menu(app: &mut Moonglow, view: &mut AreaView, ui: &mut egui::Ui, live: &AreaLive) {
    let mut ids: Vec<_> = view.live.selection_ids().map(str::to_owned).collect();
    ids.sort();
    ui.push_id((view.area, ids), |ui| contents(app, view, ui, live));
}

fn contents(app: &mut Moonglow, view: &mut AreaView, ui: &mut egui::Ui, live: &AreaLive) {
    let objects: Vec<_> = view.live.selection_ids().filter_map(|id| live.find(id)).collect();
    let policy = app.live.as_ref().map(|s| s.policy()).unwrap_or_default();
    let actors: Vec<_> = objects
        .iter()
        .filter(|(k, o)| actor(*k, o, policy.players))
        .map(|(_, o)| o.id.clone())
        .collect();
    let npcs: Vec<_> =
        objects.iter().filter(|(k, o)| actor(*k, o, false)).map(|(_, o)| o.id.clone()).collect();
    let mut chosen = None;
    ui.strong(format!(
        "{} selected - {} allowed",
        objects.len(),
        objects.iter().filter(|(_, o)| allowed(o, policy.players)).count()
    ));
    if objects.iter().any(|(_, o)| o.pc) && !policy.players {
        ui.weak("Player: actions are off").on_hover_text(
            "Options > Live > Allow actions on players, plus the server player switch.",
        );
    }
    movement(view, ui, &actors, "Walk Here…", false, false);
    attack(view, ui, &actors);
    say(view, ui, &objects, &mut chosen);
    let inventories: Vec<_> = objects
        .iter()
        .filter(|(k, o)| *k == ObjectKind::Creature && allowed(o, policy.players))
        .collect();
    if inventories.len() == 1
        && ui.button("Inventory…").on_hover_text("Open Live Inventory for this creature.").clicked()
    {
        view.live.group.focus(&mut view.live.selected, &inventories[0].1.id);
        app.palette.area = Some(view.area);
        app.actions.push(Action::LiveInventory);
        ui.close();
    }
    if ui
        .add_enabled(!app.settings.no_live_focus, egui::Button::new("Focus"))
        .on_hover_text("Watch this object or group, its conversations and events.")
        .on_disabled_hover_text("Enable Focus window in Options > Live.")
        .clicked()
    {
        crate::live_focus::add(app, &objects.iter().map(|(_, o)| o.id.clone()).collect::<Vec<_>>());
        ui.close();
    }
    if let [(ObjectKind::Creature, npc)] = objects.as_slice()
        && !npc.pc
        && !npc.dm
        && !npc.possessed
    {
        if ui
            .button("NPC card")
            .on_hover_text(
                "Edit this NPC's identity, knowledge, secrets and voice for the assistant.",
            )
            .clicked()
        {
            crate::live_assistant::open_npc_card(app, &npc.id);
            ui.close();
        }
        if ui
            .add_enabled(!app.settings.no_live_focus, egui::Button::new("Memory"))
            .on_hover_text("Focus this NPC and open their saved conversation history.")
            .on_disabled_hover_text("Enable Focus window in Options > Live.")
            .clicked()
        {
            crate::live_focus::add(app, std::slice::from_ref(&npc.id));
            app.live_focus.memory_page = true;
            app.live_focus.member = None;
            app.live_focus.area = None;
            app.live_focus.text.clear();
            ui.close();
        }
    }
    ui.separator();
    ui.menu_button("Move", |ui| {
        movement(view, ui, &actors, "Walk Here…", false, false);
        movement(view, ui, &actors, "Run Here…", false, true);
        if objects.len() > 1 {
            ui.separator();
            for (label, formation, run) in [
                ("Walk in Formation…", true, false),
                ("Run in Formation…", true, true),
                ("Walk to One Point…", false, false),
                ("Run to One Point…", false, true),
            ] {
                movement(view, ui, &actors, label, formation, run);
            }
        }
        ui.separator();
        crate::live_teleport::menu(
            app,
            view,
            ui,
            &objects.iter().map(|(_, o)| *o).collect::<Vec<_>>(),
        );
        ui.separator();
        crate::live_patrol::menu(app, view, ui, npcs.clone());
    });
    ui.menu_button("Combat", |ui| {
        attack(view, ui, &actors);
        chosen = crate::live_menu::stop_attack(ui, "0", !actors.is_empty()).or(chosen.take());
        if ui
            .button("Stop Fighting Here (10 m)")
            .on_hover_text(
                "Aim a radius on the map. This affects nearby creatures, not just the selection.",
            )
            .clicked()
        {
            crate::live_aim::start(
                app,
                view,
                Op::StopCombat { target: None, x: None, y: None, radius: Some(10.) },
                "Stop Fighting in radius",
                crate::live_aim::Source::Menu,
            );
            ui.close();
        }
        ui.separator();
        ui.add_enabled_ui(!actors.is_empty(), |ui| {
            chosen = crate::live_menu::freeze(ui, "0", objects.len() == 1 && objects[0].1.frozen)
                .or(chosen.take());
            if objects.len() > 1 && ui.button("Unfreeze").clicked() {
                chosen = Some(Op::Unfreeze { target: "0".into() });
            }
        })
        .response
        .on_disabled_hover_text(CREATURE_REASON);
    });
    ui.menu_button("Character", |ui| {
        ui.add_enabled_ui(!npcs.is_empty(), |ui| {
            ui.menu_button("Faction", |ui| {
                let module = app.module_path();
                let boot = app.live.as_ref().map_or_else(String::new, |s| s.boot());
                app.faction_view.live.reset_context(module, boot);
                let server = app.live.as_ref().and_then(|s| s.factions());
                let local = app.factions();
                egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
                    if let Some(server) = &server {
                        for f in server.iter().filter(|f| f.id != 0) {
                            let label = app.faction_view.live.label_for(f.id, &f.name);
                            if ui.button(format!("{}: {label}", f.id)).clicked() {
                                chosen = Some(Op::SetFaction {
                                    target: "0".into(),
                                    faction: crate::live::Faction::Id(f.id),
                                });
                            }
                        }
                    } else {
                        ui.weak(
                            "Waiting for the server's faction list. Older servers need an update.",
                        );
                    }
                    if let Some(local) = &local {
                        for (id, f) in
                            local.factions.iter().enumerate().skip(1).filter(|(id, _)| {
                                server
                                    .as_ref()
                                    .is_none_or(|s| !s.iter().any(|f| f.id == *id as u32))
                            })
                        {
                            ui.add_enabled(
                                false,
                                egui::Button::new(format!("{id}: {} (editor only)", f.name)),
                            )
                            .on_disabled_hover_text(
                                "Not reported by the server: save, rebuild and restart the module.",
                            );
                        }
                    }
                });
            })
            .response
            .on_disabled_hover_text("Faction changes require a living NPC.");
        });
        ui.separator();

        ui.add_enabled_ui(!actors.is_empty(), |ui| {
            chosen = crate::live_menu::heal(ui, "0", false).or(chosen.take());
            let level = (!objects
                .iter()
                .any(|(k, o)| actor(*k, o, policy.players) && o.level.is_none_or(|n| n > 1)))
            .then_some(1);
            chosen = crate::live_menu::economy(ui, "0", level).or(chosen.take());
            ui.add_enabled_ui(
                objects.iter().any(|(_, o)| o.pc && !o.dead && allowed(o, policy.players)),
                |ui| {
                    for (label, field, max, gold) in [
                        ("Give Gold", "Gold amount", 1_000_000, true),
                        ("Give XP", "XP amount", 100_000, false),
                    ] {
                        crate::live_menu::values(ui, label, |ui| {
                            if let Some(amount) = crate::live_menu::number(
                                ui,
                                "0",
                                field,
                                100,
                                max,
                                &format!("{label} Now"),
                            ) {
                                chosen = Some(if gold {
                                    Op::PcGiveGold { target: "0".into(), amount }
                                } else {
                                    Op::PcGiveXp { target: "0".into(), amount }
                                });
                            }
                        });
                    }
                    if ui.button("Give Level").clicked() {
                        chosen = Some(Op::GiveLevel { target: "0".into() });
                    }
                    ui.menu_button("Alignment", |ui| {
                        for (label, law_chaos, good_evil) in [
                            ("Good +10", 0, 10),
                            ("Evil +10", 0, -10),
                            ("Lawful +10", 10, 0),
                            ("Chaotic +10", -10, 0),
                        ] {
                            if ui.button(label).clicked() {
                                chosen = Some(Op::GiveAlignment {
                                    target: "0".into(),
                                    law_chaos,
                                    good_evil,
                                });
                            }
                        }
                    });
                },
            )
            .response
            .on_disabled_hover_text("Requires a living player and both player-operation switches.");
            ui.separator();
            for name in ["Invulnerable", "Immortal"] {
                ui.menu_button(name, |ui| {
                    for (label, enabled) in [("On", true), ("Off", false)] {
                        if ui.button(label).clicked() {
                            chosen = Some(if name == "Immortal" {
                                Op::SetImmortal { target: "0".into(), enabled }
                            } else {
                                Op::SetInvulnerable { target: "0".into(), enabled }
                            });
                        }
                    }
                });
            }
            ui.menu_button("AI", |ui| {
                use crate::live::AiLevel as L;
                for (label, level) in [
                    ("Default", L::Default),
                    ("Very Low", L::VeryLow),
                    ("Low", L::Low),
                    ("Normal", L::Normal),
                    ("High", L::High),
                    ("Very High", L::VeryHigh),
                ] {
                    if ui.button(label).clicked() {
                        chosen = Some(Op::SetAi { target: "0".into(), level });
                    }
                }
            });
        })
        .response
        .on_disabled_hover_text(CREATURE_REASON);
        if button(
            ui,
            "Raise",
            objects.iter().any(|(_, o)| o.pc && o.dead && allowed(o, policy.players)),
            "Requires a dead player and both player-operation switches.",
        ) {
            chosen = Some(Op::PcRaise { target: "0".into() });
        }
    });
    ui.menu_button("Look and effects", |ui| {
        ui.add_enabled_ui(!actors.is_empty(), |ui| {
            chosen = crate::live_animation::menu(ui, app.game.as_deref(), "0").or(chosen.take());
        })
        .response
        .on_disabled_hover_text(CREATURE_REASON);
        if button(
            ui,
            "Apply VFX…",
            objects.iter().any(|(_, o)| allowed(o, policy.players) && o.is_static != Some(true)),
            "Select an allowed dynamic object. Static placeables must be made dynamic first.",
        ) {
            app.palette.area = Some(view.area);
            app.live_vfx.ground = false;
            app.actions.push(Action::LiveVfx);
            ui.close();
        }
        let appearance = objects
            .iter()
            .filter(|(kind, o)| {
                matches!(kind, ObjectKind::Creature | ObjectKind::Placeable)
                    && allowed(o, policy.players)
            })
            .min_by_key(|(_, o)| Some(&o.id) != view.live.selected.as_ref());
        if button(
            ui,
            "Appearance…",
            appearance.is_some(),
            "Select an allowed creature or placeable. Players need both player-operation switches.",
        ) {
            app.palette.area = Some(view.area);
            let mut gallery = crate::appearance_gallery::Gallery::default();
            gallery.kind = if appearance.unwrap().0 == ObjectKind::Placeable {
                crate::appearance_gallery::Kind::Placeable
            } else {
                crate::appearance_gallery::Kind::Creature
            };
            app.placeable_gallery = Some(gallery);
            app.actions.push(Action::OpenTab(Tab::PlaceableGallery));
            ui.close();
        }
        if objects.len() == 1 {
            situated(app, view, ui, objects[0], &mut chosen);
        }
    });
    ui.menu_button("Talk", |ui| {
        say(view, ui, &objects, &mut chosen);
        if button(
            ui,
            "Skill Check…",
            objects.iter().any(|(k, o)| *k == ObjectKind::Creature && allowed(o, policy.players)),
            CREATURE_REASON,
        ) {
            app.palette.area = Some(view.area);
            app.live_skill.single = None;
            app.actions.push(Action::LiveSkill);
            ui.close();
        }
        player_dialog(app, view, ui, &objects, policy.players, false);
    });
    ui.menu_button("Player", |ui| {
        player_dialog(app, view, ui, &objects, policy.players, true);
        ui.add_enabled_ui(objects.iter().any(|(_, o)| o.pc && allowed(o, policy.players)), |ui| {
            ui.menu_button("Fade", |ui| {
                for (label, direction) in [
                    ("Fade to Black", mg_live::store::FadeDirection::ToBlack),
                    ("Fade from Black", mg_live::store::FadeDirection::FromBlack),
                ] {
                    if ui.button(label).clicked() {
                        chosen = Some(Op::Fade { target: "0".into(), direction, seconds: 1. });
                    }
                }
            });
        })
        .response
        .on_disabled_hover_text("Requires a player and both player-operation switches.");
    });
    ui.menu_button("Scripts", |ui| {
        for (enabled, chunk, label) in
            [(policy.scripts, false, "Run Script…"), (policy.chunks, true, "Run NWScript…")]
        {
            if button(ui, label, enabled, "Enable this script operation in Live Options.") {
                view.live.script = Some(crate::live_view::ScriptDraft {
                    target: None,
                    targets: objects.iter().map(|(_, o)| o.id.clone()).collect(),
                    text: String::new(),
                    chunk,
                    wrap: true,
                });
                ui.close();
            }
        }
        if button(
            ui,
            "Locals (to the Live Log)",
            objects.iter().any(|(_, o)| allowed(o, false)),
            "Select an NPC or world object.",
        ) {
            chosen = Some(Op::DumpLocals { target: "0".into() });
        }
    });
    ui.separator();
    ui.menu_button(egui::RichText::new("DANGER").color(egui::Color32::LIGHT_RED), |ui| {
        for (label, enabled, explanation, op) in [
            ("Kill", !actors.is_empty(), KILL, Op::Kill { target: "0".into() }),
            (
                "Limbo",
                !npcs.is_empty(),
                "Removes the NPC from the area into limbo. Keep its object ID to bring it back.",
                Op::Limbo { target: "0".into() },
            ),
        ] {
            ui.add_enabled_ui(enabled, |ui| {
                ui.menu_button(label, |ui| {
                    ui.set_max_width(280.);
                    ui.label(explanation);
                    if ui.button(format!("Confirm {label}")).clicked() {
                        chosen = Some(op);
                    }
                })
                .response
                .on_hover_text(explanation);
            })
            .response
            .on_disabled_hover_text(CREATURE_REASON);
        }
        if button(
            ui,
            "Remove from game (no body)",
            objects.iter().any(|(kind, o)| crate::live_delete::removable(*kind, o)),
            if objects.iter().any(|(kind, _)| *kind == ObjectKind::Encounter) {
                crate::live_delete::ENCOUNTER_REASON
            } else {
                REMOVE
            },
        ) {
            crate::live_delete::request(app, view);
            ui.close();
        }
        ui.separator();
        if button(ui, "Force Rest", !actors.is_empty(), CREATURE_REASON) {
            chosen = Some(Op::ForceRest { target: "0".into() });
        }
    });
    ui.weak("Eligible targets only; players need both switches.").on_hover_text("Movement/combat need living creatures. Faction, Patrol and Limbo need NPCs. Say also accepts placeables. Inventory opens only for one eligible creature.");
    if let Some(op) = chosen {
        send(app, view, &objects, policy.players, op);
        ui.close();
    }
}

fn movement(
    view: &mut AreaView,
    ui: &mut egui::Ui,
    ids: &[String],
    label: &str,
    formation: bool,
    run: bool,
) {
    if ui
        .add_enabled(!ids.is_empty(), egui::Button::new(label))
        .on_disabled_hover_text(CREATURE_REASON)
        .on_hover_text(
            "Click a map destination; right-click or Esc cancels. Shift-click also runs.",
        )
        .clicked()
    {
        crate::live_group::move_to(view, ids.to_vec(), formation, run);
        ui.close();
    }
}
fn attack(view: &mut AreaView, ui: &mut egui::Ui, ids: &[String]) {
    if button(ui, "Attack…", !ids.is_empty(), CREATURE_REASON) {
        crate::live_group::attack(view, ids.to_vec());
        ui.close();
    }
}
fn say(
    view: &mut AreaView,
    ui: &mut egui::Ui,
    objects: &[(ObjectKind, &LiveObject)],
    chosen: &mut Option<Op>,
) {
    let speakers: Vec<_> = objects
        .iter()
        .filter(|(k, o)| {
            !o.dead
                && allowed(o, false)
                && matches!(k, ObjectKind::Creature | ObjectKind::Placeable)
        })
        .collect();
    if objects.len() == 1 {
        if button(
            ui,
            "Say…",
            !speakers.is_empty(),
            "Say requires an NPC or a placeable; use Message for a player.",
        ) {
            view.live.say = Some((speakers[0].1.id.clone(), String::new()));
            ui.close();
        }
    } else {
        ui.add_enabled_ui(!speakers.is_empty(), |ui| {
            crate::live_menu::values(ui, "Say…", |ui| {
                let id = ui.make_persistent_id((view.area, "group-say"));
                let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
                let label = ui.label("Speech text");
                ui.add(egui::TextEdit::singleline(&mut text).char_limit(1000))
                    .labelled_by(label.id);
                ui.data_mut(|d| d.insert_temp(id, text.clone()));
                if button(ui, "Say Now", !text.trim().is_empty(), "Enter speech text.") {
                    *chosen =
                        Some(Op::NpcSay { target: "0".into(), text, volume: Default::default() });
                }
            });
        });
    }
}
fn player_dialog(
    app: &mut Moonglow,
    view: &mut AreaView,
    ui: &mut egui::Ui,
    objects: &[(ObjectKind, &LiveObject)],
    players: bool,
    boot: bool,
) {
    use crate::live_player::Form;
    let eligible: Vec<_> = objects.iter().filter(|(_, o)| o.pc && allowed(o, players)).collect();
    for (label, form, is_boot) in [
        (
            "Message…",
            Form::Message { text: String::new(), mode: mg_live::store::MessageMode::Chat },
            false,
        ),
        ("Journal…", Form::Journal { quest: String::new(), state: 0 }, false),
        ("Boot…", Form::Boot { reason: String::new() }, true),
    ] {
        if boot == is_boot
            && button(
                ui,
                label,
                eligible.len() == 1,
                "Select one allowed player; enable both player-operation switches.",
            )
        {
            let o = eligible[0].1;
            view.live.player_dialog = Some(crate::live_player::Dialog {
                target: o.id.clone(),
                name: crate::live_panel::name(o).into(),
                boot: app.live.as_ref().map(|s| s.boot()).unwrap_or_default(),
                form,
            });
            ui.close();
        }
    }
}
fn send(
    app: &mut Moonglow,
    view: &mut AreaView,
    objects: &[(ObjectKind, &LiveObject)],
    players: bool,
    op: Op,
) {
    for &(kind, o) in objects {
        if !allowed(o, players) {
            continue;
        }
        let eligible = match &op {
            Op::SetFaction { .. } | Op::Limbo { .. } => actor(kind, o, false),
            Op::NpcSay { .. } => {
                !o.dead && !o.pc && matches!(kind, ObjectKind::Creature | ObjectKind::Placeable)
            }
            Op::Destroy { .. } => crate::live_delete::removable(kind, o),
            Op::DumpLocals { .. } => !o.pc,
            Op::SetState { .. }
            | Op::SetUseable { .. }
            | Op::DisableTrap { .. }
            | Op::SpawnTrapOnObject { .. }
            | Op::SetStatic { .. }
            | Op::SetFacing { .. }
            | Op::SetVisualTransform { .. }
            | Op::PlayAnimation { .. } => matches!(kind, ObjectKind::Placeable | ObjectKind::Door),
            Op::PcRaise { .. } => o.pc && o.dead,
            Op::Fade { .. }
            | Op::PcGiveGold { .. }
            | Op::PcGiveXp { .. }
            | Op::GiveLevel { .. }
            | Op::GiveAlignment { .. } => o.pc,
            Op::RemoveLevel { .. } => actor(kind, o, players) && o.level.is_none_or(|n| n > 1),
            _ => actor(kind, o, players),
        };
        if !eligible {
            continue;
        }
        let mut value = serde_json::to_value(&op).expect("typed command");
        value["target"] = o.id.clone().into();
        if o.pc && matches!(op, Op::Heal { .. }) {
            value["op"] = "pc_heal".into();
        }
        crate::live_view::queue(
            app,
            view,
            serde_json::from_value(value).expect("retargeted command"),
        );
    }
}

fn situated(
    app: &mut Moonglow,
    view: &mut AreaView,
    ui: &mut egui::Ui,
    (kind, o): (ObjectKind, &LiveObject),
    chosen: &mut Option<Op>,
) {
    if !matches!(kind, ObjectKind::Placeable | ObjectKind::Door) {
        return;
    }
    let target = o.id.clone();
    ui.separator();
    if ui.button("Visual Transform…").clicked() {
        view.live.properties.open = true;
        ui.close();
    }
    if o.is_static == Some(true) {
        ui.weak("Static: VFX and visual transforms are unavailable.");
        if ui.button("Make Dynamic").clicked() {
            *chosen = Some(Op::SetStatic { target: target.clone(), r#static: false });
        }
        ui.weak("Game clients must re-enter the area after changing Static.");
    }
    ui.menu_button("Object state", |ui| {
        for (label, open, lock) in [
            ("Open", Some(true), None),
            ("Close", Some(false), None),
            ("Lock", None, Some(true)),
            ("Unlock", None, Some(false)),
        ] {
            if ui.button(label).clicked() {
                *chosen = Some(Op::SetState { target: target.clone(), open, lock });
            }
        }
        let useable = o.useable.unwrap_or(true);
        if ui.button(if useable { "Make Not Useable" } else { "Make Useable" }).clicked() {
            *chosen = Some(Op::SetUseable { target: target.clone(), useable: !useable });
        }
        if ui.button("Disable Trap").clicked() {
            *chosen = Some(Op::DisableTrap { target: target.clone() });
        }
        ui.menu_button("Trap", |ui| {
            if let Some(table) = app.game.as_ref().and_then(|g| g.table("traps").ok()) {
                egui::ScrollArea::vertical().max_height(320.).show(ui, |ui| {
                    for row in 0..table.len() {
                        if let Some(label) =
                            table.get(row, "Label").filter(|l| !l.is_empty() && *l != "****")
                            && ui.button(label).clicked()
                        {
                            *chosen = Some(Op::SpawnTrapOnObject {
                                target: target.clone(),
                                trap: row as u16,
                            });
                        }
                    }
                });
            }
        });
    });
    if kind != ObjectKind::Placeable {
        return;
    }
    ui.menu_button("Turn", |ui| {
        for (label, by) in [("45° Left", 45.), ("45° Right", -45.), ("Around", 180.)] {
            if ui.button(label).clicked() {
                *chosen = Some(Op::SetFacing {
                    target: target.clone(),
                    f: (o.facing + by).rem_euclid(360.),
                });
            }
        }
    });
    ui.add_enabled_ui(o.is_static != Some(true), |ui| {
        ui.menu_button("Visual Transform presets", |ui| {
            for scale in [0.5, 1.5, 2.0] {
                if ui.button(format!("Scale {:.0}%", scale * 100.)).clicked() {
                    *chosen = Some(Op::SetVisualTransform {
                        target: target.clone(),
                        scale: Some(scale),
                        rotate: None,
                        translate: None,
                        reset: false,
                    });
                }
            }
            if ui.button("Reset").clicked() {
                *chosen = Some(Op::SetVisualTransform {
                    target: target.clone(),
                    scale: None,
                    rotate: None,
                    translate: None,
                    reset: true,
                });
            }
        });
    });
    ui.menu_button("Object animation", |ui| {
        for (label, name) in [
            ("Activate", crate::live::Animation::Activate),
            ("Deactivate", crate::live::Animation::Deactivate),
        ] {
            if ui.button(label).clicked() {
                *chosen = Some(Op::PlayAnimation { target: target.clone(), name });
            }
        }
    });
}
