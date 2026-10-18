use super::*;

fn show_on_map(app: &mut Moonglow, members: &[Member]) {
    let Some(first) = members.iter().find(|m| m.reported) else { return };
    let Ok(area) = first.area.parse() else { return };
    let members: Vec<_> = members.iter().filter(|m| m.reported && m.area == first.area).collect();
    let centre = members.iter().map(|m| glam::Vec3::from_array(m.position)).sum::<glam::Vec3>()
        / members.len() as f32;
    crate::area_view::ensure_live(app, area);
    app.actions.push(Action::OpenTab(Tab::Area(area)));
    app.palette.area = Some(area);
    if let Some(view) = app.area_views.get_mut(&area) {
        view.live.on = true;
        for (i, m) in members.iter().enumerate() {
            view.live.select(Some(m.id.clone()), i != 0);
        }
        if let Some(orbit) = &mut view.orbit {
            orbit.target = centre;
        }
    }
    app.live_panel.follow =
        Some(crate::live_panel::Follow { area, id: first.id.clone(), boot: first.boot.clone() });
}

pub(crate) fn reply_orders(
    store: &LiveStore,
    members: &[Member],
    group: bool,
    volume: mg_live::store::SpeechVolume,
    text: &str,
) -> Result<Vec<mg_live::store::Command>, String> {
    let recipients: Vec<_> = members.iter().filter(|m| m.reported && (!group || m.pc)).collect();
    if recipients.is_empty() {
        return Err(if group {
            "No reported player in this group."
        } else {
            "The recipient is not reported."
        }
        .into());
    }
    recipients
        .into_iter()
        .map(|m| crate::live_chat::target_order(store, &m.id, volume, text))
        .collect()
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("window.focus");
    ui.heading("Focus");
    if app.settings.no_live_focus {
        ui.label("Focus is off. Enable Focus window in Options > Live.");
        return;
    }
    ui.weak("Watch conversations and events. Right-click an object or selection > Focus.");
    ui.separator();
    // Temporarily move state so portraits can borrow the resource loader.
    let mut focus = std::mem::take(&mut app.live_focus);
    if let Some(note) = &focus.notice {
        ui.colored_label(ui.visuals().warn_fg_color, note);
    }
    if focus.entries.is_empty() {
        ui.label("No focused objects yet.");
        ui.label("Select a player, NPC or group on the Live map, then choose Focus in its menu.");
        app.live_focus = focus;
        return;
    }
    let mut save = false;
    let mut remove = None;
    let mut map = None;
    let width = (ui.available_width() * 0.36).clamp(145., 260.);
    egui::Panel::left("focus-watch-list")
        .resizable(true)
        .default_size(width)
        .min_size(130.)
        .frame(egui::Frame::NONE.inner_margin(4.))
        .show(ui, |ui| {
            ui.strong("Watching");
            egui::ScrollArea::vertical().id_salt("focus-entries").show(ui, |ui| {
                for (i, entry) in focus.entries.iter().enumerate() {
                    let reported = entry.members.iter().filter(|m| m.reported).count();
                    ui.push_id(i, |ui| {
                        ui.horizontal_top(|ui| {
                            let m = &entry.members[0];
                            let portrait = crate::live_selection::portrait(
                                app,
                                ui,
                                m.portrait.as_deref(),
                                m.appearance,
                                m.gender,
                                &entry.name,
                            );
                            ui.vertical(|ui| {
                                let mut text = egui::RichText::new(&entry.name);
                                if reported == 0 {
                                    text = text.color(ui.visuals().weak_text_color());
                                }
                                if ui.selectable_label(focus.selected == Some(i), text).clicked()
                                    || portrait.clicked()
                                {
                                    focus.selected = Some(i);
                                    focus.member = None;
                                    focus.area = None;
                                    focus.text.clear();
                                }
                                if entry.unread > 0 {
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        format!("{} unread", entry.unread),
                                    );
                                }
                                if reported == 0 {
                                    ui.weak("Not reported");
                                } else if entry.members.len() == 1 {
                                    crate::live_backend::chips(app, ui, &m.id);
                                    ui.small(m.hp.map_or_else(
                                        || "HP unknown".into(),
                                        |[a, b]| format!("HP {a}/{b}"),
                                    ));
                                    ui.small(format!(
                                        "{}{}",
                                        m.area,
                                        if m.combat { " · Combat" } else { "" }
                                    ));
                                } else {
                                    ui.small(format!(
                                        "{reported}/{} reported",
                                        entry.members.len()
                                    ));
                                    let hp: Vec<_> =
                                        entry.members.iter().filter_map(|m| m.hp).collect();
                                    if !hp.is_empty() {
                                        ui.small(format!(
                                            "HP {}/{}",
                                            hp.iter().map(|h| h[0]).sum::<i32>(),
                                            hp.iter().map(|h| h[1]).sum::<i32>()
                                        ));
                                    }
                                    let mut areas: Vec<_> = entry
                                        .members
                                        .iter()
                                        .filter(|m| m.reported)
                                        .map(|m| m.area.as_str())
                                        .collect();
                                    areas.sort();
                                    areas.dedup();
                                    ui.small(areas.join(", "));
                                    if entry.members.iter().any(|m| m.combat) {
                                        ui.small("Combat");
                                    }
                                }
                            });
                        });
                        ui.separator();
                    });
                }
            });
        });
    egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(6.)).show(ui,|ui| {
        let Some(index)=focus.selected.filter(|i|*i<focus.entries.len()) else {return};
        let entry=&mut focus.entries[index];
        if focus.member.as_ref().is_some_and(|id|!entry.members.iter().any(|m|&m.id==id)) {focus.member=None;}
        if focus.member.is_some() && ui.button("Back to group").clicked() {focus.member=None;focus.area=None;}
        let members:Vec<_>=entry.members.iter().filter(|m|focus.member.as_ref().is_none_or(|id|&m.id==id)).cloned().collect();
        ui.horizontal_wrapped(|ui| {
            ui.strong(if focus.member.is_some() {&members[0].name}else{&entry.name});
            egui::containers::menu::MenuButton::new("Manage")
                .config(egui::containers::menu::MenuConfig::default()
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside))
                .ui(ui,|ui| {
                ui.label("Focus name");
                let key=ui.id().with(("rename",index,&entry.name,&entry.members[0].id));
                let mut name=ui.data_mut(|d|d.get_temp::<String>(key)).unwrap_or_else(||entry.name.clone());
                ui.add(egui::TextEdit::singleline(&mut name).char_limit(80).hint_text("Entry name"));
                if ui.add_enabled(!name.trim().is_empty(),egui::Button::new("Rename"))
                    .on_disabled_hover_text("Enter a name first.").clicked() {
                    entry.name=name.trim().to_owned(); save=true; ui.close();
                }
                ui.data_mut(|d|d.insert_temp(key,name));
                if ui.button("Remove from focus").clicked() {remove=Some(index);ui.close();}
            });
            let first=members.iter().find(|m|m.reported);
            if ui.add_enabled(first.is_some(),egui::Button::new("Show on map"))
                .on_disabled_hover_text("No member is currently reported.").clicked() {map=Some(members.clone());}
        });
        if entry.members.len()>1 && focus.member.is_none() {
            ui.collapsing("Members",|ui| {
                for m in &entry.members {
                    ui.horizontal(|ui| {
                        crate::live_backend::chips(app, ui, &m.id);
                        let portrait=crate::live_selection::portrait(app,ui,m.portrait.as_deref(),m.appearance,m.gender,&m.name);
                        if ui.button(&m.name).on_hover_text(format!("{} · {}",m.area,if m.reported {"reported"}else{"not reported"})).clicked() || portrait.clicked() {
                        focus.member=Some(m.id.clone());focus.area=None;focus.text.clear();
                    }
                    });
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.selectable_label(!focus.memory_page&&!focus.events_page,"Conversation").clicked() {focus.events_page=false;focus.memory_page=false;}
            if ui.selectable_label(!focus.memory_page&&focus.events_page,"Events").clicked() {focus.events_page=true;focus.memory_page=false;}
            if members.len()==1 && !members[0].pc {
                if ui.selectable_label(focus.memory_page,"Memory").clicked() {focus.memory_page=true;}
                crate::live_assistant::npc_button(app,ui,&members[0].id);
            }
        });
        if members.len()==1 {ui.collapsing("DM note",|ui|crate::live_notes::editor(app,ui,&members[0].id));}
        if focus.memory_page && members.len()==1 && !members[0].pc {let memory_id=if members[0].tag.is_empty() {members[0].id.clone()}else{format!("tag:{}",members[0].tag)};crate::live_assistant::memory(app,ui,&memory_id);return;}
        if !focus.events_page {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut focus.conversation.nearby,"Nearby / incoming speech")
                    .on_hover_text("Include captured speech by others nearby, area shouts and messages addressed to this entry.");
                ui.checkbox(&mut focus.conversation.server,"Server messages")
                    .on_hover_text("Include server feedback and acknowledged DM messages to this entry.");
            });
        }
        let areas=crate::live_areas::choices(app,entry.rows.iter().filter(|r|focus.member.as_ref().is_none_or(|id|r.members.contains(id))).map(|r|r.event.area.as_str()).chain(members.iter().map(|m|m.area.as_str())));
        crate::live_areas::tabs(ui,&mut focus.area,&areas,true,false);
        // Keep unread until the selected conversation/events is actually drawn.
        for row in &mut entry.rows {
            if focus.member.as_ref().is_none_or(|id|row.members.contains(id)) && focus.area.as_ref().is_none_or(|a|row.event.area==*a)
                && (!matches!(row.event.kind.as_str(),"chat"|"dialog"|"dm_message"))==focus.events_page && (focus.events_page || focus.conversation.shows(row,&members)) {row.read=true;}
        }
        entry.unread=entry.rows.iter().filter(|r|!r.read).count();
        ui.separator();
        let rows:Vec<_>=entry.rows.iter().filter(|r|focus.member.as_ref().is_none_or(|id|r.members.contains(id)) && focus.area.as_ref().is_none_or(|a|r.event.area==*a)).filter(|r|(!matches!(r.event.kind.as_str(),"chat"|"dialog"|"dm_message"))==focus.events_page).filter(|r|focus.events_page || focus.conversation.shows(r,&members)).collect();
        let targets:Vec<_>=members.iter().map(|m|m.id.clone()).collect();
        let draft_height=if focus.events_page {0.} else {crate::live_drafts::reserved(app,focus.area.as_deref(),Some(&targets),false)};
        let colors=app.live_log.colors;
        egui::ScrollArea::vertical().id_salt(("focus-history",index,focus.events_page))
            .auto_shrink(false).stick_to_bottom(true).max_height((ui.available_height()-145.-draft_height).max(60.))
            .show(ui,|ui| {
                if rows.is_empty() {ui.weak(if focus.events_page {"No captured events for this entry yet."}else{"No captured conversation for this entry yet."});}
                for row in rows {
                    let area=if focus.area.is_none() {format!("[{}] ",areas.iter().find(|a|a.id==row.event.area).map(|a|a.name.as_str()).unwrap_or("Server"))}else{String::new()};
                    let mut text=egui::RichText::new(format!("{area}{}  {}",crate::live_log::clock(row.event.at_ms),crate::live_log::attributed(&row.text,&row.event)));
                    if let Some(color)=row.speaker.and_then(|s|colors[s as usize]) {text=text.color(color);}
                    ui.label(text);
                }
            });
        ui.separator();
        if !focus.events_page {crate::live_drafts::ui(app,ui,focus.area.as_deref(),Some(&targets),false);}
        if focus.conversation.nearby && focus.nearby_estimated {ui.small("Nearby: approximate 20 m, using the speaker's event position and the watcher's latest report.");}
        if focus.conversation.nearby && focus.nearby_missing {ui.small("Older speech without a position is shown only for its speaker or recipient.");}
        ui.collapsing("Capture limits",|ui| {
            ui.small("Only events captured by the running server are shown. Nearby speech and area shouts use latest watcher reports; matches stay fixed when received and do not prove who heard a line. Older nearby speech cannot be reconstructed when a watch is first added. Unresolved tells cannot be assigned to a recipient. Dialogue (.dlg) lines require opt-in server instrumentation. DM replies are command acknowledgements, not client delivery receipts. AI shouts are hidden. Item rows currently cover acknowledged DM inventory commands; ordinary game looting is not reported. History is limited to 500 events per entry; it is not saved on disk.");
        });
        let group=members.len()>1;
        ui.strong(if group {"Reply to the group's players"}else if members[0].pc {"Reply to the player"}else{"Speak as the NPC"});
        if !group && !members[0].pc {
            ui.horizontal(|ui| {
                for (v,label) in [(mg_live::store::SpeechVolume::Talk,"Talk"),(mg_live::store::SpeechVolume::Whisper,"Whisper"),(mg_live::store::SpeechVolume::Shout,"Shout")] {
                    ui.selectable_value(&mut focus.volume,v,label);
                }
            });
        }
        let orders=app.live.as_ref().ok_or_else(||"Start Live to reply.".to_string())
            .and_then(|store|reply_orders(store,&members.iter().filter(|m|focus.area.as_ref().is_none_or(|a|m.area==*a)).cloned().collect::<Vec<_>>(),group,focus.volume,&focus.text));
        let reason=orders.as_ref().err().cloned();
        let mut send=false;
        ui.horizontal(|ui| {
            let input=ui.add(egui::TextEdit::singleline(&mut focus.text).hint_text("Focus reply (Enter sends)").desired_width((ui.available_width()-90.).max(40.)));
            send=ui.add_enabled(reason.is_none(),egui::Button::new("Send reply"))
                .on_disabled_hover_text(reason.as_deref().unwrap_or_default()).clicked()
                || (reason.is_none() && input.lost_focus() && ui.input(|i|i.key_pressed(egui::Key::Enter)));
        });
        if send && let Ok(commands)=orders {
            let mut failed=None;
            for command in commands {
                if let Err(e)=crate::live_scene::enqueue(app,command) {failed=Some(e.to_string());break;}
            }
            focus.notice=failed;
            if focus.notice.is_none() {focus.text.clear();}
        }
    });
    if let Some(index) = remove {
        focus.entries.remove(index);
        focus.member = None;
        focus.area = None;
        focus.selected =
            (!focus.entries.is_empty()).then_some(index.min(focus.entries.len().saturating_sub(1)));
        save = true;
    }
    if save && let Err(e) = focus.save() {
        focus.notice = Some(format!("Focus list was not saved: {e}"));
    }
    if let Some(member) = map {
        show_on_map(app, &member);
    }
    app.live_focus = focus;
}
