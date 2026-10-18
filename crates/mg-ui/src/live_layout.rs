//! Live tools share the right palette pane, leaving the area visible.
use crate::{Action, Moonglow, Tab};

pub(crate) fn open(app: &mut Moonglow, tab: &Tab) -> bool {
    if !tab.kind().starts_with("live-") || matches!(tab, Tab::LiveChatArea(_)) {
        return false;
    }
    if app.dock.find_tab(&Tab::Palette).is_none() {
        app.run_now(Action::OpenTab(Tab::Palette));
    }
    if let (Some(old), Some(right)) = (app.dock.find_tab(tab), app.dock.find_tab(&Tab::Palette)) {
        if old.surface == right.surface && old.node == right.node {
            let _ = app.dock.set_active_tab(old);
            return true;
        }
        app.dock.remove_tab(old);
    }
    let Some(right) = app.dock.find_tab(&Tab::Palette) else { return false };
    let pane = egui_dock::NodePath { surface: right.surface, node: right.node };
    app.dock.set_focused_node_and_surface(pane);
    app.dock.push_to_focused_leaf(tab.clone());
    let fraction = app.live_backend().panel_fraction();
    if right.surface.is_main()
        && let Some(parent) = right.node.parent()
        && let egui_dock::Node::Horizontal(split) = &mut app.dock.main_surface_mut()[parent]
        && right.node == parent.right()
    {
        split.fraction = fraction;
    }
    app.tab_places.remove(tab);
    true
}

pub(crate) fn activate(app: &mut Moonglow) {
    if !app.settings.no_live_focus {
        app.actions.push(Action::OpenTab(Tab::LiveFocus));
    }
    for tab in [
        Tab::LiveLegend,
        Tab::LivePanel,
        Tab::LiveSelection,
        Tab::LiveSkill,
        Tab::LiveVfx,
        Tab::LiveLibrary,
        Tab::Scene,
        Tab::LiveInventory,
        Tab::LiveChat,
    ] {
        app.actions.push(Action::OpenTab(tab));
    }
}

/// Open views own subscriptions; cached closed area views and chat history do not.
/// This union also keeps an area's watch while a second window still needs it.
pub(crate) fn watch_areas(app: &mut Moonglow) {
    if app.live.is_none() {
        return;
    }
    let mut areas = std::collections::BTreeSet::new();
    for (_, tab) in app.dock.iter_all_tabs() {
        match tab {
            Tab::Area(area) if app.area_views.get(area).is_some_and(|v| v.live.on) => {
                areas.insert(area.to_string());
            }
            Tab::LiveChatArea(area) => {
                areas.insert(area.to_string());
            }
            Tab::LiveChat => {
                if let Some(area) =
                    app.live_chat.area.clone().or_else(|| app.palette.area.map(|a| a.to_string()))
                {
                    areas.insert(area);
                }
            }
            Tab::LiveFocus if !app.settings.no_live_focus => {
                for member in app.live_focus.entries.iter().flat_map(|e| &e.members) {
                    // Saved member locations bootstrap Focus again after a server boot.
                    if !member.area.is_empty() {
                        areas.insert(member.area.clone());
                    }
                }
                if let Some(area) = &app.live_focus.area {
                    areas.insert(area.clone());
                }
            }
            _ => {}
        }
    }
    if let Some(follow) = &app.live_panel.follow {
        // update_follow already resolved this frame; do not clone its look again.
        areas.insert(follow.area.to_string());
    }
    let areas: Vec<_> = areas
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if let Err(error) = crate::live_backend::watch(app, &areas) {
        app.live_panel.notice = Some(format!("Live area watches: {error}"));
    }
}
