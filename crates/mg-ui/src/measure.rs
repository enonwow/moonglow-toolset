//! Optional measurement spans, never enabled in a normal build.
//! Inclusive elapsed wall time on the UI thread, not hardware GPU time. Nested spans overlap.
use std::{collections::BTreeMap, sync::Mutex, time::Instant};

static SPANS: Mutex<BTreeMap<&'static str, (u64, f64)>> = Mutex::new(BTreeMap::new());

#[derive(Debug)]
pub struct Span {
    name: &'static str,
    start: Instant,
}

pub fn span(name: &'static str) -> Span {
    Span { name, start: Instant::now() }
}

impl Drop for Span {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_secs_f64() * 1000.;
        let mut spans = SPANS.lock().unwrap();
        let row = spans.entry(self.name).or_default();
        row.0 += 1;
        row.1 += elapsed;
    }
}

/// Record an internal renderer observation without adding a dependency on the UI.
pub fn value(name: &'static str, ms: f64) {
    let mut spans = SPANS.lock().unwrap();
    let row = spans.entry(name).or_default();
    row.0 += 1;
    row.1 += ms;
}

pub fn census(app: &crate::Moonglow, area: &mg_core::ResRef) -> serde_json::Value {
    let Some(view) = app.area_views.get(area) else {
        return serde_json::Value::Null;
    };
    let events = app
        .live
        .as_ref()
        .map(|s| s.events(&mg_live::events::Query::default()).0)
        .unwrap_or_default();
    serde_json::json!({
        "event_query_count":events.len(),
        "event_query_serialized_bytes":serde_json::to_vec(&events).map_or(0,|v|v.len()),
        "looks":view.live.measure_memory(),
        "scene":view.measure_scene(),
        "renderer":app.viewport.as_ref().map(|v|v.renderer.measure_memory()),
        "particles":view.particles_shown, "live_particles":view.live_particles_shown,
    })
}

/// Drain once per GUI pass so measurements never grow with the session.
pub fn take() -> BTreeMap<&'static str, (u64, f64)> {
    std::mem::take(&mut *SPANS.lock().unwrap())
}

/// Minute-scale census for the soak helper; never called by a normal build.
/// Counts logical cache entries, not exclusive allocator ownership.
pub fn census_all(app: &crate::Moonglow) -> serde_json::Value {
    serde_json::json!({
        "areas":app.area_views.iter().map(|(area,v)|serde_json::json!({
            "area":area.to_string(),"on":v.live.on,"pictures":v.pictures,
            "report_reads":v.live.report_reads,"reconciliations":v.live.reconciliations,
            "looks":v.live.measure_memory(),"scene":v.measure_scene(),
            "selected":v.live.selection_ids().count(),"particles":v.particles_shown,
            "live_particles":v.live_particles_shown,"cues":v.live.transient_effects,
            "bubbles":v.live.speech_bubbles,
        })).collect::<Vec<_>>(),
        "pictures":app.pictures.len(),
        "picture_pixels":app.pictures.values().flatten().map(|p|p.texture.size().iter().product::<usize>()).sum::<usize>(),
        "palettes":app.palettes.len(),
        "focus_entries":app.live_focus.entries.len(),
        "focus_rows":app.live_focus.entries.iter().map(|e|e.rows.len()).collect::<Vec<_>>(),
        "chat":app.live_chat.measure_cache(),"log":app.live_log.measure_cache(),
        "drafts":app.live_drafts.measure_cache(),
        "renderer":app.viewport.as_ref().map(|v|v.renderer.measure_memory()),
    })
}

/// Cheap counters taken only when a view actually submitted a scene picture.
pub fn visual_counts(view: &crate::area_view::AreaView) -> [usize; 4] {
    [
        view.live.measure_effect_models(),
        view.live_particles_shown,
        view.live.transient_effects,
        view.live.speech_bubbles,
    ]
}
