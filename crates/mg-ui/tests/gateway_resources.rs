#[path = "support/resource_host.rs"]
mod support;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use mg_ui::{Action, Tab};
use std::sync::atomic::Ordering;
use support::{Host, pump};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resources_cancel_resume_switch_and_refuse_clear_while_connected() {
    let data = support::Scratch::new();
    let host = Host::new(data.path()).await;
    host.delay.store(300, Ordering::Relaxed);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200., 900.))
        .build_ui_state(support::draw(data.path().join("client")), host.app());
    pump(&mut h, |app| app.busy()).await;
    h.get_by_label("Download Live resources");
    pump(&mut h, |_| support::blob_count(&data.path().join("client/live-resources")) > 0).await;
    h.get_by_label_contains("Received:");
    h.get_by_label_contains("Files:");
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    pump(&mut h, |app| !app.busy()).await;
    assert!(h.state().ws.is_none(), "cancellation cannot activate an incomplete generation");
    let cached = support::blob_count(&data.path().join("client/live-resources"));
    assert!(cached > 0);
    let requests = host.seen.lock().unwrap().len();
    host.delay.store(15, Ordering::Relaxed);
    h.get_by_label("Resume download").click();
    h.run_steps(2);
    pump(&mut h, |app| app.ws.is_some()).await;
    assert!(
        h.state()
            .ws
            .as_ref()
            .unwrap()
            .module
            .contains(&mg_resman::ResKey::parse("square", mg_core::ResType::ARE).unwrap())
    );
    assert!(
        h.state().game.as_ref().unwrap().resman.layers().iter().any(|l| l.label == "Live NWSync")
    );
    assert_eq!(
        h.state()
            .game
            .as_ref()
            .unwrap()
            .resman
            .get_named("file0", mg_core::ResType::TXT)
            .unwrap()
            .as_ref(),
        vec![0u8; 4096]
    );
    let fetched = host.seen.lock().unwrap().len() - requests;
    assert!(fetched < 14, "resume reuses verified files");
    assert!(
        h.state_mut().ws.as_mut().unwrap().module.save().is_err(),
        "cache module stays read-only"
    );
    host.change_area("harbor");
    let mut blocked = false;
    pump(&mut h, |app| {
        blocked |= app.busy();
        app.ws.as_ref().is_some_and(|ws| {
            ws.module.contains(&mg_resman::ResKey::parse("harbor", mg_core::ResType::ARE).unwrap())
        })
    })
    .await;
    assert!(
        !h.state()
            .ws
            .as_ref()
            .unwrap()
            .module
            .contains(&mg_resman::ResKey::parse("square", mg_core::ResType::ARE).unwrap())
    );
    assert!(!blocked, "a small live update must keep the UI available");
    assert!(h.state().dock.find_tab(&Tab::Area("harbor".parse().unwrap())).is_some());
    h.state_mut().actions.push(Action::LiveOptions);
    h.run_steps(3);
    assert!(h.get_by_label("Clear cache").accesskit_node().is_disabled());
    h.get_by_label("Disconnect Live to clear the cache.");
    let local = data.path().join("local.mod");
    support::module(&local, "local");
    h.state_mut().open_module(&local);
    h.state_mut().settings.live.enabled = false;
    h.state_mut().apply_live(&egui::Context::default());
    h.state_mut().actions.push(Action::LiveOptions);
    h.run_steps(3);
    assert_eq!(
        h.state().module_path().as_deref(),
        Some(local.as_path()),
        "disconnect must preserve a separately opened local module"
    );
    h.get_by_label("Clear cache").click();
    h.run_steps(3);
    pump(&mut h, |app| !app.busy()).await;
    assert!(!data.path().join("client/live-resources").exists());
    assert!(
        h.state().game.as_ref().unwrap().resman.get_named("file0", mg_core::ResType::TXT).is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tampered_resource_is_reported_and_never_activated() {
    let data = support::Scratch::new();
    let host = Host::new(data.path()).await;
    host.corrupt.store(true, Ordering::Relaxed);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200., 900.))
        .build_ui_state(support::draw(data.path().join("client")), host.app());
    pump(&mut h, |app| app.log.entries.iter().any(|(_, text)| text.starts_with("Live resources:")))
        .await;
    assert!(h.state().ws.is_none());
    h.get_by_label("Resume download");
    assert!(h.state().log.entries.iter().all(|(_, text)| !text.contains("fixture-dm")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cache_budget_refuses_growth_without_removing_existing_files() {
    let data = support::Scratch::new();
    let host = Host::new(data.path()).await;
    let root = data.path().join("client/live-resources");
    std::fs::create_dir_all(&root).unwrap();
    let old = root.join("retained");
    std::fs::write(&old, vec![0; 1024 * 1024]).unwrap();
    let mut app = host.app();
    app.settings.live_cache_mib = 1;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200., 900.))
        .build_ui_state(support::draw(data.path().join("client")), app);
    pump(&mut h, |app| {
        app.log.entries.iter().any(|(_, text)| text.contains("Cache limit reached"))
    })
    .await;
    assert!(h.state().ws.is_none());
    assert_eq!(std::fs::metadata(old).unwrap().len(), 1024 * 1024);
    assert!(host.seen.lock().unwrap().iter().all(|path| !path.contains("/data/sha1/")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_download_retries_automatically_from_verified_cache() {
    let data = support::Scratch::new();
    let host = Host::new(data.path()).await;
    host.interrupt.store(true, Ordering::Relaxed);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200., 900.))
        .build_ui_state(support::draw(data.path().join("client")), host.app());
    pump(&mut h, |app| app.ws.is_some()).await;
    assert!(h.state().log.entries.iter().any(|(_, text)| text.contains("resource HTTP 503")));
    assert!(!host.interrupt.load(Ordering::Relaxed));
    assert!(support::blob_count(&data.path().join("client/live-resources")) > 0);
}
