//! Reported objects, with the same filters as the area's Live markers.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use mg_area::ObjectKind;
use mg_core::{ResRef, ResType};

use crate::live::{AreaLive, LiveObject};
use crate::{Action, Moonglow, Tab};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    Players,
    Npcs,
    Own,
    Hostile,
    Combat,
    Dead,
    Wounded,
    Placeables,
    Doors,
    Items,
}

impl Filter {
    pub const ALL: [Self; 10] = [
        Self::Players,
        Self::Npcs,
        Self::Own,
        Self::Hostile,
        Self::Combat,
        Self::Dead,
        Self::Wounded,
        Self::Placeables,
        Self::Doors,
        Self::Items,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Players => "Players",
            Self::Npcs => "NPCs",
            Self::Own => "Own",
            Self::Hostile => "Hostile",
            Self::Combat => "In combat",
            Self::Dead => "Dead",
            Self::Wounded => "Wounded",
            Self::Placeables => "Placeables",
            Self::Doors => "Doors",
            Self::Items => "Items",
        }
    }

    fn category(self) -> bool {
        matches!(self, Self::Players | Self::Npcs | Self::Placeables | Self::Doors | Self::Items)
    }

    fn matches(self, kind: ObjectKind, o: &LiveObject, selected: Option<&LiveObject>) -> bool {
        match self {
            Self::Players => kind == ObjectKind::Creature && (o.pc || o.dm || o.possessed),
            Self::Npcs => kind == ObjectKind::Creature && !o.pc && !o.dm && !o.possessed,
            Self::Own => o.own,
            Self::Hostile => selected
                .filter(|s| s.pc)
                .map_or(!o.hostile.is_empty(), |s| o.hostile.contains(&s.id)),
            Self::Combat => o.combat,
            Self::Dead => o.dead,
            Self::Wounded => {
                o.hp.is_some_and(|[hp, max]| max > 0 && i64::from(hp) * 2 < i64::from(max))
            }
            Self::Placeables => kind == ObjectKind::Placeable,
            Self::Doors => kind == ObjectKind::Door,
            Self::Items => kind == ObjectKind::Item,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LiveFilter {
    pub search: String,
    pub enabled: HashSet<Filter>,
}

impl LiveFilter {
    /// Kinds are alternatives; state chips narrow the chosen kinds together.
    pub fn matches(
        &self,
        kind: ObjectKind,
        object: &LiveObject,
        selected: Option<&LiveObject>,
    ) -> bool {
        self.matches_with_note(kind, object, selected, "")
    }
    pub(crate) fn matches_with_note(
        &self,
        kind: ObjectKind,
        object: &LiveObject,
        selected: Option<&LiveObject>,
        note: &str,
    ) -> bool {
        let search = self.search.trim().to_lowercase();
        (search.is_empty()
            || [&object.name, &object.tag, &object.resref, note]
                .iter()
                .any(|s| s.to_lowercase().contains(&search)))
            && (!self.enabled.iter().any(|f| f.category())
                || self
                    .enabled
                    .iter()
                    .filter(|f| f.category())
                    .any(|f| f.matches(kind, object, selected)))
            && self
                .enabled
                .iter()
                .filter(|f| !f.category())
                .all(|f| f.matches(kind, object, selected))
    }
}

#[derive(Debug, Clone)]
pub struct Follow {
    pub area: ResRef,
    pub id: String,
    pub boot: String,
}

#[derive(Debug, Default)]
pub struct LivePanel {
    pub follow: Option<Follow>,
    pub notice: Option<String>,
    follow_gap: Option<(String, f64)>,
    actions: Option<HashMap<i32, String>>,
    pub sort: Sort,
    pub descending: bool,
    order: Vec<String>,
    order_area: Option<ResRef>,
    order_until: f64,
    report_source: Weak<crate::live::LiveStore>,
    report_stamp: Option<(ResRef, String, u64)>,
    report: Option<Arc<AreaLive>>,
}

impl LivePanel {
    /// A separate cursor from the map's report cache: reading this window must
    /// never consume the change that triggers map reconciliation.
    fn report(
        &mut self,
        store: &Arc<crate::live::LiveStore>,
        area: ResRef,
    ) -> Option<Arc<AreaLive>> {
        let (boot, seq) = store.revision();
        let stamp = (area, boot, seq);
        let source = Arc::downgrade(store);
        if !self.report_source.ptr_eq(&source) || self.report_stamp.as_ref() != Some(&stamp) {
            self.report = store.area(&area.to_string()).map(Arc::new);
            self.report_source = source;
            self.report_stamp = Some(stamp);
        }
        self.report.clone()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Camera,
    Name,
    Hp,
    Distance,
}

fn compare(
    sort: Sort,
    descending: bool,
    a: &LiveObject,
    b: &LiveObject,
    camera: glam::Vec3,
    selected: glam::Vec3,
) -> std::cmp::Ordering {
    let order = match sort {
        Sort::Camera => {
            a.position().distance_squared(camera).total_cmp(&b.position().distance_squared(camera))
        }
        Sort::Name => name(a).to_lowercase().cmp(&name(b).to_lowercase()),
        Sort::Hp => match (a.hp, b.hp) {
            (Some([a, _]), Some([b, _])) => a.cmp(&b),
            _ => return a.hp.is_none().cmp(&b.hp.is_none()).then_with(|| a.id.cmp(&b.id)),
        },
        Sort::Distance => a
            .position()
            .distance_squared(selected)
            .total_cmp(&b.position().distance_squared(selected)),
    };
    (if descending { order.reverse() } else { order }).then_with(|| a.id.cmp(&b.id))
}

fn header(ui: &mut egui::Ui, panel: &mut LivePanel, name: &str, sort: Sort) {
    let arrow = if panel.sort != sort {
        ""
    } else if panel.descending {
        " ↓"
    } else {
        " ↑"
    };
    if ui.button(format!("{name}{arrow}")).clicked() {
        panel.descending = panel.sort == sort && !panel.descending;
        panel.sort = sort;
        panel.order_until = 0.;
    }
}

pub fn name(o: &LiveObject) -> &str {
    if !o.name.is_empty() {
        &o.name
    } else if !o.resref.is_empty() {
        &o.resref
    } else {
        &o.id
    }
}

pub(crate) fn kind_name(kind: ObjectKind, o: &LiveObject) -> &'static str {
    match kind {
        ObjectKind::Creature if o.dm || o.possessed => "DM",
        ObjectKind::Creature if o.pc => "Player",
        ObjectKind::Creature => "NPC",
        ObjectKind::Placeable => "Placeable",
        ObjectKind::Door => "Door",
        ObjectKind::Item => "Item",
        ObjectKind::Waypoint => "Waypoint",
        ObjectKind::Sound => "Sound",
        ObjectKind::Encounter => "Encounter",
        ObjectKind::Store => "Store",
        ObjectKind::Trigger => "Trigger",
    }
}

fn action_names(source: &[u8]) -> HashMap<i32, String> {
    mg_script::spec::Spec::parse(source)
        .constants
        .into_iter()
        .filter_map(|c| {
            let name = c.name.strip_prefix("ACTION_")?;
            // ACTION_MODE_* is a different enum, with colliding numeric values.
            if name.starts_with("MODE_") {
                return None;
            }
            let value = c.value.parse().ok()?;
            let label = match name {
                "MOVETOPOINT" => "Move to point",
                "PICKUPITEM" => "Pick up item",
                "DROPITEM" => "Drop item",
                "ATTACKOBJECT" => "Attack",
                "CASTSPELL" => "Cast spell",
                "OPENLOCK" => "Open lock",
                "LOCK" => "Lock",
                "USEOBJECT" => "Use object",
                "ANIMALEMPATHY" => "Animal empathy",
                "REST" => "Rest",
                "TAUNT" => "Taunt",
                "SMITEGOOD" => "Smite good",
                "EXAMINETRAP" => "Examine trap",
                "DISABLETRAP" => "Disable trap",
                "RECOVERTRAP" => "Recover trap",
                "FLAGTRAP" => "Flag trap",
                "EXAMINE" => "Examine",
                "SETTRAP" => "Set trap",
                "WAIT" => "Wait",
                "PAUSE" => "Pause",
                "OPENDOOR" => "Open door",
                "CLOSEDOOR" => "Close door",
                "DIALOGOBJECT" => "Dialogue",
                "FOLLOW" => "Follow",
                "KIDAMAGE" => "Ki damage",
                "RANDOMWALK" => "Random walk",
                "ITEMCASTSPELL" => "Use item spell",
                "COUNTERSPELL" => "Counterspell",
                "HEAL" => "Heal",
                "PICKPOCKET" => "Pick pocket",
                "FORCEFOLLOWOBJECT" => "Force follow",
                "SIT" => "Sit",
                "INVALID" => "None",
                _ => name,
            };
            Some((value, label.to_owned()))
        })
        .collect()
}

/// Uses the currently open area's report, never the editable GIT.
pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui) {
    #[cfg(feature = "measure")]
    let _measure = crate::measure::span("window.panel");
    ui.heading("Live Panel");
    crate::live_backend::status_ui(app, ui);
    ui.separator();
    // The dock is moved out of `app` while tab contents are drawn.
    let Some(area) = app
        .palette
        .area
        .filter(|a| app.panes_tabs.iter().any(|tabs| tabs.contains(&Tab::Area(*a))))
    else {
        ui.label("Open an area to see its reported objects.");
        return;
    };
    let live = {
        #[cfg(feature = "measure")]
        let _measure = crate::measure::span("window.panel_report");
        app.live.as_ref().and_then(|s| app.live_panel.report(s, area))
    };
    let Some(live) = live else {
        ui.label("No live report for this area.");
        return;
    };
    if app.live_panel.actions.is_none() {
        app.live_panel.actions = app
            .game
            .as_ref()
            .and_then(|g| g.resman.get_named("nwscript", ResType::NSS).ok())
            .map(|s| action_names(&s));
    }
    let Some(view) = app.area_views.get_mut(&area) else { return };
    ui.horizontal_wrapped(|ui| {
        ui.strong(format!("Area: {area}"));
        ui.weak(format!(
            "Report: {:.1} s ago",
            crate::live_view::now_ms().saturating_sub(live.received_ms) as f32 / 1000.
        ));
        if ui
            .selectable_label(app.live_panel.sort == Sort::Camera, "Near camera")
            .on_hover_text("Sort by distance from the camera target")
            .clicked()
        {
            app.live_panel.sort = Sort::Camera;
            app.live_panel.descending = false;
            app.live_panel.order_until = 0.;
        }
        if app.live_panel.follow.is_some() && ui.button("Stop following").clicked() {
            app.live_panel.follow = None;
        }
    });
    if let Some(note) = &app.live_panel.notice {
        ui.label(note);
    }
    ui.horizontal(|ui| {
        let label = ui.label("Search live objects");
        ui.add(
            egui::TextEdit::singleline(&mut view.live.filter.search)
                .hint_text("Name, tag, resref or DM note")
                .desired_width(f32::INFINITY),
        )
        .labelled_by(label.id);
    });
    ui.horizontal_wrapped(|ui| {
        for filter in Filter::ALL {
            let mut on = view.live.filter.enabled.contains(&filter);
            if ui.toggle_value(&mut on, filter.name()).changed() {
                if on {
                    view.live.filter.enabled.insert(filter);
                } else {
                    view.live.filter.enabled.remove(&filter);
                }
            }
        }
        if ui.button("Clear filters").clicked() {
            view.live.filter = LiveFilter::default();
        }
    });
    ui.checkbox(&mut view.live.hide_module_markers, "Hide module markers")
        .on_hover_text("Hide the module editor's markers while showing reported Live objects.");
    ui.collapsing("Filters and selection help", |ui| {
        ui.label("Filters also hide markers on the map. Kind chips combine; state chips narrow the results.");
        ui.label("Sound markers currently cover sounds created through Live.");
        ui.label("Wounded: below 50% HP. Hostile: to the selected player, otherwise any player.");
        ui.label("Click a name to select and centre; double-click to follow. Ctrl+click toggles selection.");
    });
    ui.strong(format!("{} selected", view.live.selection_ids().count()));
    let selected = view.live.selected.as_ref().and_then(|id| live.find(id)).map(|(_, o)| o);
    let mut objects: Vec<_> = live
        .all()
        .filter(|(k, o)| {
            view.show[k.index()]
                && view.live.filter.matches_with_note(
                    *k,
                    o,
                    selected,
                    view.live.notes.get(&o.id).map_or("", String::as_str),
                )
        })
        .collect();
    let camera = view.camera().map(|c| c.target).unwrap_or_default();
    let reference = selected.map(|o| o.position()).unwrap_or(camera);
    objects.sort_by(|(_, a), (_, b)| {
        compare(app.live_panel.sort, app.live_panel.descending, a, b, camera, reference)
    });
    let time = ui.input(|i| i.time);
    // Centring the camera on the first click must not move that row before
    // the second click of a follow gesture.
    if app.live_panel.order_area == Some(area) && time < app.live_panel.order_until {
        objects.sort_by_key(|(_, o)| {
            app.live_panel.order.iter().position(|id| *id == o.id).unwrap_or(usize::MAX)
        });
    } else {
        app.live_panel.order_area = Some(area);
        app.live_panel.order = objects.iter().map(|(_, o)| o.id.clone()).collect();
    }
    ui.label(format!("{} of {} reported objects", objects.len(), live.all().count()));
    ui.separator();
    if objects.is_empty() {
        if live.all().next().is_some() {
            ui.label("No objects match the current filters.");
            ui.weak("Clear filters above; check the area toolbar's kind toggles if objects are still hidden.");
        } else {
            ui.label("No objects reported in this area yet.");
        }
        return;
    }
    let mut clicked = None;
    egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
        egui::Grid::new(("live-objects", area)).striped(true).min_col_width(40.).show(ui, |ui| {
            header(ui, &mut app.live_panel, "Name", Sort::Name);
            ui.strong("Tag");
            ui.strong("Kind");
            header(ui, &mut app.live_panel, "HP", Sort::Hp);
            ui.strong("Action");
            ui.strong("State");
            header(ui, &mut app.live_panel, "Distance", Sort::Distance);
            ui.end_row();
            for (kind, o) in objects {
                ui.push_id(&o.id, |ui| {
                    let response = ui.selectable_label(view.live.is_selected(&o.id), name(o));
                    if response.clicked() || response.double_clicked() {
                        clicked = Some((
                            o.id.clone(),
                            response.double_clicked(),
                            ui.input(|i| i.modifiers.ctrl || i.modifiers.command),
                        ));
                    }
                    response.on_hover_text(format!("{} · {}", o.id, o.resref));
                });
                ui.label(&o.tag);
                ui.label(kind_name(kind, o));
                ui.label(o.hp.map_or_else(|| "—".into(), |[hp, max]| format!("{hp}/{max}")));
                ui.label(if kind == ObjectKind::Creature {
                    app.live_panel
                        .actions
                        .as_ref()
                        .and_then(|a| a.get(&o.act))
                        .cloned()
                        .unwrap_or_else(|| format!("Action {}", o.act))
                } else {
                    "—".into()
                });
                let state = [(o.dead, "dead"), (o.combat, "in combat"), (o.own, "own")]
                    .into_iter()
                    .filter_map(|(yes, name)| yes.then_some(name))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.label(state);
                ui.label(format!("{:.1} m", reference.distance(o.position()))).on_hover_text(
                    if selected.is_some() {
                        "From the selected object"
                    } else {
                        "From the camera target"
                    },
                );
                ui.end_row();
            }
        });
    });
    if let Some((id, follow, toggle)) = clicked {
        app.live_panel.order_until = time + 0.5;
        select(app, area, &live, &id, follow, toggle);
    }
}

