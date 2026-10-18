//! Verified gateway resources: worker-owned I/O, UI-thread generation handover.
use crate::{Action, Moonglow, Tab};
use mg_live::resource_files::ResourceGeneration;
use mg_live_link::{
    LinkStatus, PendingCall,
    resources::{Cancel, Generations, Progress, ReadyGeneration},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

const LARGE: u64 = 16 * 1024 * 1024;
const TITLE: &str = "Download Live resources";
fn foreground(first: bool, missing_bytes: u64) -> bool {
    first || missing_bytes >= LARGE
}

#[derive(Default)]
pub(crate) struct Resources {
    server: Option<String>,
    boot: String,
    connections: u64,
    files: Option<PendingCall>,
    next_poll: Option<Instant>,
    desired: Option<ResourceGeneration>,
    attempted: Option<String>,
    retry_after: Option<Instant>,
    retries: u8,
    generations: Generations,
    worker: Option<mpsc::Receiver<Result<ReadyGeneration, String>>>,
    cancel: Cancel,
    progress: Arc<Mutex<Option<Progress>>>,
    plan: Arc<Mutex<Option<crate::resource_plan::Plan>>>,
    ready: Option<ReadyGeneration>,
    mounted: Option<PathBuf>,
    notice: String,
    used: Arc<Mutex<Option<u64>>>,
}

impl Drop for Resources {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Clone, Default)]
struct CacheView {
    locked: bool,
    used: Option<u64>,
    notice: String,
}
fn view_id() -> egui::Id {
    egui::Id::new("live-resource-cache")
}
fn action_id() -> egui::Id {
    egui::Id::new("live-resource-action")
}
fn budget(mib: u32) -> u64 {
    u64::from(if mib == 0 { 2048 } else { mib }) * 1024 * 1024
}
fn bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KiB", n as f64 / 1024.)
    } else {
        format!("{:.1} MiB", n as f64 / (1024. * 1024.))
    }
}
fn cache_root(ctx: &egui::Context) -> Option<PathBuf> {
    crate::options::resource_data_dir(ctx).map(|p| p.join("live-resources"))
}
fn server_root(root: &Path, id: &str) -> PathBuf {
    root.join(mg_core::sha1::hex(&mg_core::sha1::sha1(id.as_bytes())))
}

pub(crate) fn options(ui: &mut egui::Ui, limit: &mut u32) {
    let view = ui.ctx().data(|d| d.get_temp::<CacheView>(view_id())).unwrap_or_default();
    ui.vertical(|ui| {
        ui.label(view.used.map_or_else(
            || "Cache size checked on connection".into(),
            |n| format!("Used: {}", bytes(n)),
        ));
        ui.horizontal(|ui| {
            ui.label("Limit (MiB)");
            let mut shown = if *limit == 0 { 2048 } else { *limit };
            if ui.add(egui::DragValue::new(&mut shown).range(64..=65536)).changed() {
                *limit = shown;
            }
            if ui
                .add_enabled(!view.locked, egui::Button::new("Clear cache"))
                .on_disabled_hover_text(
                    "Disconnect Live and finish downloads before clearing the cache.",
                )
                .clicked()
            {
                ui.ctx().data_mut(|d| d.insert_temp(action_id(), "clear".to_string()));
            }
        });
        if view.locked {
            ui.weak("Disconnect Live to clear the cache.");
        }
        if !view.notice.is_empty() {
            ui.label(&view.notice);
        }
    });
}

pub(crate) fn status(app: &Moonglow, ui: &mut egui::Ui) {
    if app.job.as_ref().is_some_and(|job| job.modal && job.title == TITLE) {
        return;
    }
    let r = &app.live_resources;
    ui.horizontal(|ui| {
        if r.worker.is_some() {
            ui.spinner();
            if let Some(p) = &*r.progress.lock().unwrap_or_else(|e| e.into_inner()) {
                ui.label(format!("Live resources: {} received, {} files", bytes(p.bytes), p.files))
                    .on_hover_text(&p.current);
            } else {
                ui.label("Checking Live resources…");
            }
            if ui.button("Download details").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(action_id(), "details".to_string()));
            }
            if ui.button("Cancel download").clicked() {
                r.cancel.cancel();
            }
        } else if !r.notice.is_empty() {
            ui.label(&r.notice);
            if r.desired.is_some() && ui.button("Resume download").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(action_id(), "resume".to_string()));
            }
        }
    });
}

