//! Shared selection snapshots and eligibility for Live command windows.
use crate::{
    Moonglow,
    live::{Command, LiveObject, Op},
};
use mg_area::ObjectKind;
use mg_core::ResRef;

#[derive(Debug, Clone)]
pub(crate) struct Selection {
    pub area: ResRef,
    pub boot: String,
    pub objects: Vec<(ObjectKind, LiveObject)>,
}

/// The running service has already applied environment overrides (demo launcher).
/// Saved options can remain disabled for an enabled runtime service.
pub(crate) fn enabled(app: &Moonglow) -> bool {
    app.live_service.as_ref().map_or(app.settings.live.enabled, |s| s.config().enabled)
}

pub(crate) fn selection(app: &Moonglow) -> Result<Selection, String> {
    let area = app
        .palette
        .area
        .filter(|a| app.panes_tabs.iter().any(|tabs| tabs.contains(&crate::Tab::Area(*a))))
        .ok_or("Open a Live area first.")?;
    let view =
        app.area_views.get(&area).filter(|v| v.live.on).ok_or("Enable Live in the area view.")?;
    let store = app.live.as_ref().ok_or("Live is unavailable.")?;
    let report = store.area(&area.to_string()).ok_or("Waiting for this area's report.")?;
    if !enabled(app) || mg_live::now_ms().saturating_sub(report.received_ms) > 10_000 {
        return Err("Live report is offline or stale; wait for a fresh report.".into());
    }
    let mut objects: Vec<_> = view
        .live
        .selection_ids()
        .filter_map(|id| report.find(id).map(|(k, o)| (k, o.clone())))
        .collect();
    objects.sort_by_key(|(_, o)| Some(&o.id) != view.live.selected.as_ref());
    Ok(Selection { area, boot: store.boot(), objects })
}

pub(crate) fn send(app: &mut Moonglow, area: ResRef, op: Op) -> Result<String, String> {
    crate::live_scene::enqueue(app, Command { id: String::new(), area: area.to_string(), op })
        .map_err(|e| e.to_string())
}

/// Local player consent never makes DM or possessed objects eligible.
pub(crate) fn allowed(object: &LiveObject, players: bool) -> bool {
    !object.dm && !object.possessed && (!object.pc || players)
}

pub(crate) fn creature(kind: ObjectKind, object: &LiveObject, players: bool) -> bool {
    kind == ObjectKind::Creature && !object.dead && allowed(object, players)
}