fn select(
    app: &mut Moonglow,
    area: ResRef,
    live: &AreaLive,
    id: &str,
    following: bool,
    toggle: bool,
) {
    let Some((_, object)) = live.find(id) else { return };
    if let Some(view) = app.area_views.get_mut(&area) {
        view.live.on = true;
        view.live.select(Some(id.into()), toggle);
        if !toggle && let Some(orbit) = &mut view.orbit {
            orbit.target = object.position();
        }
    }
    app.live_panel.notice = None;
    app.live_panel.follow = following.then(|| Follow {
        area,
        id: id.into(),
        boot: app.live.as_ref().unwrap().status(crate::live_view::now_ms())["boot"]
            .as_str()
            .unwrap_or_default()
            .into(),
    });
}

pub(crate) fn update_follow(app: &mut Moonglow, now: f64) {
    let Some(mut follow) = app.live_panel.follow.take() else {
        app.live_panel.follow_gap = None;
        return;
    };
    let stop = |app: &mut Moonglow, note: String| {
        if let Some(v) = app.area_views.get_mut(&follow.area) {
            v.notice = Some(note.clone());
        }
        app.live_panel.notice = Some(note);
    };
    if app.dock.find_tab(&Tab::Area(follow.area)).is_none()
        || !app.area_views.get(&follow.area).is_some_and(|v| v.live.on)
    {
        stop(app, "Stopped following: the Live area view closed or was switched off.".into());
        return;
    }
    let Some(store) = app.live.clone() else { return };
    if store.boot() != follow.boot {
        stop(app, "Stopped following: the game restarted.".into());
        return;
    }
    let Some((area, _, object)) = store.locate(&follow.id) else {
        let since = app
            .live_panel
            .follow_gap
            .as_ref()
            .filter(|(id, _)| id == &follow.id)
            .map_or(now, |(_, t)| *t);
        app.live_panel.follow_gap = Some((follow.id.clone(), since));
        if now - since < 10. {
            app.live_panel.notice =
                Some("Waiting for the followed object’s next area report...".into());
            app.live_panel.follow = Some(follow);
        } else {
            stop(app, "Stopped following: the object was not reported for 10 seconds.".into());
        }
        return;
    };
    if app.live_panel.follow_gap.take().is_some() {
        app.live_panel.notice = None;
    }
    let area: ResRef = match area.parse() {
        Ok(a) => a,
        Err(_) => return,
    };
    if area != follow.area {
        crate::area_view::ensure_live(app, area);
        app.run_now(Action::OpenTab(Tab::Area(area)));
        app.palette.area = Some(area);
        // Histories follow unless they are independent pinned Chat windows.
        app.live_chat.area = Some(area.to_string());
        app.live_log.area = Some(area.to_string());
        app.live_focus.area = Some(area.to_string());
        follow.area = area;
        app.live_panel.notice = Some(format!("Following {} in {area}", name(&object)));
    }
    if let Some(view) = app.area_views.get_mut(&area) {
        view.live.on = true;
        view.live.select(Some(follow.id.clone()), false);
        if let Some(live) = store.area(&area.to_string()) {
            view.live.motion.observe(&live, &follow.boot, now);
        }
        if let Some(orbit) = &mut view.orbit {
            orbit.target =
                view.live.motion.position(&follow.id, now).unwrap_or_else(|| object.position());
        }
    }
    app.live_panel.follow = Some(follow);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_panel_report_refreshes_on_report_area_boot_and_source() {
        let store = Arc::new(crate::live::LiveStore::default());
        let sync = |store: &crate::live::LiveStore, boot: &str, seq, name| {
            store
                .sync(
                    &serde_json::to_vec(&serde_json::json!({
                        "v":1,"boot":boot,"seq":seq,"areas":[
                            {"area":"a","full":true,"cre":[{"id":"10","name":name}]},
                            {"area":"b","full":true,"cre":[{"id":"20","name":"Other"}]}
                        ]
                    }))
                    .unwrap(),
                    seq,
                )
                .unwrap();
        };
        let mut panel = LivePanel::default();
        let a = "a".parse().unwrap();
        assert!(panel.report(&store, a).is_none());
        sync(&store, "first", 1, "Old");
        let old = panel.report(&store, a).unwrap();
        assert_eq!(old.find("10").unwrap().1.name, "Old");
        assert!(Arc::ptr_eq(&old, &panel.report(&store, a).unwrap()));
        sync(&store, "first", 2, "Renamed");
        let renamed = panel.report(&store, a).unwrap();
        assert!(!Arc::ptr_eq(&old, &renamed));
        assert_eq!(renamed.find("10").unwrap().1.name, "Renamed");
        let b = panel.report(&store, "b".parse().unwrap()).unwrap();
        assert_eq!(b.find("20").unwrap().1.name, "Other");
        sync(&store, "second", 1, "Rebooted");
        assert_eq!(panel.report(&store, a).unwrap().find("10").unwrap().1.name, "Rebooted");
        let replacement = Arc::new(crate::live::LiveStore::default());
        sync(&replacement, "second", 1, "New source");
        assert_eq!(panel.report(&replacement, a).unwrap().find("10").unwrap().1.name, "New source");
        assert!(panel.report(&replacement, "missing".parse().unwrap()).is_none());
    }

    #[test]
    fn live_filters_combine_kinds_and_narrow_states() {
        let player = LiveObject { id: "p".into(), pc: true, ..Default::default() };
        let npc = LiveObject {
            id: "n".into(),
            name: "Strażnik".into(),
            tag: "GateGuard".into(),
            resref: "nw_guard".into(),
            hp: Some([4, 10]),
            own: true,
            combat: true,
            hostile: vec!["p".into()],
            ..Default::default()
        };
        let mut f = LiveFilter::default();
        for text in ["STRAŻ", "gateguard", "nw_GUARD"] {
            f.search = text.into();
            assert!(f.matches(ObjectKind::Creature, &npc, Some(&player)));
            assert!(!f.matches(ObjectKind::Creature, &player, Some(&player)));
        }
        f.search.clear();
        f.enabled.extend([Filter::Players, Filter::Npcs]);
        assert!(f.matches(ObjectKind::Creature, &player, None));
        assert!(f.matches(ObjectKind::Creature, &npc, None));
        assert!(!f.matches(ObjectKind::Placeable, &npc, None));
        f.enabled.extend([Filter::Own, Filter::Combat, Filter::Wounded, Filter::Hostile]);
        assert!(f.matches(ObjectKind::Creature, &npc, Some(&player)));
        assert!(!f.matches(ObjectKind::Creature, &player, None));
        let stranger = LiveObject { id: "other-player".into(), ..player };
        assert!(!f.matches(ObjectKind::Creature, &npc, Some(&stranger)));
        let half = LiveObject { hp: Some([5, 10]), ..npc.clone() };
        assert!(!f.matches(ObjectKind::Creature, &half, None));
        let missing_hp = LiveObject { hp: None, ..npc.clone() };
        assert!(!f.matches(ObjectKind::Creature, &missing_hp, None));
        let zero_max = LiveObject { hp: Some([0, 0]), ..npc.clone() };
        assert!(!f.matches(ObjectKind::Creature, &zero_max, None));
        f.enabled = HashSet::from([Filter::Dead]);
        assert!(!f.matches(ObjectKind::Creature, &npc, None));
        assert!(f.matches(ObjectKind::Creature, &LiveObject { dead: true, ..npc }, None));
    }

    #[test]
    fn live_rows_sort_by_camera_name_hp_and_selected_distance() {
        use std::cmp::Ordering::{Greater, Less};
        let a = LiveObject {
            id: "2".into(),
            name: "Zed".into(),
            x: 3.,
            hp: Some([1, 10]),
            ..Default::default()
        };
        let b = LiveObject {
            id: "1".into(),
            name: "Ada".into(),
            x: 8.,
            hp: Some([9, 10]),
            ..Default::default()
        };
        let camera = glam::Vec3::ZERO;
        let selected = glam::Vec3::X * 10.;
        assert_eq!(compare(Sort::Camera, false, &a, &b, camera, selected), Less);
        assert_eq!(compare(Sort::Name, false, &a, &b, camera, selected), Greater);
        assert_eq!(compare(Sort::Hp, false, &a, &b, camera, selected), Less);
        assert_eq!(compare(Sort::Hp, true, &a, &b, camera, selected), Greater);
        assert_eq!(compare(Sort::Distance, false, &a, &b, camera, selected), Greater);
        let unknown = LiveObject { hp: None, ..b.clone() };
        assert_eq!(compare(Sort::Hp, true, &a, &unknown, camera, selected), Less);
        let tied = LiveObject { x: a.x, ..b };
        assert_eq!(compare(Sort::Camera, false, &a, &tied, camera, selected), Greater);
    }

    #[test]
    fn live_action_names_read_ids_from_the_loaded_spec() {
        let names = action_names(b"const int ACTION_ATTACKOBJECT = 99;\nconst int ACTION_INVALID = 65535;\nconst int ACTION_MODE_DETECT = 99;\nconst int OTHER = 1;");
        assert_eq!(names.get(&99).map(String::as_str), Some("Attack"));
        assert_eq!(names.get(&65535).map(String::as_str), Some("None"));
        assert!(!names.contains_key(&1));
    }
}