pub(crate) fn update(app: &mut Moonglow, ctx: &egui::Context) {
    let mut r = std::mem::take(&mut app.live_resources);
    update_inner(app, ctx, &mut r);
    ctx.data_mut(|d| {
        d.insert_temp(
            view_id(),
            CacheView {
                locked: app.live_backend().active() || r.worker.is_some() || app.job.is_some(),
                notice: r.notice.clone(),
                used: *r.used.lock().unwrap_or_else(|e| e.into_inner()),
            },
        )
    });
    app.live_resources = r;
}

fn invalidate(app: &mut Moonglow) {
    for view in app.area_views.values_mut() {
        view.release_target(app.viewport.as_ref());
    }
    for view in app.model_views.values_mut() {
        view.release_target(app.viewport.as_ref());
    }
    app.area_views.clear();
    app.game_data_changed();
    if let Some(viewport) = &mut app.viewport {
        viewport.renderer.clear_textures();
    }
}

fn mounted(app: &Moonglow, r: &Resources) -> bool {
    r.mounted.as_deref().is_some_and(|path| app.module_path().as_deref() == Some(path))
}

fn update_inner(app: &mut Moonglow, ctx: &egui::Context, r: &mut Resources) {
    if let Some(action) = ctx.data_mut(|d| d.remove_temp::<String>(action_id())) {
        if action == "details"
            && let Some(job) = &mut app.job
            && job.title == TITLE
        {
            job.modal = true;
        }
        if action == "resume" {
            r.attempted = None;
            r.retry_after = None;
            r.retries = 0;
            r.notice.clear();
        }
        if action == "clear" {
            if app.live_backend().active() || r.worker.is_some() || app.job.is_some() {
                r.notice = "Disconnect Live and finish downloads before clearing the cache.".into();
            } else if let Some(root) = cache_root(ctx) {
                let cached_workspace = app.module_path().is_some_and(|p| p.starts_with(&root));
                if cached_workspace && app.has_unsaved_work() {
                    r.notice = "Close the edited cache workspace before clearing the cache.".into();
                    return;
                }
                if cached_workspace {
                    app.close();
                    invalidate(app);
                    r.mounted = None;
                }
                r.generations.clear();
                r.ready = None;
                let previous = app.background_jobs;
                app.background_jobs = true;
                app.start_job_anywhere(
                    "Clear Live cache",
                    move |job| clear_cache(&root, &|| job.progress.cancelled()),
                    |app, result| {
                        app.live_resources.notice = match result {
                            Ok(size) => {
                                *app.live_resources
                                    .used
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner()) = Some(0);
                                format!("Cache cleared: {}", bytes(size))
                            }
                            Err(e) => e,
                        };
                    },
                );
                app.background_jobs = previous;
            }
        }
    }
    let server = app.live_backend().server_id().map(|id| {
        let resolved = app.settings.live_servers.resolve();
        format!("{id}|{:?}", resolved.as_ref().map(|r| (&r.server.url, &r.server.cert_pin)))
    });
    let boot = app.live.as_ref().map(|s| s.boot()).unwrap_or_default();
    if server != r.server || (!boot.is_empty() && boot != r.boot) {
        r.cancel.cancel();
        r.generations.clear();
        r.ready = None;
        r.desired = None;
        r.attempted = None;
        r.files = None;
        r.next_poll = None;
        r.connections = 0;
        if mounted(app, r) {
            app.close();
            invalidate(app);
            r.mounted = None;
        }
        r.server = server;
        r.boot = boot.clone();
        r.notice.clear();
    }
    if let Some(worker) = &r.worker {
        match worker.try_recv() {
            Ok(result) => {
                r.worker = None;
                if !r.cancel.is_cancelled() {
                    match result {
                        Ok(ready) => r.ready = Some(ready),
                        Err(e) => {
                            r.notice = format!("Live resources: {e}");
                            app.log.warn(&r.notice);
                            if transient(&e) && r.retries < 3 {
                                r.retry_after =
                                    Some(Instant::now() + Duration::from_secs(2 << r.retries));
                                r.retries += 1;
                                r.notice.push_str("; retrying automatically");
                            }
                        }
                    }
                } else {
                    r.notice = "Download canceled; verified files are kept for resume.".into();
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                r.worker = None;
                r.notice = "Resource worker stopped; resume to retry.".into();
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
    // A foreground job's final callback releases its game-data snapshot first.
    if r.ready.is_some()
        && app.job.is_none()
        && app.live_backend().connection_status()["connected"] == true
    {
        if app.has_unsaved_work() {
            r.notice = "Resources ready. Save or close your edited module before switching.".into();
        } else if app.game.as_ref().is_none_or(|g| Arc::strong_count(g) == 1) {
            let ready = r.ready.take().unwrap();
            if ready.generation().boot == boot && r.generations.activate(ready) {
                let module = r.generations.current().unwrap().module().clone();
                let mounted_path = module.location.as_ref().map(|l| l.path().to_path_buf());
                app.use_module(module);
                invalidate(app);
                if let Some(area) = app.ws.as_ref().and_then(|ws| {
                    ws.module.keys_of(mg_core::ResType::ARE).next().map(|k| k.resref)
                }) {
                    app.actions.push(Action::OpenTab(Tab::Area(area)));
                    crate::area_view::ensure_live(app, area);
                }
                r.mounted = mounted_path;
                r.notice.clear();
                app.log.info("Live resource generation ready");
            }
        }
    }
    let backend = app.live_backend();
    let Some(link) = backend.link() else { return };
    let snapshot = link.snapshot();
    if snapshot.status != LinkStatus::Connected {
        return;
    }
    if snapshot.connections != r.connections {
        r.connections = snapshot.connections;
        r.attempted = None;
        r.next_poll = None;
        if let Some(spec) = snapshot.welcome.and_then(|w| w.nwsync).and_then(|n| n.generation) {
            select(r, spec, &boot);
        }
    }
    if let Some(call) = &r.files {
        match call.try_recv() {
            Ok(Some(value)) => {
                r.files = None;
                if let Ok(spec) = serde_json::from_value(value) {
                    select(r, spec, &boot);
                }
            }
            Err(_) => r.files = None,
            Ok(None) => {}
        }
    }
    if r.files.is_none() && r.next_poll.is_none_or(|t| Instant::now() >= t) {
        r.files = link.request("files", serde_json::json!({})).ok();
        r.next_poll = Some(Instant::now() + Duration::from_secs(5));
    }
    if let Some(p) = &*r.plan.lock().unwrap_or_else(|e| e.into_inner())
        && foreground(false, p.missing_bytes)
        && r.worker.is_some()
        && let Some(job) = &mut app.job
        && job.title == TITLE
    {
        job.modal = true;
    }
    if r.retry_after.is_some_and(|t| Instant::now() >= t) {
        r.attempted = None;
        r.retry_after = None;
    }
    if r.worker.is_none()
        && r.ready.is_none()
        && app.job.is_none()
        && let Some(spec) = r.desired.clone()
        && r.attempted.as_deref() != Some(&spec.id())
        && r.generations.current().is_none_or(|g| g.generation().id() != spec.id())
    {
        start(app, ctx, r, spec);
    }
    ctx.request_repaint_after(Duration::from_secs(1));
}

fn transient(error: &str) -> bool {
    error.starts_with("resource transport")
        || error == "resource read failed"
        || error == "Cannot read resource manifest"
        || [
            "resource HTTP 408",
            "resource HTTP 429",
            "resource HTTP 502",
            "resource HTTP 503",
            "resource HTTP 504",
        ]
        .contains(&error)
}

fn select(r: &mut Resources, spec: ResourceGeneration, boot: &str) {
    if spec.validate().is_err() || spec.boot != boot {
        return;
    }
    if r.desired.as_ref().is_none_or(|old| old.id() != spec.id()) {
        r.cancel.cancel();
        r.ready = None;
        r.retry_after = None;
        r.retries = 0;
        r.generations.select(&spec);
        r.desired = Some(spec);
        r.attempted = None;
    }
}

fn start(app: &mut Moonglow, ctx: &egui::Context, r: &mut Resources, spec: ResourceGeneration) {
    let Some(resolved) = app.settings.live_servers.resolve() else { return };
    let Some(root) = cache_root(ctx).map(|root| server_root(&root, &resolved.server.id)) else {
        return;
    };
    let limit = budget(app.settings.live_cache_mib);
    let modal = foreground(r.generations.current().is_none(), 0);
    r.attempted = Some(spec.id());
    r.cancel = Cancel::default();
    *r.progress.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *r.plan.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let (tx, rx) = mpsc::channel();
    r.worker = Some(rx);
    r.notice.clear();
    let (cancel, progress, wake) = (r.cancel.clone(), r.progress.clone(), ctx.clone());
    let planned = r.plan.clone();
    let used_bytes = r.used.clone();
    let all_cache = root.parent().unwrap().to_path_buf();
    let previous = app.background_jobs;
    app.background_jobs = true;
    app.start_job_anywhere(
        TITLE,
        move |job| {
            let stop = cancel.clone();
            job.progress.on_cancel(move || stop.cancel());
            let result = (|| {
                let used = usage(&all_cache)?;
                *used_bytes.lock().unwrap_or_else(|e| e.into_inner()) = Some(used);
                let origin = resolved.server.url.clone();
                let (gateway, token) = crate::options::resource_access(&wake, resolved)?;
                let limits =
                    mg_module::nwsync_cache::Limits { total_bytes: limit, ..Default::default() };
                job.progress.say("Checking resource manifests and verified cache files");
                let plan = crate::resource_plan::plan(
                    &origin, &gateway, &token, &root, &spec, limits, &cancel,
                )?;
                if used.checked_add(plan.disk_growth).is_none_or(|n| n > limit) {
                    return Err(
                        "Cache limit reached; disconnect and clear the cache or raise its limit."
                            .into(),
                    );
                }
                *planned.lock().unwrap_or_else(|e| e.into_inner()) = Some(plan.clone());
                job.progress.step(0, plan.transfers as usize);
                wake.request_repaint();
                let client =
                    mg_live_link::resources::ResourceClient::new(gateway, token, root, limits);
                client.download(&spec, &cancel, &|mut p| {
                    p.current = plan.item(&p.current);
                    job.progress.step(p.files as usize, plan.transfers as usize);
                    job.progress.say(format!(
                        "Received: {} (network)\nTo download: {} unpacked\nFiles: {} / {}\nCurrent item: {}",
                        bytes(p.bytes),
                        bytes(plan.missing_bytes),
                        p.files,
                        plan.transfers,
                        p.current
                    ));
                    *progress.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
                    wake.request_repaint();
                })
            })();
            if let Ok(used) = usage(&all_cache) {
                *used_bytes.lock().unwrap_or_else(|e| e.into_inner()) = Some(used);
            }
            let _ = tx.send(result);
            wake.request_repaint();
        },
        |_, ()| {},
    );
    if let Some(job) = &mut app.job {
        job.modal = modal;
    }
    app.background_jobs = previous;
}

fn usage(path: &Path) -> Result<u64, String> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err("Cannot inspect Live cache".into()),
    };
    if meta.file_type().is_symlink() {
        return Err("Live cache must not contain links".into());
    }
    if meta.is_file() {
        return Ok(meta.len());
    }
    std::fs::read_dir(path).map_err(|_| "Cannot inspect Live cache")?.try_fold(
        0u64,
        |size, entry| {
            size.checked_add(usage(&entry.map_err(|_| "Cannot inspect Live cache")?.path())?)
                .ok_or_else(|| "Cache size overflow".into())
        },
    )
}
fn clear_cache(root: &Path, cancelled: &dyn Fn() -> bool) -> Result<u64, String> {
    let size = usage(root)?; // Validate the complete caller-owned tree before any removal.
    if !root.exists() {
        return Ok(0);
    }
    #[cfg_attr(
        windows,
        expect(
            clippy::permissions_set_readonly_false,
            reason = "Windows-only removal of validated private cache files"
        )
    )]
    fn remove(path: &Path, cancelled: &dyn Fn() -> bool) -> std::io::Result<()> {
        if cancelled() {
            return Err(std::io::Error::other("cache cleanup canceled"));
        }
        let meta = std::fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::other("cache link"));
        }
        if meta.is_dir() {
            for entry in std::fs::read_dir(path)? {
                remove(&entry?.path(), cancelled)?;
            }
            std::fs::remove_dir(path)
        } else {
            #[cfg(windows)]
            {
                let mut p = meta.permissions();
                p.set_readonly(false);
                std::fs::set_permissions(path, p)?;
            }
            std::fs::remove_file(path)
        }
    }
    remove(root, cancelled).map_err(|_| "Cannot clear Live cache".to_string())?;
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_and_large_downloads_are_modal_small_updates_are_not() {
        assert!(foreground(true, 0));
        assert!(!foreground(false, LARGE - 1));
        assert!(foreground(false, LARGE));
    }
    #[test]
    fn cache_setting_round_trips_and_old_settings_use_default_budget() {
        let old: crate::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(budget(old.live_cache_mib), 2048 * 1024 * 1024);
        let mut draft = crate::OptionsDraft::from_settings(&old);
        draft.live_cache_mib = 512;
        let saved = draft.apply(&old);
        let restored: crate::Settings =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(budget(restored.live_cache_mib), 512 * 1024 * 1024);
    }
    #[test]
    fn canceled_clear_preserves_cache_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("blob"), b"retained").unwrap();
        assert!(clear_cache(temp.path(), &|| true).is_err());
        assert!(temp.path().join("blob").exists());
    }
    #[test]
    fn clear_only_removes_the_owned_cache_tree() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("live-resources");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("blob"), b"1234").unwrap();
        std::fs::write(temp.path().join("keep"), b"source").unwrap();
        assert_eq!(clear_cache(&root, &|| false).unwrap(), 4);
        assert_eq!(std::fs::read(temp.path().join("keep")).unwrap(), b"source");
        assert!(!root.exists());
    }
}
