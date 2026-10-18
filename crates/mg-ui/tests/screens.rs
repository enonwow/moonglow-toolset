//! Screenshots of windows for looking at their layout, rendered with wgpu
//! into `target/test-output/screens/` (kept from run to run: each test adds
//! its pictures). Not checks; run by hand:
//! `cargo test -p mg-ui --test screens -- --ignored`.

#[path = "support/live_timing.rs"]
mod live_timing;

#[path = "support/gateway_ops.rs"]
mod gateway_ops;
#[path = "support/gateway_ui.rs"]
mod gateway_ui;

#[path = "support/resource_host.rs"]
mod resource_host;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn live_gateway_ai_identity() {
    mg_testkit::gpu::hold();
    let data = tempfile::tempdir().unwrap();
    let host = gateway_ops::host(data.path()).await;
    let writer = gateway_ops::link(&host, gateway_ui::host::SECRET).await;
    let created = gateway_ui::host::reply_async(writer.ai_create(serde_json::json!({"name":"Scribe","avatar":gateway_ops::AVATAR,"groups":["read","focus"]})).unwrap()).await.unwrap();
    host.core.with_gateway(|g| g.set_llm_enabled(true)).await.unwrap();
    gateway_ui::host::reply_async(
        writer.ai_allow_llm(Some(created["ai"]["id"].as_str().unwrap()), true).unwrap(),
    )
    .await
    .unwrap();
    let ai = gateway_ops::link(&host, created["token"].as_str().unwrap()).await;
    let mut app = gateway_ui::app(&host, data.path());
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    app.set_render_state(rs.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280., 900.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(gateway_ui::draw(), app);
    h.ctx.include_bytes(gateway_ops::AVATAR, gateway_ops::avatar_bytes());
    gateway_ui::connected(&mut h).await;
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("AI agents").click();
    h.run_steps(3);
    gateway_ui::label(&mut h, "AI name").await;
    let stage = std::env::var("MOONGLOW_AI_SCREEN_STAGE").unwrap_or_else(|_| "after".into());
    assert!(["before", "after"].contains(&stage.as_str()));
    if stage == "after" {
        gateway_ui::label(&mut h, "Avatar preview").await;
    }
    shoot(&mut h, &mg_testkit::output_dir("screens"), &format!("live-m3-ai-{stage}"));
    h.get_by_label("Close window").click();
    h.run_steps(3);
    gateway_ui::focus(&mut h);
    gateway_ui::host::reply_async(ai.set_focus(&[("square".into(), "a1".into())]).unwrap())
        .await
        .unwrap();
    gateway_ui::label(&mut h, "Scribe \u{b7} AI").await;
    shoot(&mut h, &mg_testkit::output_dir("screens"), &format!("live-m3-focus-{stage}"));
    if stage == "after" {
        h.get_by_label("Show on map").click();
        h.run_steps(4);
        h.get_by_label("Scribe \u{b7} AI / Test \u{b7} DM");
        shoot(&mut h, &mg_testkit::output_dir("screens"), "live-m3-map-after");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn live_gateway_resource_downloads() {
    mg_testkit::gpu::hold();
    let data = resource_host::Scratch::new();
    let host = resource_host::Host::new(data.path()).await;
    host.delay.store(500, std::sync::atomic::Ordering::Relaxed);
    let mut app = host.app();
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    app.set_render_state(rs.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280., 950.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(resource_host::draw(data.path().join("client")), app);
    resource_host::pump(&mut h, |_| {
        resource_host::blob_count(&data.path().join("client/live-resources")) > 0
    })
    .await;
    let dir = mg_testkit::output_dir("screens");
    shoot(&mut h, &dir, "live-p7-download");
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    resource_host::pump(&mut h, |app| !app.busy()).await;
    shoot(&mut h, &dir, "live-p7-canceled");
    host.delay.store(20, std::sync::atomic::Ordering::Relaxed);
    h.get_by_label("Resume download").click();
    h.run_steps(2);
    resource_host::pump(&mut h, |app| {
        app.ws.is_some() && app.live.as_ref().is_some_and(|store| store.area("square").is_some())
    })
    .await;
    shoot(&mut h, &dir, "live-p7-square");
    host.change_area("harbor");
    resource_host::pump(&mut h, |app| {
        app.ws.as_ref().is_some_and(|ws| {
            ws.module.contains(&mg_resman::ResKey::parse("harbor", mg_core::ResType::ARE).unwrap())
        }) && app.live.as_ref().is_some_and(|store| store.area("harbor").is_some())
    })
    .await;
    h.run_steps(3);
    shoot(&mut h, &dir, "live-p7-harbor");
    h.state_mut().actions.push(mg_ui::Action::LiveOptions);
    h.run_steps(3);
    gateway_ui::label(&mut h, "Session saved on this device").await;
    shoot(&mut h, &dir, "live-p7-options-after");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn live_gateway_shared_books_and_ai() {
    mg_testkit::gpu::hold();
    let data = tempfile::tempdir().unwrap();
    let host = gateway_ops::host(data.path()).await;
    let mut app = gateway_ui::app(&host, data.path());
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    app.set_render_state(rs.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280., 1000.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(gateway_ui::draw(), app);
    gateway_ui::connected(&mut h).await;
    h.state_mut().area_views.get_mut(&"square".parse().unwrap()).unwrap().live.on = true;
    h.state_mut()
        .actions
        .push(mg_ui::Action::CloseTab(mg_ui::Tab::LiveChatArea("square".parse().unwrap())));
    h.run_steps(3);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveGmCard));
    h.run_steps(3);
    gateway_ui::label(&mut h, "Campaign tone").await;
    gateway_ops::input(&mut h, "Campaign tone", "My campaign: hope and discovery.");
    let writer = gateway_ops::link(&host, "fixture-other").await;
    let current = gateway_ui::host::reply_async(writer.gm_card().unwrap()).await.unwrap();
    let mut newer = current["card"].clone();
    newer["campaign_tone"] = serde_json::json!("A fellow DM saved: mystery and intrigue.");
    gateway_ui::host::reply_async(
        writer
            .book_put(mg_live_link::BookKind::Gm, "", newer, current["rev"].as_u64().unwrap())
            .unwrap(),
    )
    .await
    .unwrap();
    gateway_ui::label(&mut h, "Take newer").await;
    let dir = mg_testkit::output_dir("screens");
    shoot(&mut h, &dir, "live-p3-book-conflict");
    h.get_by_label("Take newer").click();
    h.run_steps(3);
    assert!(
        h.query_by_label("Take newer").is_none(),
        "the conflict must be resolved before capture"
    );
    shoot(&mut h, &dir, "live-p3-book-resolved");
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("AI agents").click();
    h.run_steps(3);
    gateway_ui::label(&mut h, "New AI name").await;
    gateway_ops::input(&mut h, "New AI name", "Scribe");
    h.get_by_label("Create AI").click();
    h.run_steps(2);
    gateway_ui::label(&mut h, "I saved the token").await;
    h.get_by_label("I saved the token").click();
    h.run_steps(3);
    gateway_ui::label(&mut h, "AI name").await;
    shoot(&mut h, &dir, "live-p3-ai-agents");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn live_gateway_connected() {
    mg_testkit::gpu::hold();
    let host = gateway_ui::host::Host::new().await;
    gateway_ui::report(&host, 1, serde_json::json!([])).await;
    let data = tempfile::tempdir().unwrap();
    let mut app = gateway_ui::app(&host, data.path());
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    app.set_render_state(rs.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280., 900.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(gateway_ui::draw(), app);
    gateway_ui::connected(&mut h).await;
    gateway_ui::focus(&mut h);
    gateway_ui::pump(&mut h, |_| {
        host.frames().iter().any(|f| {
            f["op"] == "focus.set" && f["a"]["objects"].as_array().is_some_and(|v| !v.is_empty())
        })
    })
    .await;
    gateway_ui::presence(&host, &mut h).await;
    h.get_by_label("Guide · DM");
    let dir = mg_testkit::output_dir("screens");
    shoot(&mut h, &dir, "live-gateway-focus");
    h.get_by_label("Live").click();
    h.run_steps(2);
    shoot(&mut h, &dir, "live-gateway-connected");
    h.key_press(egui::Key::Escape);
    host.drop_sockets();
    gateway_ui::pump(&mut h, |app| {
        app.live_backend().connection_status()["state"] == "reconnecting"
    })
    .await;
    h.get_by_label("Live").click();
    h.run_steps(2);
    shoot(&mut h, &dir, "live-gateway-reconnecting");
    h.key_press(egui::Key::Escape);
    gateway_ui::connected(&mut h).await;
    h.state_mut().live_focus.text = "Test reply".into();
    h.run_steps(2);
    h.get_by_label("Send reply").click();
    gateway_ui::pump(&mut h, |_| {
        host.store.commands(mg_live::now_ms())["commands"].as_array().is_some_and(|c| !c.is_empty())
    })
    .await;
    let id = host.store.commands(mg_live::now_ms())["commands"][0]["id"].clone();
    gateway_ui::report(
        &host,
        2,
        serde_json::json!([{"id":id,"ok":false,"code":"unknown","err":"Outcome unknown"}]),
    )
    .await;
    gateway_ui::pump(&mut h, |app| {
        app.live.as_ref().unwrap().result(id.as_str().unwrap()).is_some()
    })
    .await;
    h.get_by_label("Live").click();
    h.run_steps(2);
    shoot(&mut h, &dir, "live-gateway-unknown");
    h.key_press(egui::Key::Escape);
    gateway_ui::presence(&host, &mut h).await;
    h.get_by_label("Show on map").click();
    h.run_steps(4);
    h.get_by_label("Guide · DM / Scribe · AI");
    shoot(&mut h, &dir, "live-gateway-markers");
}

use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use mg_core::ResType;
use mg_edit::GffPath;
use mg_module::script_wizard::{ClassLevel, Lists, Perform};
use mg_resman::{GameInstall, ResKey};
use mg_ui::script_wizard::{ScriptWizard, Step};
use mg_ui::{Moonglow, NoDialogs};

fn shoot(h: &mut Harness<'_, Moonglow>, dir: &std::path::Path, name: &str) {
    // Steps rather than runs: animated views keep repainting.
    h.run_steps(3);
    let image = h.render().expect("render");
    image.save(dir.join(format!("{name}.png"))).unwrap();
}

#[test]
#[ignore]
fn live_gateway_options() {
    mg_testkit::gpu::hold();
    struct NoSession;
    impl mg_live_link::credentials::Backend for NoSession {
        fn get(
            &self,
            _: &str,
        ) -> Result<Option<mg_live_link::SessionToken>, mg_live_link::credentials::StoreError>
        {
            Ok(None)
        }
        fn set(
            &self,
            _: &str,
            _: &mg_live_link::SessionToken,
        ) -> Result<(), mg_live_link::credentials::StoreError> {
            Err(mg_live_link::credentials::StoreError::Denied)
        }
        fn delete(&self, _: &str) -> Result<(), mg_live_link::credentials::StoreError> {
            Ok(())
        }
    }
    let credentials = tempfile::tempdir().unwrap();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.settings.live_servers.add();
    app.settings.live_servers.servers[1].name = "Campaign server".into();
    app.actions.push(mg_ui::Action::LiveOptions);
    let mut h = Harness::builder().with_size(egui::vec2(1120., 760.)).wgpu().build_ui_state(
        |ui, app: &mut Moonglow| {
            mg_ui::OptionsDraft::set_live_credential_backend(
                ui.ctx(),
                Arc::new(NoSession),
                credentials.path().into(),
            );
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    let path = h.state().dock.find_tab(&mg_ui::Tab::Options).unwrap();
    h.state_mut()
        .dock
        .get_window_state_mut(path.surface)
        .unwrap()
        .set_position(egui::pos2(40., 40.))
        .set_size(egui::vec2(1000., 650.));
    std::thread::sleep(std::time::Duration::from_millis(30));
    shoot(&mut h, &mg_testkit::output_dir("screens"), "live-gateway-options");
    let draft = h.state_mut().options.as_mut().unwrap();
    draft
        .live
        .clients
        .push(mg_live::access::ClientToken { name: "Reader".into(), ..Default::default() });
    draft.live_credential_error = Some("OS credential store refused access".into());
    shoot(&mut h, &mg_testkit::output_dir("screens"), "live-gateway-credential-error");
}

#[test]
#[ignore]
fn live_prepare_snapshot() {
    mg_testkit::gpu::hold();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.settings.live_nwnx = "C:/NWNX".into();
    app.live_prepare.select("C:/Campaign/Harbor Town.mod".into());
    app.live_prepare.capture_dialogue = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(760., 680.))
        .build_ui_state(|ui, app| mg_ui::live_prepare::ui(app, ui), app);
    h.run_steps(3);
    h.state_mut().live_prepare.report = Some(mg_live::build::Report {
        ok: true,
        output: "C:/Campaign/Harbor Town-live.mod".into(),
        compiled: 48,
        module_ifo: "Mod_OnModLoad: city_load → mglive_start (original first)".into(),
        added: vec!["mglive_start.nss".into(), "mglive_tick.ncs".into()],
        warnings: vec!["Clients still need the module's original HAKs.".into()],
        ..Default::default()
    });
    let dir = mg_testkit::output_dir("screens");
    shoot(&mut h, &dir, "live-prepare");
}

/// Same authored working module for Phase 1 before/after; no game client is launched.
#[test]
#[ignore]
fn live_next_phase1() {
    live_next_pictures(false);
}

#[test]
#[ignore]
fn live_next_phase1b() {
    live_next_pictures(true);
}

fn live_next_pictures(acceptance_fixes: bool) {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let base =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output");
    let pictures = base.join("live-next/visual");
    std::fs::create_dir_all(&pictures).unwrap();
    let stage = std::env::var("MGLIVE_PICTURE_STAGE").unwrap_or_else(|_| "after".into());
    assert!(matches!(stage.as_str(), "before" | "after"));
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&base.join("live-mode/viewer/fixture.mod"));
    app.open_palette = false;
    let area =
        app.ws.as_ref().unwrap().module.keys().find(|k| k.restype == ResType::ARE).unwrap().resref;
    let look = mg_gff::Gff {
        file_type: *b"UTC ",
        version: mg_gff::Gff::VERSION,
        root: mg_preview::CreatureLook::new(6).to_utc(),
    };
    let look = mg_gff::to_json(&look, mg_core::Codepage::WINDOWS_1252).unwrap();
    let store = Arc::new(mg_live::store::LiveStore::default());
    store.sync(serde_json::json!({"v":1,"boot":"next-picture","seq":1,"areas":[{
        "area":area.to_string(),"full":true,"cre":[
            {"id":"1","r":"nw_bandit001","name":"Guard","x":17,"y":25,"f":90,"hp":[18,30]},
            {"id":"2","r":"nw_bartender","name":"Innkeeper","x":if acceptance_fixes {23} else {25},"y":if acceptance_fixes {20} else {23},"hp":[12,12],"dead":acceptance_fixes},
            {"id":"3","name":"Player","pc":true,"x":21,"y":21,"hp":[24,30],"look":look,"lookHash":1,"dead":acceptance_fixes}]
    }]}).to_string().as_bytes(), mg_live::now_ms()).unwrap();
    app.live = Some(store);
    app.settings.live = mg_live::LiveConfig {
        enabled: true,
        api_port: 0,
        redis_port: 1,
        keep_wsl: Some(false),
        ..Default::default()
    };
    app.apply_live(&egui::Context::default());
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500., 950.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    let v = h.state_mut().area_views.get_mut(&area).unwrap();
    v.live.on = true;
    v.live.selected = Some("1".into());
    let orbit = v.orbit.as_mut().unwrap();
    orbit.target = glam::vec3(21., 22., 0.);
    orbit.distance = 24.;
    if acceptance_fixes {
        v.show_start = false;
        v.grid = false;
        let orbit = v.orbit.as_mut().unwrap();
        orbit.target = glam::vec3(22., 20.5, 0.6);
        orbit.distance = 9.;
        orbit.pitch = 0.55;
        shoot(&mut h, &pictures, &format!("dead-{stage}"));
        h.state_mut().palette.selected = ResKey::parse("nw_bandit001", ResType::UTC);
        let at = h.state().area_views[&area].screen_pos(glam::vec3(23., 24., 0.)).unwrap();
        h.hover_at(at);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        shoot(&mut h, &pictures, &format!("spawn-{stage}"));
        return;
    }
    shoot(&mut h, &pictures, &format!("player-{stage}"));
    h.state_mut().area_views.get_mut(&area).unwrap().live.aim =
        Some(mg_ui::live_view::Aim::Move("1".into()));
    let at = h.state().area_views[&area].screen_pos(glam::vec3(23., 26., 0.)).unwrap();
    h.hover_at(at);
    shoot(&mut h, &pictures, &format!("move-{stage}"));
    h.state_mut().area_views.get_mut(&area).unwrap().live.aim =
        Some(mg_ui::live_view::Aim::Attack("1".into()));
    let at = h.state().area_views[&area].screen_pos(glam::vec3(25., 23., 0.3)).unwrap();
    h.hover_at(at);
    shoot(&mut h, &pictures, &format!("attack-{stage}"));
    if stage == "after" {
        let v = h.state_mut().area_views.get_mut(&area).unwrap();
        v.live.aim = None;
        v.show_start = false;
        v.grid = false;
        let orbit = v.orbit.as_mut().unwrap();
        orbit.target = glam::vec3(21., 21., 0.8);
        orbit.distance = 7.;
        shoot(&mut h, &pictures, "player-close-after");
    }
}

/// One reusable live fixture: moving NPCs, a player, a destroyed chest and
/// a newly spawned chest. Pictures are outside the game installation.
#[test]
#[ignore]
fn live_mode() {
    use mg_module::instances::{Placement, Placing, instance};
    use mg_module::new::{AreaSpec, add_area, new_module};
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let install = GameInstall::new(&root, None, "en");
    let game = mg_rules::GameData::open(&install).unwrap();
    let mut rng = fastrand::Rng::with_seed(19);
    let mut module = new_module(&game, "Live mode fixture", &mut rng).unwrap();
    let area = add_area(
        &mut module,
        &game,
        &AreaSpec {
            name: "Live field".into(),
            tileset: mg_core::ResRef::from_str("ttr01").unwrap(),
            width: 4,
            height: 4,
        },
        &mut rng,
    )
    .unwrap();
    let git_key = ResKey::new(area, ResType::GIT);
    let mut git = module.gff(&git_key).unwrap().unwrap();
    let none = |_: mg_core::ResRef| None;
    let placing = Placing { game: &game, item: &none };
    for (list, restype, names, positions) in [
        (
            "Creature List",
            ResType::UTC,
            ["nw_bandit001", "nw_bartender"],
            [[17., 21., 0.], [23., 21., 0.]],
        ),
        (
            "Placeable List",
            ResType::UTP,
            ["plc_chest1", "plc_chest1"],
            [[17., 17., 0.], [23., 17., 0.]],
        ),
    ] {
        let objects = names
            .into_iter()
            .zip(positions)
            .map(|(name, position)| {
                let key = ResKey::parse(name, restype).unwrap();
                let bp = mg_gff::Gff::read(&game.resman.get(&key).unwrap()).unwrap();
                let mut item = instance(
                    &placing,
                    restype,
                    &bp.root,
                    Placement { position, rotation: 0. },
                    &[],
                )
                .unwrap();
                if restype == ResType::UTP {
                    item.set("Static", mg_gff::Value::Byte(0));
                }
                item
            })
            .collect();
        git.root.set(list, mg_gff::Value::List(objects));
    }
    module.set_gff(git_key, &git).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-mode/viewer");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fixture.mod");
    module.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&path);
    app.open_palette = false;
    let store = Arc::new(mg_ui::live::LiveStore::default());
    let now =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
            as u64;
    store.sync(serde_json::json!({"v":1,"boot":"picture","seq":1,"areas":[{
        "area":area.to_string(),"full":true,"cre":[
            {"id":"1","r":"nw_bandit001","name":"Guard","x":17,"y":25,"f":90,"hp":[18,30]},
            {"id":"2","r":"nw_bartender","name":"Innkeeper","x":25,"y":23,"f":180,"hp":[12,12]},
            {"id":"3","name":"Player","pc":true,"x":21,"y":21,"hp":[24,30]}],
        "plc":[{"id":"4","r":"plc_chest1","x":17,"y":17,"open":true,"lock":true},
            {"id":"5","r":"plc_chest1","name":"Live chest","own":true,"x":27,"y":18,"hp":[8,10]}]
    }]}).to_string().as_bytes(), now).unwrap();
    app.live = Some(store);
    app.settings.live = mg_live::LiveConfig {
        enabled: true,
        api_port: 0,
        redis_port: 1,
        keep_wsl: Some(false),
        ..Default::default()
    };
    app.settings.live.policy.chunks = true; // Explicit UI fixture policy; never a real server.
    app.apply_live(&egui::Context::default());
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280., 850.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    // Reuse the same authored module while keeping the v1 evidence intact.
    let pictures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-redis/visual");
    std::fs::create_dir_all(&pictures).unwrap();
    h.state_mut().area_views.get_mut(&area).unwrap().live.on = true;
    h.run_steps(3);
    let at = h.state().area_views[&area].screen_pos(glam::Vec3::new(17., 25., 0.3)).unwrap();
    h.hover_at(at);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(2);
    // The same harness captures the unchanged menu before implementation and
    // the actual controls afterwards, without duplicating a module or assets.
    let scripts = h.query_by_label("Run Script…").is_some();
    shoot(&mut h, &pictures, if scripts { "scripts-menu-after" } else { "scripts-menu-before" });
    if scripts {
        h.get_by_label("Run Script…").click();
        shoot(&mut h, &pictures, "run-script-after");
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        h.hover_at(at);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run_steps(2);
        h.get_by_label("Run NWScript…").click();
        shoot(&mut h, &pictures, "run-chunk-after");
    }
}

/// Render actual WSL reports saved by tools/live/test-demo.ps1. This exercises
/// the real viewer with engine data, separately from the desktop TLS process.
#[test]
#[ignore]
fn live_demo_snapshot() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-demo");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app = Moonglow::new(
        Some(GameInstall::new(&root, Some(dir.join("user")), "en")),
        Box::new(NoDialogs::default()),
    );
    app.set_render_state(rs.clone());
    app.open_module(&dir.join("DEMO-Live-01.mod"));
    app.open_palette = false;
    let store = Arc::new(mg_ui::live::LiveStore::default());
    app.live = Some(store.clone());
    app.settings.live = mg_live::LiveConfig {
        enabled: true,
        api_port: 0,
        redis_port: 1,
        keep_wsl: Some(false),
        ..Default::default()
    };
    app.apply_live(&egui::Context::default());
    let area = mg_core::ResRef::from_str("demo_area").unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600., 1000.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    h.state_mut().area_views.get_mut(&area).unwrap().live.on = true;
    // A live NPC must have drawable geometry, not merely a report marker.
    for name in ["city_actor", "nw_bartender"] {
        let app = h.state_mut();
        let game = app.game.as_deref().unwrap();
        let bp = mg_module::gff_root(
            Some(&app.ws.as_ref().unwrap().module),
            game,
            &ResKey::parse(name, ResType::UTC).unwrap(),
        )
        .unwrap();
        let items = |r| {
            mg_module::gff_root(
                Some(&app.ws.as_ref().unwrap().module),
                game,
                &ResKey::new(r, ResType::UTI),
            )
        };
        let item = mg_module::instances::instance(
            &mg_module::instances::Placing { game, item: &items },
            ResType::UTC,
            &bp,
            mg_module::instances::Placement::default(),
            &[],
        )
        .unwrap();
        let object = mg_area::AreaObject::read(game, mg_area::ObjectKind::Creature, 0, &item);
        let (instances, bounds) = mg_area::AreaScene::default().ghost(
            &app.viewport.as_ref().unwrap().gpu,
            game,
            &object,
            0.,
            1.,
        );
        eprintln!(
            "{name}: {} instances, bounds {bounds:?}, visual {:?}, problem {:?}",
            instances.len(),
            object.visual,
            object.problem
        );
        assert!(!instances.is_empty(), "{name} must draw a model: {:?}", object.problem);
    }
    for (seq, name) in ["move-before", "move-after", "spawn-door"].into_iter().enumerate() {
        let a: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(format!("{name}.json"))).unwrap())
                .unwrap();
        assert_eq!(a["area"], area.to_string());
        assert!(a["creatures"].as_array().unwrap().len() >= 37);
        let report = serde_json::json!({"v":1,"boot":"recorded-wsl-proof","seq":seq+1,"areas":[{
            "area":a["area"],"full":true,"cre":a["creatures"],"plc":a["placeables"],
            "doors":a["doors"],"items":a["items"]
        }]});
        let now =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
                as u64;
        store.sync(report.to_string().as_bytes(), now).unwrap();
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.grid = false;
        for kind in [
            mg_area::ObjectKind::Waypoint,
            mg_area::ObjectKind::Sound,
            mg_area::ObjectKind::Trigger,
            mg_area::ObjectKind::Encounter,
        ] {
            view.show[kind.index()] = false;
        }
        if seq == 0 {
            let patrol =
                a["creatures"].as_array().unwrap().iter().find(|c| c["tag"] == "C4_ACTOR").unwrap();
            let orbit = view.orbit.as_mut().unwrap();
            orbit.target = glam::vec3(
                patrol["x"].as_f64().unwrap() as f32,
                patrol["y"].as_f64().unwrap() as f32,
                0.,
            );
            orbit.distance = 24.;
            orbit.pitch = 55_f32.to_radians();
            orbit.yaw = 0.;
        }
        if name == "spawn-door" {
            let own = a["creatures"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["own"] == true)
                .expect("engine must report the spawned object");
            view.live.selected = Some(own["id"].as_str().unwrap().into());
            let orbit = view.orbit.as_mut().unwrap();
            orbit.target = glam::vec3(
                own["x"].as_f64().unwrap() as f32,
                own["y"].as_f64().unwrap() as f32,
                1.,
            );
            orbit.distance = 14.;
            orbit.pitch = 45_f32.to_radians();
        }
        shoot(&mut h, &dir, name);
        if name == "spawn-door" {
            let door = a["doors"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["open"] == true)
                .expect("engine must report the open door");
            let view = h.state_mut().area_views.get_mut(&area).unwrap();
            view.orbit.as_mut().unwrap().target = glam::vec3(
                door["x"].as_f64().unwrap() as f32,
                door["y"].as_f64().unwrap() as f32,
                0.,
            );
            view.orbit.as_mut().unwrap().distance = 24.;
            shoot(&mut h, &dir, "door-open");
        }
    }
}

/// Morning pictures use the same real engine reports as the curl acceptance.
#[test]
#[ignore]
fn live_morning_snapshot() {
    live_morning_pictures(None);
}

#[test]
#[ignore]
fn live_labels_snapshot() {
    live_morning_pictures(Some("labels"));
}

#[test]
#[ignore]
fn live_panel_snapshot() {
    live_morning_pictures(Some("panel"));
}

#[test]
#[ignore]
fn live_combat_snapshot() {
    live_morning_pictures(Some("combat"));
}

#[test]
#[ignore]
fn live_factions_snapshot() {
    live_morning_pictures(Some("factions"));
}

#[test]
#[ignore]
fn live_motion_snapshot() {
    live_morning_pictures(Some("motion"));
}

#[test]
#[ignore]
fn live_grid_snapshot() {
    live_morning_pictures(Some("grid"));
}

#[test]
#[ignore]
fn live_effects_snapshot() {
    live_morning_pictures(Some("effects"));
}

#[test]
#[ignore]
fn live_full_effects_snapshot() {
    live_morning_pictures(Some("full-effects"));
}

#[test]
#[ignore]
fn live_event_effects_snapshot() {
    live_morning_pictures(Some("event-effects"));
}

#[test]
#[ignore]
fn live_speech_snapshot() {
    live_morning_pictures(Some("speech"));
}

#[test]
#[ignore]
fn live_review_legend_snapshot() {
    live_morning_pictures(Some("review-legend"));
}

#[test]
#[ignore]
fn live_review_group_snapshot() {
    live_morning_pictures(Some("review-group"));
}

#[test]
#[ignore]
fn live_review_props_snapshot() {
    live_morning_pictures(Some("review-props"));
}

#[test]
#[ignore]
fn live_review_lightshaft_snapshot() {
    live_morning_pictures(Some("review-lightshaft"));
}

#[test]
#[ignore]
fn live_review_profile_snapshot() {
    live_morning_pictures(Some("review-profile"));
}

#[test]
#[ignore]
fn live_review_spells_snapshot() {
    live_morning_pictures(Some("review-spells"));
}

#[test]
#[ignore]
fn live_review_patrol_snapshot() {
    live_morning_pictures(Some("review-patrol"));
}

#[test]
#[ignore]
fn live_review_tiles_snapshot() {
    live_morning_pictures(Some("review-tiles"));
}

#[test]
#[ignore]
fn live_accept2_baselines() {
    for component in [
        "review-group",
        "review-props",
        "review-legend",
        "review-tiles",
        "review-layout",
        "review-actions",
        "review-appearance",
    ] {
        live_morning_pictures(Some(component));
    }
}

#[test]
#[ignore]
fn live_accept2_layout_snapshot() {
    live_morning_pictures(Some("review-layout"));
}
#[test]
#[ignore]
fn live_accept2_actions_snapshot() {
    live_morning_pictures(Some("review-actions"));
}
#[test]
#[ignore]
fn live_accept2_appearance_snapshot() {
    live_morning_pictures(Some("review-appearance"));
}

fn live_morning_pictures(component: Option<&str>) {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-demo");
    let morning = dir.join("morning");
    let evidence: serde_json::Value =
        serde_json::from_slice(&std::fs::read(morning.join("curl-morning.json")).unwrap()).unwrap();
    assert_eq!(evidence["passed"], true);
    assert_eq!(evidence["walk"]["arrived"], true);
    let area = mg_core::ResRef::from_str(evidence["area"].as_str().unwrap()).unwrap();
    let selected = evidence["target"].as_str().unwrap();
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app = Moonglow::new(
        Some(GameInstall::new(&root, Some(dir.join("user")), "en")),
        Box::new(NoDialogs::default()),
    );
    app.set_render_state(rs.clone());
    app.open_module(&dir.join("DEMO-Live-01.mod"));
    app.open_palette = false;
    let store = Arc::new(mg_ui::live::LiveStore::default());
    app.live = Some(store.clone());
    app.settings.live = mg_live::LiveConfig {
        enabled: true,
        api_port: 0,
        redis_port: 1,
        keep_wsl: Some(false),
        ..Default::default()
    };
    app.apply_live(&egui::Context::default());
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_step_dt(if matches!(component, Some("motion" | "full-effects" | "event-effects")) {
            0.1
        } else {
            1. / 60.
        })
        .with_size(egui::vec2(if component == Some("grid") { 1280. } else { 1600. }, 1000.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    let states: &[&str] = if component.is_some() { &["after"] } else { &["before", "after"] };
    for (seq, &name) in states.iter().enumerate() {
        let mut a: serde_json::Value = serde_json::from_slice(
            &std::fs::read(morning.join(format!("curl-{name}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(a["area"], area.to_string());
        let npc = a["creatures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == selected)
            .expect("accepted target must remain reported")
            .clone();
        if name == "after" {
            assert_eq!(npc["name"], "Chunk ran");
        }
        if matches!(component, Some("review-lightshaft" | "review-profile")) {
            let game = h.state().game.as_ref().unwrap();
            let key = game
                .resman
                .list(ResType::UTP)
                .into_iter()
                .find(|r| {
                    let Ok(data) = game.resman.get(&ResKey::new(*r, ResType::UTP)) else {
                        return false;
                    };
                    let Ok(g) = mg_gff::Gff::read(&data) else { return false };
                    let name = g
                        .root
                        .locstring("LocName")
                        .and_then(|n| game.locstring(n))
                        .unwrap_or_default()
                        .to_lowercase();
                    name.contains("lightshaft") && name.contains("green")
                })
                .expect("installed Green Lightshaft blueprint");
            eprintln!("Green Lightshaft blueprint: {key}");
            if component == Some("review-profile") {
                let originals = a["creatures"].as_array().unwrap().clone();
                a["creatures"] = serde_json::json!(
                    (0..217)
                        .map(|n| {
                            let mut o = originals[n % originals.len()].clone();
                            o["id"] = format!("{:x}", 4096 + n).into();
                            o["name"] = format!("Crowd {n}").into();
                            o["x"] = (16. + (n % 15) as f32 * 1.2).into();
                            o["y"] = (16. + (n / 15) as f32 * 1.2).into();
                            o["pc"] = false.into();
                            o["dm"] = false.into();
                            o["possessed"] = false.into();
                            o
                        })
                        .collect::<Vec<_>>()
                );
            } else {
                a["creatures"] = serde_json::json!([]);
            }
            a["placeables"] = serde_json::json!([{"id":"ff11","r":key.to_string(),"name":"Lightshaft, Green","x":25,"y":25,"own":true}]);
        }
        if component == Some("review-appearance") {
            let game = h.state().game.as_ref().unwrap();
            let table = game.table("placeables").unwrap();
            let row = (0..table.len())
                .find(|&r| {
                    table.get(r, "Label").is_some_and(|v| v.to_lowercase().contains("cabinet"))
                })
                .unwrap();
            a["creatures"] = serde_json::json!([]);
            a["placeables"] = serde_json::json!([{"id":"ff10","r":"city_crate","app":row,"name":"Reported cabinet","x":25,"y":25,"own":true}]);
        }
        if component == Some("review-actions") {
            let mut player = npc.clone();
            player["pc"] = true.into();
            player["name"] = "Player actions".into();
            player["x"] = 25.into();
            player["y"] = 25.into();
            a["creatures"] = serde_json::json!([player]);
            h.state_mut().settings.live.policy.players = true;
            store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
        }
        if component == Some("review-props") {
            a["creatures"] = serde_json::json!([]);
            a["placeables"] = serde_json::json!([{"id":"ff10","r":"city_crate","name":"Live chest","x":25,"y":25,"f":45,"own":true,"useable":true,"vt":{"scale":1.3,"rotate":{"x":10,"y":20,"z":30},"translate":{"x":0.2,"y":0.3,"z":0.5}}}]);
        }
        if matches!(component, Some("review-group" | "review-patrol")) {
            a["size"] = serde_json::json!([120, 120]);
            let base = npc.clone();
            a["creatures"] = serde_json::json!(
                (0..3)
                    .map(|n| {
                        let mut o = base.clone();
                        o["id"] = format!("ff{}", n).into();
                        o["name"] = format!("Squad {}", n + 1).into();
                        o["x"] = (23 + 2 * n).into();
                        o["y"] = 25.into();
                        o
                    })
                    .collect::<Vec<_>>()
            );
        }
        if matches!(component, Some("combat" | "factions" | "motion")) {
            // Controlled overlay states on the recorded crowd, not a new engine proof.
            let mut target = npc.clone();
            target["id"] = "ffe1".into();
            target["name"] = "Combat target".into();
            target["x"] = 29.0.into();
            target["y"] = 25.0.into();
            target["combat"] = true.into();
            target["own"] = false.into();
            let mut corpse = target.clone();
            corpse["id"] = "ffe2".into();
            corpse["name"] = "Fallen creature".into();
            corpse["x"] = 27.0.into();
            corpse["y"] = 21.0.into();
            corpse["dead"] = true.into();
            corpse["combat"] = false.into();
            corpse["hp"] = serde_json::json!([0, 10]);
            let creatures = a["creatures"].as_array_mut().unwrap();
            let attacker = creatures.iter_mut().find(|c| c["id"] == selected).unwrap();
            attacker["name"] = "Combat attacker".into();
            attacker["combat"] = true.into();
            attacker["target"] = "ffe1".into();
            if component == Some("motion") {
                attacker["name"] = "Moving creature".into();
                attacker["combat"] = false.into();
                attacker["target"] = serde_json::Value::Null;
                attacker["x"] = 23.0.into();
            }
            creatures.extend([target, corpse]);
            if matches!(component, Some("factions" | "motion")) {
                for o in creatures {
                    o["faction"] = if o["id"] == selected {
                        4
                    } else if o["id"] == "ffe1" {
                        1
                    } else {
                        2
                    }
                    .into();
                    o["friendly"] = serde_json::json!([]);
                }
            }
        }
        if matches!(
            component,
            Some("effects" | "full-effects" | "event-effects" | "speech" | "review-spells")
        ) {
            let creatures = a["creatures"].as_array_mut().unwrap();
            let mut actor = creatures.iter().find(|o| o["id"] == selected).unwrap().clone();
            actor["x"] = 25.into();
            actor["y"] = 25.into();
            actor["name"] = "Red glow".into();
            actor["vfx"] = serde_json::json!([411]);
            let mut other = actor.clone();
            other["id"] = "ffe1".into();
            other["y"] = 29.into();
            other["name"] = "Blue glow".into();
            other["vfx"] = serde_json::json!([410]);
            let mut unknown = actor.clone();
            unknown["id"] = "ffe2".into();
            unknown["y"] = 21.into();
            unknown["name"] = "Unknown effect".into();
            unknown["vfx"] = serde_json::json!([999999]);
            if component == Some("full-effects") {
                let table = h.state().game.as_ref().unwrap().table("visualeffects").unwrap();
                let row = (0..table.len())
                    .find(|&r| table.get(r, "Label") == Some("VFX_DUR_DARKNESS"))
                    .unwrap();
                other["name"] = "Darkness".into();
                other["vfx"] = serde_json::json!([row]);
                other["y"] = 18.into();
                unknown["y"] = 32.into();
            }
            if matches!(component, Some("event-effects" | "speech" | "review-spells")) {
                actor["name"] = "Caster".into();
                other["name"] = "Projectile target".into();
                unknown["name"] = "Damage target".into();
                other["y"] = 18.into();
                unknown["y"] = 32.into();
                for object in [&mut actor, &mut other, &mut unknown] {
                    object["vfx"] = serde_json::json!([]);
                }
            }
            if component == Some("speech") {
                actor["name"] = "Captain".into();
                other["name"] = "Guard".into();
                unknown["name"] = "Lookout".into();
            }
            *creatures = vec![actor, other, unknown];
        }
        let mut report = serde_json::json!({"v":1,"boot":"morning-recorded-proof","seq":seq+1,"areas":[{
            "area":a["area"],"full":true,"cre":a["creatures"],"plc":a["placeables"],
            "doors":a["doors"],"items":a["items"]
        }]});
        if let Some(size) = a.get("size") {
            report["areas"][0]["size"] = size.clone();
        }
        let reported_ms = mg_live::now_ms();
        store.sync(report.to_string().as_bytes(), reported_ms).unwrap();
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.live.on = true;
        view.live.selected =
            Some(if component == Some("review-props") { "ff10" } else { selected }.into());
        if component == Some("full-effects") {
            view.live.effects = mg_ui::live_effects::EffectsMode::Full;
        }
        if component.is_some() {
            view.live.labels = mg_ui::live_view::LabelMode::All;
            if !matches!(component, Some("factions" | "motion")) {
                view.live.colors = mg_ui::live_colors::ColorMode::Kind;
            }
        }
        view.grid = false;
        for kind in [
            mg_area::ObjectKind::Waypoint,
            mg_area::ObjectKind::Sound,
            mg_area::ObjectKind::Trigger,
            mg_area::ObjectKind::Encounter,
        ] {
            view.show[kind.index()] = false;
        }
        let orbit = view.orbit.as_mut().unwrap();
        orbit.target = glam::vec3(25., 25., 0.);
        orbit.distance = 38.;
        orbit.pitch = 55_f32.to_radians();
        orbit.yaw = 0.;
        if let Some(component) = component {
            let pictures = dir.join("../live-next/visual");
            std::fs::create_dir_all(&pictures).unwrap();
            let stage = std::env::var("MGLIVE_PICTURE_STAGE").unwrap_or_else(|_| "after".into());
            assert!(stage.ends_with("before") || stage.ends_with("after"));
            if component == "full-effects" {
                // Compare modes with identical inputs, unobstructed on the paving.
                view.orbit.as_mut().unwrap().distance = 24.;
                view.live.effects = if stage.ends_with("before") {
                    mg_ui::live_effects::EffectsMode::Markers
                } else {
                    mg_ui::live_effects::EffectsMode::Full
                };
            }
            if matches!(component, "event-effects" | "speech" | "review-spells") {
                view.orbit.as_mut().unwrap().distance = 24.;
            }
            if component == "panel" {
                view.live.hide_module_markers = true;
                h.run_steps(3); // Let the default Live layout finish selecting its tabs first.
                h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LivePanel));
                h.run_steps(3);
                h.get_by_label("Search live objects");
            }
            if component == "motion" {
                h.state_mut().area_views.get_mut(&area).unwrap().live.trail = true;
                h.run_steps(3);
                report["seq"] = 2.into();
                let actor = report["areas"][0]["cre"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|o| o["id"] == selected)
                    .unwrap();
                actor["x"] = 27.0.into();
                actor["f"] = 45.0.into();
                store.sync(report.to_string().as_bytes(), reported_ms + 1000).unwrap();
                h.run_steps(3);
            }
            if component == "full-effects" {
                h.run_steps(12);
            }
            if component == "grid" {
                h.state_mut().area_views.get_mut(&area).unwrap().live.on = false;
                h.run_steps(3);
                h.get_by_label("⊞ Grid").click();
                h.run_steps(2);
                assert!(h.state().area_views[&area].grid, "Grid is reachable with Live off");
            }
            if component == "review-spells" {
                h.run_steps(3);
                let game = h.state().game.as_ref().unwrap();
                let table = game.table("spells").unwrap();
                let spell = (0..table.len())
                    .find(|&r| {
                        table.get(r, "Label").is_some_and(|s| s.eq_ignore_ascii_case("Fireball"))
                    })
                    .unwrap();
                let script = table.get(spell, "ImpactScript").unwrap_or_default();
                let source = game
                    .resman
                    .get_named(script, ResType::NSS)
                    .ok()
                    .map(|d| String::from_utf8_lossy(&d).into_owned())
                    .unwrap_or_default();
                let spec = mg_script::spec::Spec::parse(
                    &game.resman.get_named("nwscript", ResType::NSS).unwrap(),
                );
                let probe = serde_json::json!({"spell":spell,"script":script,"target_shape":table.get(spell,"TargetShape"),"target_size_x":table.get(spell,"TargetSizeX"),"target_size_y":table.get(spell,"TargetSizeY"),"columns":table.columns(),"radius_lines":source.lines().filter(|l|l.contains("RADIUS_")||l.contains("Shape")).collect::<Vec<_>>(),"radius_constants":spec.constants.iter().filter(|c|c.name.starts_with("RADIUS_")).map(|c|(&c.name,&c.value)).collect::<Vec<_>>()});
                std::fs::write(
                    dir.join("../live-next/review-spells-data.json"),
                    serde_json::to_vec_pretty(&probe).unwrap(),
                )
                .unwrap();
                store.sync(serde_json::json!({"v":1,"boot":"morning-recorded-proof","seq":2,"areas":[],"events":[
                    {"seq":1,"kind":"spell","area":area.to_string(),"id":selected,"position":{"x":25,"y":24,"z":0},"spell":spell,"instant":true}
                ]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
            }
            if component == "event-effects" {
                h.run_steps(3);
                store.sync(serde_json::json!({"v":1,"boot":"morning-recorded-proof","seq":2,"areas":[],"events":[
                    {"seq":1,"kind":"spell","area":area.to_string(),"id":selected,"target":"ffe1","spell":107,"instant":false},
                    {"seq":2,"kind":"vfx","subtype":"projectile","area":area.to_string(),"id":selected,"target":"ffe1","spell":107,"delta_ms":700},
                    {"seq":3,"kind":"damage","area":area.to_string(),"id":"ffe1","target":"ffe2","amount":7}
                ]}).to_string().as_bytes(), mg_live::now_ms()).unwrap();
            }
            if component == "speech" {
                h.run_steps(3);
                store.sync(serde_json::json!({"v":1,"boot":"morning-recorded-proof","seq":2,"areas":[],"events":[
                    {"seq":1,"kind":"chat","area":area.to_string(),"id":selected,"text":"Za mną! Pilnujcie bramy.","channel":1},
                    {"seq":2,"kind":"chat","area":area.to_string(),"id":"ffe1","text":"Zrozumiano.","channel":1}
                ]}).to_string().as_bytes(), mg_live::now_ms()).unwrap();
            }
            if component == "review-lightshaft" {
                h.run_steps(10);
            }
            if component == "review-props" {
                h.run_steps(3);
                if stage.ends_with("before") {
                    let pos =
                        h.state().area_views[&area].screen_pos(glam::vec3(25., 25., 0.3)).unwrap();
                    h.hover_at(pos);
                    for pressed in [true, false] {
                        h.event(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                    h.run_steps(2);
                    h.get_by_label("Visual Transform ⏵").click();
                    h.run_steps(2);
                } else {
                    let pos =
                        h.state().area_views[&area].screen_pos(glam::vec3(25., 25., 0.3)).unwrap();
                    h.hover_at(pos);
                    for pressed in [true, false] {
                        h.event(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                    h.run_steps(2);
                    h.get_by_label("Visual Transform…").click();
                    h.run_steps(2);
                }
            }
            if matches!(component, "review-group" | "review-patrol") {
                h.run_steps(3);
                for n in 0..3 {
                    let pos = h.state().area_views[&area]
                        .screen_pos(glam::vec3(23. + 2. * n as f32, 25., 0.3))
                        .unwrap();
                    h.hover_at(pos);
                    for pressed in [true, false] {
                        h.event_modifiers(
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::CTRL,
                            },
                            egui::Modifiers::CTRL,
                        );
                    }
                    h.run_steps(2);
                }
            }
            if component == "review-patrol" {
                let pos =
                    h.state().area_views[&area].screen_pos(glam::vec3(25., 25., 0.3)).unwrap();
                h.hover_at(pos);
                for pressed in [true, false] {
                    h.event(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                h.run_steps(2);
                if !stage.ends_with("before") {
                    h.get_by_label("Patrol…").click();
                    h.run_steps(2);
                    for at in [
                        glam::vec3(20., 22., 0.),
                        glam::vec3(29., 22., 0.),
                        glam::vec3(29., 29., 0.),
                    ] {
                        let pos = h.state().area_views[&area].screen_pos(at).unwrap();
                        h.hover_at(pos);
                        for pressed in [true, false] {
                            h.event(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            });
                        }
                        h.run_steps(3);
                    }
                    h.get_by_label("Loop").click();
                    h.run_steps(2);
                }
            }
            if component == "review-tiles"
                && (stage == "accept2-palette-after" || stage.starts_with("accept3"))
            {
                h.run_steps(3);
                h.state_mut().palette.area = Some(area);
                h.state_mut().palette.tiles = true;
                h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Palette));
                h.run_steps(4);
                let tile = mg_area::tileset(
                    h.state().game.as_deref().unwrap(),
                    h.state().area_views[&area].model.as_ref().unwrap().tileset.unwrap(),
                )
                .unwrap()
                .tiles[0]
                    .model
                    .clone();
                h.get_by_label(&format!("Use tile {tile}")).click();
                h.run_steps(2);
                let pos = h.state().area_views[&area].screen_pos(glam::vec3(25., 35., 0.)).unwrap();
                h.hover_at(pos);
                h.key_press(egui::Key::Q);
                h.run_steps(20);
            }
            if component == "review-tiles"
                && !stage.ends_with("before")
                && stage != "accept2-palette-after"
                && !stage.starts_with("accept3")
            {
                h.state_mut().area_views.get_mut(&area).unwrap().tile_mode = true;
                h.run_steps(3);
                let pos = h.state().area_views[&area].screen_pos(glam::vec3(25., 35., 0.)).unwrap();
                let model = h.state().area_views[&area].model.as_ref().unwrap();
                let original = model.tiles[(3 * model.width + 2) as usize].clone();
                h.hover_at(pos);
                for pressed in [true, false] {
                    h.event(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                h.run_steps(2);
                h.get_by_label("Advanced").click();
                h.run_steps(2);
                for (label, value) in [("Tile ID", "0"), ("Orientation", "1"), ("Tile height", "1")]
                {
                    h.get_by_label(label).click();
                    h.run_steps(1);
                    h.event(egui::Event::Text(value.into()));
                    h.key_press(egui::Key::Enter);
                    h.run_steps(2);
                }
                h.get_by_label("Apply Live Tile").click();
                h.run_steps(2);
                shoot(&mut h, &pictures, "review-tiles-confirm");
                h.get_by_label("Confirm Live Painting").click();
                h.run_steps(2);
                let q = store.status(mg_live::now_ms())["queued"][0].clone();
                assert_eq!(q["op"], "set_tile");
                store.sync(serde_json::json!({"v":1,"boot":"morning-recorded-proof","seq":2,"areas":[],"results":[{"id":q["id"],"ok":true}],"events":[{"seq":1,"kind":"tile","area":area.to_string(),"x":q["x"],"y":q["y"],"tile_id":q["tile_id"],"orientation":q["orientation"],"height":q["height"],"before":{"tile_id":original.id,"orientation":original.orientation,"height":original.height}}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
                h.run_steps(3);
                assert!(!h.state().has_unsaved_work());
            }
            if component == "review-layout" {
                h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveLog));
                h.run_steps(4);
            }
            if component == "review-actions" {
                h.run_steps(4);
                let pos =
                    h.state().area_views[&area].screen_pos(glam::vec3(25., 25., 0.3)).unwrap();
                h.hover_at(pos);
                for pressed in [true, false] {
                    h.event(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                h.run_steps(2);
                if stage.starts_with("stop-attack") || !stage.ends_with("before") {
                    h.get_by_label("Player ⏵").click();
                    h.run_steps(2);
                    if stage.starts_with("stop-attack") {
                        if stage.ends_with("after") {
                            h.get_by_label("Stop Attack");
                        }
                    } else if stage.contains("player") {
                        h.get_by_label("Boot…").click();
                    } else {
                        h.get_by_label("Give Gold ⏵").click();
                    }
                    h.run_steps(2);
                }
            }
            if component == "review-legend"
                && !stage.ends_with("before")
                && !stage.starts_with("accept3")
            {
                h.run_steps(3);
                if stage.starts_with("accept2") {
                    h.get_by_label("Placeables").click();
                    h.get_by_label("NPC factions").click();
                } else {
                    h.get_by_label("- Legend").click();
                }
                h.run_steps(2);
            }
            if component == "review-group"
                && stage.starts_with("accept2")
                && !stage.ends_with("before")
            {
                h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::LiveSelection));
                h.run_steps(3);
            }
            if component == "review-group"
                && (stage.starts_with("teleport")
                    || stage.starts_with("stop-attack")
                    || stage.starts_with("accept3"))
            {
                let pos =
                    h.state().area_views[&area].screen_pos(glam::vec3(25., 25., 0.3)).unwrap();
                h.hover_at(pos);
                for pressed in [true, false] {
                    h.event(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                h.run_steps(3);
                if stage.starts_with("teleport") && !stage.ends_with("before") {
                    shoot(&mut h, &pictures, "review-teleport-menu-after");
                    h.get_by_label(if stage == "teleport-area-after" {
                        "Teleport to Area…"
                    } else {
                        "Teleport…"
                    })
                    .click();
                    h.run_steps(3);
                    if stage == "teleport-area-after" {
                        h.get_by_label("Area Centre");
                    }
                    if stage != "teleport-area-after" {
                        let at = h.state().area_views[&area]
                            .screen_pos(glam::vec3(23., 29., 0.))
                            .unwrap();
                        h.hover_at(at);
                        h.key_press(egui::Key::Q);
                        h.run_steps(3);
                    }
                }
            }
            if component != "review-profile" || stage != "accept3-effects-after" {
                shoot(&mut h, &pictures, &format!("{component}-{stage}"));
            }
            if component == "review-profile" {
                if stage == "accept3-effects-after" {
                    let mut effects_report = report.clone();
                    effects_report["seq"] = (seq + 2).into();
                    for creature in effects_report["areas"][0]["cre"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .take(16)
                    {
                        creature["vfx"] = serde_json::json!([7]);
                    }
                    store.sync(effects_report.to_string().as_bytes(), mg_live::now_ms()).unwrap();
                    h.state_mut().area_views.get_mut(&area).unwrap().live.effects =
                        mg_ui::live_effects::EffectsMode::Full;
                    h.run_steps(90);
                    assert!(h.state().area_views[&area].effect_models_shown >= 16);
                    assert!(h.state().area_views[&area].live_particles_shown > 0);
                    shoot(&mut h, &pictures, &format!("{component}-{stage}"));
                }
                let mut timing = live_timing::measure(&mut h, area);
                if stage == "accept3-effects-after" {
                    timing["variant"]="First 16 creatures have vfx:[7] (mind-affecting negative); base fixture unchanged".into();
                    timing["effect_models"] =
                        h.state().area_views[&area].effect_models_shown.into();
                }
                timing["objects"] = ["cre", "plc", "doors", "items"]
                    .iter()
                    .map(|key| report["areas"][0][key].as_array().map_or(0, Vec::len))
                    .sum::<usize>()
                    .into();
                timing["crowd"] = 218.into();
                timing["fixture"] = "review-profile-fixture.json".into();
                let fixture = dir.join("../live-next/review-profile-fixture.json");
                if fixture.exists() {
                    let existing: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(&fixture).unwrap()).unwrap();
                    // Compare both through the wire decoder: serde_json without float_roundtrip
                    // can differ by one f64 ULP from an in-memory f32 coordinate.
                    let wire: serde_json::Value =
                        serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
                    assert!(existing == wire, "profile must use the same replay input");
                } else {
                    std::fs::write(&fixture, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
                }
                std::fs::write(
                    dir.join(format!("../live-next/review-profile-{stage}.json")),
                    serde_json::to_vec_pretty(&timing).unwrap(),
                )
                .unwrap();
            }
            if component == "full-effects" {
                let view = &h.state().area_views[&area];
                eprintln!(
                    "Full fixture: {} attached models, {} live particles",
                    view.effect_models_shown, view.live_particles_shown
                );
                if !stage.ends_with("before") {
                    assert!(view.effect_models_shown > 0 && view.live_particles_shown > 0);
                }
            }
            if component == "factions" {
                h.state_mut().actions.push(mg_ui::Action::LiveOptions);
                h.run_steps(3);
                place(&mut h, &mg_ui::Tab::Options, (330., 190.), (900., 600.));
                h.run_steps(2);
                h.get_by_label("Factions").click();
                shoot(&mut h, &pictures, "factions-options-after");
            }
        } else {
            shoot(&mut h, &morning, name);
        }
    }
}

#[test]
#[ignore]
fn script_wizard_pages() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    let dir = mg_testkit::output_dir("screens");
    let lists = Arc::new(Lists::load(app.game.as_deref().unwrap()));
    let key = ResKey::parse("dlg", ResType::DLG).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    for condition in [true, false] {
        let mut w = ScriptWizard::new(
            key,
            GffPath::root(),
            "Active",
            condition,
            lists.clone(),
            "sc_001".into(),
        );
        w.classes.push(ClassLevel { class: None, level: Some(5) });
        w.items.push("key_1".into());
        w.give_items.push("nw_wswls001".into());
        w.perform = Perform::Store { tag: "store_1".into(), appraise: true };
        let pages = w.pages.len();
        let prefix = if condition { "condition" } else { "action" };
        h.state_mut().script_wizard = Some(w.clone());
        shoot(&mut h, &dir, &format!("wizard-{prefix}-choose"));
        for i in 0..pages {
            w.step = Step::Page(i);
            h.state_mut().script_wizard = Some(w.clone());
            shoot(&mut h, &dir, &format!("wizard-{prefix}-{i:02}"));
        }
        w.step = Step::Name;
        h.state_mut().script_wizard = Some(w.clone());
        shoot(&mut h, &dir, &format!("wizard-{prefix}-name"));
    }
}

/// Module Properties › Custom Content: the hak list, two rows chosen and
/// some haks not found.
#[test]
#[ignore]
fn module_properties_haks() {
    use mg_schema::{ExoString, StructExt, ifo};
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let user = dir.join("haks-user");
    std::fs::create_dir_all(user.join("hak")).unwrap();
    let names = ["bloodwar", "bmmountainsv102", "cb_tile_atemple", "dia_udrkspid3", "ghcastlehd"];
    for name in &names[..3] {
        let empty = mg_erf::ErfWriter::new(*b"HAK ").to_bytes().unwrap();
        std::fs::write(user.join(format!("hak/{name}.hak")), empty).unwrap();
    }
    let mut m = mg_module::Module::new();
    let mut info = mg_gff::Gff::new(*b"IFO ");
    let haks = names
        .iter()
        .map(|n| {
            let mut item = ifo::MOD_HAK_LIST.new_item();
            item.write(&ifo::mod_hak_list::MOD_HAK, ExoString::from(*n));
            item
        })
        .collect();
    info.root.set(ifo::MOD_HAK_LIST.label, mg_gff::Value::List(haks));
    m.set_info(&info).unwrap();
    let path = dir.join("haks.mod");
    m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    let mut app = Moonglow::new(
        Some(GameInstall::new(&root, Some(user), "en")),
        Box::new(NoDialogs::default()),
    );
    app.open_module(&path);
    app.open_palette = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    h.get_by_label("Custom Content").click();
    h.run_steps(2);
    h.state_mut().hak_list.selected = [1, 2].into_iter().collect();
    shoot(&mut h, &dir, "module-properties-haks");
}

/// The 2DA editor on a copy of the game's feat.2da kept as a file: a
/// cell that names a feat that is not there, selected; and Take Rows,
/// with the game's copy to take from.
#[test]
#[ignore]
fn table_editor() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-table");
    let install = GameInstall::new(&root, None, "en");
    let game = mg_rules::GameData::open(&install).unwrap();
    // The game's table with a feat changed, one wrong, and its last rows
    // not there.
    let mut t = (*game.table("feat").unwrap()).clone();
    let (prereq, label) = (t.column("PREREQFEAT1").unwrap(), t.column("LABEL").unwrap());
    t.rows.truncate(t.len() - 25);
    t.set(3, prereq, Some("4000".into())).unwrap();
    t.set(5, label, Some("MyOwnDodge".into())).unwrap();
    let file = dir.join("feat.2da");
    std::fs::write(&file, t.to_bytes(mg_core::Codepage::default()).unwrap()).unwrap();
    let app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let id = mg_ui::table_view::open_path(h.state_mut(), &file).unwrap();
    h.run_steps(6);
    {
        let doc = h.state_mut().tables.iter_mut().find(|d| d.id == id).unwrap();
        doc.go_to(3, prereq);
    }
    h.run_steps(6);
    shoot(&mut h, &dir, "table-editor");
    // Typing in a cell.
    {
        let doc = h.state_mut().tables.iter_mut().find(|d| d.id == id).unwrap();
        doc.go_to(5, label);
        doc.typing = Some("MyOwnDodge".into());
    }
    h.run_steps(4);
    shoot(&mut h, &dir, "table-editor-typing");
    {
        let doc = h.state_mut().tables.iter_mut().find(|d| d.id == id).unwrap();
        doc.typing = None;
        doc.taking = Some(Default::default());
    }
    h.run_steps(6);
    h.get_by_label("New").click();
    h.run_steps(3);
    shoot(&mut h, &dir, "table-editor-take-rows");
    // Several cells selected, and a column's menu.
    {
        let doc = h.state_mut().tables.iter_mut().find(|d| d.id == id).unwrap();
        doc.taking = None;
        doc.go_to(2, 0);
        doc.select_to((7, 2));
    }
    h.run_steps(4);
    let name = h.get_by_label("DESCRIPTION").rect().center();
    h.hover_at(name);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: name,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(3);
    shoot(&mut h, &dir, "table-editor-selection");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    // The window's close button, with nothing unsaved.
    h.state_mut().actions.push(mg_ui::Action::CloseWindow);
    h.run_steps(3);
    shoot(&mut h, &dir, "close-question");
    h.state_mut().closing = false;
    // Where a row is named, and a row of another table to choose from.
    {
        let doc = h.state_mut().tables.iter_mut().find(|d| d.id == id).unwrap();
        doc.go_to(3, prereq);
    }
    h.run_steps(4);
    h.get_by_label("Find References to Row 3").click();
    h.run_steps(4);
    shoot(&mut h, &dir, "table-editor-references");
    // The resource browser's Edit… on a table of the game's.
    let key = ResKey::parse("skills", ResType::TWODA).unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Resource(key)));
    h.run_steps(5);
    h.get_by_label("Edit…").click();
    h.run_steps(3);
    shoot(&mut h, &dir, "table-editor-edit-menu");
}

/// A table with a description beside it: values by name, a cell that is
/// none of its list, and a flags cell being ticked (the flags are the
/// game's own `DAMAGE_TYPE_` constants, read from `nwscript`).
#[test]
#[ignore]
fn described_table() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-described");
    let install = GameInstall::new(&root, None, "en");
    // (A project of its own: what the description names is looked for
    // here, not through this repository.)
    std::fs::write(dir.join("nasher.cfg"), "").unwrap();
    let file = dir.join("my_ai_spells.2da");
    std::fs::write(
        &file,
        "2DA V2.0\r\n\r\n  Label Level DamageType AOEShape Duration ClassRequired Cures\r\n\
         0 Acid_Fog 6 0x0010 0 2 **** ****\r\n1 Aid 2 **** **** 3 **** ****\r\n\
         2 Animate_Dead 3 **** **** 4 2 ****\r\n3 Barkskin 2 **** **** 4 **** ****\r\n\
         4 Bestow_Curse 3 **** **** 5 **** ****\r\n5 Blade_Barrier 6 0x0006 1 2 **** ****\r\n\
         6 Bless 1 **** **** 3 **** ****\r\n7 Bless_Weapon 12 **** **** 9 **** ****\r\n\
         8 Blindness_and_Deafness 2 **** **** #N/A 99 ****\r\n\
         9 Bulls_Strength 2 **** **** 4 **** ****\r\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("my_ai_spells.2da.toml"),
        "rows = \"spells\"\n\n[columns.Level]\nkind = \"int\"\nmin = -1\nmax = 10\n\n\
         [columns.DamageType]\nkind = \"flags\"\nconstants = \"nwscript\"\nprefix = \
         \"DAMAGE_TYPE_\"\n\n[columns.AOEShape]\nkind = \"list\"\nvalues = { Sphere = 0, \
         Wall = 1, Cone = 3 }\n\n[columns.Duration]\nkind = \"list\"\nvalues = { Instant = 1, \
         Short = 2, Medium = 3, Long = 4, Permanent = 5 }\n\n[columns.ClassRequired]\nkind = \
         \"row\"\ntable = \"classes\"\n",
    )
    .unwrap();
    let app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let id = mg_ui::table_view::open_path(h.state_mut(), &file).unwrap();
    h.run_steps(6);
    h.state_mut().tables.iter_mut().find(|d| d.id == id).unwrap().go_to(5, 2);
    h.run_steps(4);
    let combo = egui_kittest::kittest::by().role(egui::accesskit::Role::ComboBox);
    let at = h
        .query_all(combo)
        .find(|n| n.accesskit_node().value().is_some_and(|v| v == "2 flags"))
        .map(|n| n.rect().center())
        .expect("the flags' list");
    h.hover_at(at);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_steps(4);
    shoot(&mut h, &dir, "described-table");
}

/// The conversation editor's Text Appears When page, with a script that
/// has parameters.
#[test]
#[ignore]
fn dialog_script_parameters() {
    use mg_module::dialog::{Parent, add_node, new_dialog, params_value};
    mg_testkit::gpu::hold();
    let dir = mg_testkit::scratch_dir("screens-dialog-params");
    let mut g = new_dialog();
    add_node(&mut g, Parent::Root, "What do you want made?");
    {
        let link = &mut g.root.list_mut("StartingList").unwrap()[0];
        link.set("Active", mg_gff::Value::resref("cond_weapmod".parse().unwrap()));
        let pairs = [
            ("tag".to_string(), "weficort".to_string()),
            ("apariencia".to_string(), "05".to_string()),
        ];
        link.set("ConditionParams", params_value(&pairs));
    }
    let mut m = mg_module::Module::new();
    let mut info = mg_gff::Gff::new(*b"IFO ");
    info.root.set("Mod_Name", mg_gff::Value::String(b"x".to_vec()));
    m.set_info(&info).unwrap();
    let key = ResKey::parse("smith", ResType::DLG).unwrap();
    m.set_gff(key, &g).unwrap();
    m.set(
        ResKey::parse("cond_weapmod", ResType::NSS).unwrap(),
        b"// TRUE where the weapon is the one named\nint StartingConditional()\n{\n    return TRUE;\n}\n".to_vec(),
    );
    let path = dir.join("d.mod");
    m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.open_module(&path);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Dialog(key)));
    h.run_steps(4);
    h.get_by_label_contains("What do you want made?").click();
    h.run_steps(4);
    shoot(&mut h, &dir, "dialog-script-parameters");
    // (And at a larger size of its own: Options › Conversation Editor.)
    h.state_mut().settings.dialog_font_size = Some(20);
    shoot(&mut h, &dir, "dialog-font-20");
    // (Its right-click menu, at that size too.)
    let at = h.get_by_label_contains("What do you want made?").rect().center();
    h.hover_at(at);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    shoot(&mut h, &dir, "dialog-font-20-menu");
}

/// The Talk Table editor on a line with a long text: plain, and with a
/// feminine table.
#[test]
#[ignore]
fn talk_table_editor() {
    use mg_module::talk::{Line, Table};
    mg_testkit::gpu::hold();
    let dir = mg_testkit::output_dir("screens");
    for feminine in [false, true] {
        let app = Moonglow::new(None, Box::new(NoDialogs::default()));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 760.0))
            .wgpu()
            .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
        let name = if feminine { "long_f" } else { "long" };
        let mut table = Table::create(name, &dir, mg_core::Language::ENGLISH, feminine);
        let long: Vec<String> = (1..=50).map(|n| format!("Line {n} of a long text.")).collect();
        for row in 0..60 {
            let text = if row == 27 { long.join("\n") } else { format!("Text {row}") };
            let line = Line {
                feminine: feminine.then(|| text.clone()),
                text,
                sound: String::new(),
                sound_length: 0.0,
            };
            table.add_line(&line).unwrap();
        }
        h.state_mut().talk = Some(table);
        h.state_mut().talk_view.outside = true;
        h.state_mut().talk_view.selected = Some(27);
        h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::TalkTable));
        // (The list's height settles over a few frames.)
        h.run_steps(8);
        shoot(&mut h, &dir, &format!("talk-table-{name}"));
        // The pointer over a line: the whole row shows it.
        h.hover_at(egui::pos2(520.0, 472.0));
        shoot(&mut h, &dir, &format!("talk-table-{name}-hover"));
    }
}

#[test]
#[ignore]
fn options_window() {
    mg_testkit::gpu::hold();
    use egui_kittest::kittest::Queryable;
    let dir = mg_testkit::scratch_dir("screens-options");
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.settings.script_style.colors[3] = Some([255, 128, 0]);
    app.options = Some(mg_ui::OptionsDraft::from_settings(&app.settings));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 900.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    shoot(&mut h, &dir, "options-folders");
    h.get_by_label("General").click();
    shoot(&mut h, &dir, "options-general");
    h.get_by_label("Script Editor").click();
    shoot(&mut h, &dir, "options-script-editor");
    h.get_by_label("Keyboard").click();
    h.run();
    let draft = h.state_mut().options.as_mut().unwrap();
    draft.keymap.set(
        mg_ui::keys::Cmd::Manual,
        vec![egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S)],
    );
    shoot(&mut h, &dir, "options-keyboard");
}

/// Options › Live, using the same window and GPU guard as the other Options pictures.
#[test]
#[ignore]
fn live_options() {
    mg_testkit::gpu::hold();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-redis/visual");
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.options = Some(mg_ui::OptionsDraft::from_settings(&app.settings));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100., 900.))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.get_all_by_label("Live").last().unwrap().click();
    shoot(&mut h, &dir, "options-live-after");
}

#[test]
#[ignore]
fn live_clients_snapshot() {
    mg_testkit::gpu::hold();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-next/visual");
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.settings.live.clients.push(mg_live::access::ClientToken {
        name: "Story assistant".into(),
        token: "example-credential-for-picture-only-1234".into(),
        groups: vec![mg_live::access::ClientGroup::Read, mg_live::access::ClientGroup::Npc],
        per_minute: 30,
        delivery: Default::default(),
    });
    app.actions.push(mg_ui::Action::LiveOptions);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280., 1000.))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    place(&mut h, &mg_ui::Tab::Options, (120., 40.), (1040., 900.));
    h.run_steps(3);
    let stage = std::env::var("MGLIVE_PICTURE_STAGE").unwrap_or_else(|_| "after".into());
    if !stage.ends_with("before") {
        h.get_by_label("Named clients").click();
    }
    shoot(&mut h, &dir, &format!("clients-{stage}"));
}

#[test]
#[ignore]
fn live_scene_snapshot() {
    mg_testkit::gpu::hold();
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output");
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.open_module(&out.join("ui-area-live-effects/area.mod"));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280., 950.))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let stage = std::env::var("MGLIVE_PICTURE_STAGE").unwrap_or_else(|_| "after".into());
    if !stage.ends_with("before") || stage.starts_with("accept3") || stage.starts_with("accept4") {
        let app = h.state_mut();
        app.live_scene.data_dir = Some(out.join("live-next/scene-picture"));
        let store = Arc::new(mg_live::store::LiveStore::default());
        store.sync(serde_json::json!({"v":1,"boot":"picture","seq":1,"areas":[{"area":"field","full":true,"cre":[{"id":"7","name":"Guard","x":20,"y":20}]}]}).to_string().as_bytes(),mg_live::now_ms()).unwrap();
        app.live = Some(store);
        app.settings.live.enabled = true;
        app.live_scene.draft.name = "Guard greeting".into();
        app.live_scene.draft.steps = vec![
            mg_live::scene::Step { op:"live_npc_say".into(), input:serde_json::json!({"area":"field","target":"7","text":"Za mną! Pilnujcie bramy."}), delay_ms:0, wait:mg_live::scene::Wait::Result { timeout_ms:5000 } }.into(),
            mg_live::scene::Step { op:"live_npc_move".into(), input:serde_json::json!({"area":"field","target":"7","x":25,"y":20,"z":0,"hold":true}), delay_ms:1200, wait:mg_live::scene::Wait::Near { area:"field".into(),target:"7".into(),x:25.,y:20.,radius:1.,timeout_ms:30000 } }.into(),
        ];
        app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Scene));
        h.run_steps(3);
        place(&mut h, &mg_ui::Tab::Scene, (90., 30.), (1100., 880.));
    }
    shoot(&mut h, &out.join("live-next/visual"), &format!("scene-{stage}"));
    if stage == "accept4-after" {
        h.get_by_label("Scene").click();
        h.run_steps(3);
        shoot(&mut h, &out.join("live-next/visual"), "scene-accept4-menu");
        h.key_press(egui::Key::Escape);
        h.state_mut().live_scene.draft.steps.clear();
        h.run_steps(3);
        shoot(&mut h, &out.join("live-next/visual"), "scene-accept4-empty");
    }
}

/// The Live menu, open (E2.7).
#[test]
#[ignore]
fn live_menu() {
    mg_testkit::gpu::hold();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-next/visual");
    std::fs::create_dir_all(&dir).unwrap();
    let app = Moonglow::new(None, Box::new(NoDialogs::default()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100., 500.))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.get_by_label("Live").click();
    shoot(&mut h, &dir, "live-menu");
}

/// The Live Log with the game's events (E3.5).
#[test]
#[ignore]
fn live_log() {
    mg_testkit::gpu::hold();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-next/visual");
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.settings.live.api_port = 0;
    app.settings.live.redis_port = 1;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100., 600.))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.get_by_label("Live").click();
    h.run();
    h.get_by_label("Start Live").click();
    h.run_steps(3);
    let store = h.state().live.as_ref().unwrap().clone();
    let report = serde_json::json!({"v":1,"boot":"log","seq":1,"areas":[{"area":"gate","full":true,
        "cre":[{"id":"7","name":"Gate Guard"},{"id":"9","name":"Bandit"}]}],"events":[
        {"seq":1,"kind":"chat","id":"7","name":"Gate Guard","area":"gate","channel":1,"text":"Zażółć gęślą jaźń - who goes there?"},
        {"seq":2,"kind":"chat","id":"9","name":"Bandit","area":"gate","channel":2,"text":"Your gold!"},
        {"seq":3,"kind":"attack","id":"9","name":"Bandit","area":"gate","target":"7","result":4,"roll":6,"modifier":3},
        {"seq":4,"kind":"attack","id":"7","name":"Gate Guard","area":"gate","target":"9","result":3,"roll":20,"modifier":7},
        {"seq":5,"kind":"damage","id":"7","name":"Gate Guard","area":"gate","target":"9","amount":14},
        {"seq":6,"kind":"death","id":"9","name":"Bandit","area":"gate","killer":"7"},
        {"seq":7,"kind":"spell","id":"7","name":"Gate Guard","area":"gate","target":"7","spell":31}]});
    store.sync(report.to_string().as_bytes(), mg_live::now_ms()).unwrap();
    store.server_log(
        &serde_json::json!({"v":1,"kind":"server_log","stream":"s","seq":1,"line":"[Mon Oct 12 18:30:00] Script error: nw_c2_default9 line 12","script_error":true}),
        mg_live::now_ms(),
    );
    h.get_by_label("Live").click();
    h.run();
    h.get_by_label("Live Log").click();
    shoot(&mut h, &dir, "live-log");
}

/// Real connection refusal, kept as before/after evidence for the WSL hint.
#[test]
#[ignore]
fn live_redis_unreachable() {
    mg_testkit::gpu::hold();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-redis/visual");
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.settings.live.enabled = true;
    app.settings.live.api_port = 0;
    app.settings.live.redis_port = 1;
    app.apply_live(&egui::Context::default());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.live_service.as_ref().unwrap().status()["redis"]["lastError"] == "" {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    app.options = Some(mg_ui::OptionsDraft::from_settings(&app.settings));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100., 900.))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    h.get_all_by_label("Live").last().unwrap().click();
    shoot(&mut h, &dir, "options-redis-after");
    h.get_by_label("Redis unreachable — is WSL running?");
}

#[test]
#[ignore]
fn model_viewer() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens-model");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    for name in ["plc_a01", "c_golemerald"] {
        app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Model(
            ResKey::parse(name, ResType::MDL).unwrap(),
        )));
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    shoot(&mut h, &dir, "model-viewer");
}

#[test]
#[ignore]
fn palette_hover_preview() {
    use egui_kittest::kittest::Queryable;
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-hover");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Palette));
    app.palette.kind = mg_module::palette::BlueprintKind::Placeable;
    app.palette.tiles = false;
    app.palette.filter = "plc_chest1".into();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let chest = h.state().game.as_deref().unwrap().string(mg_core::StrRef(5348)).unwrap();
    let at = h.get_all_by_label(&chest).next().unwrap().rect().center();
    h.hover_at(at);
    h.run_steps(30);
    shoot(&mut h, &dir, "palette-hover");
}

#[test]
#[ignore]
fn blueprint_preview() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens-model");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Model(
        ResKey::parse("nw_halfdra002", ResType::UTC).unwrap(),
    )));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(4);
    shoot(&mut h, &dir, "blueprint-preview");
}

#[test]
#[ignore]
fn palettes() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Palette));
    app.palette.filter = "bear".into();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    shoot(&mut h, &dir, "palettes");
}

#[test]
#[ignore]
fn blueprint_editors() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    for (name, t, pages) in [
        ("nw_waypoint001", ResType::UTW, &["Basic", "Advanced"][..]),
        ("animalcriesday", ResType::UTS, &["Basic", "Positioning", "Advanced"][..]),
        ("x0_trapavg_shuri", ResType::UTT, &["Basic", "Trap", "Advanced", "Visuals"][..]),
        ("nw_mumcleric", ResType::UTC, &["Classes"][..]),
        ("nw_verminbeet", ResType::UTE, &["Basic", "Creature List", "Advanced"][..]),
        ("nw_storebar01", ResType::UTM, &["Basic", "Inventory", "Restrictions"][..]),
        ("x2_storethief001", ResType::UTM, &["Inventory"][..]),
        ("nw_door_ttr_01", ResType::UTD, &["Basic", "Lock", "Area Transition", "Advanced"][..]),
        ("plc_chest1", ResType::UTP, &["Basic", "Scripts", "Advanced", "Visuals"][..]),
        ("nw_wswmls010", ResType::UTI, &["General", "Appearance", "Properties"][..]),
        ("nw_aarcl004", ResType::UTI, &["General", "Appearance"][..]),
        ("nw_maarcl002", ResType::UTI, &["Appearance"][..]),
        (
            "nw_bartender",
            ResType::UTC,
            &["Basic", "Statistics", "Appearance", "Classes", "Skills", "Advanced"][..],
        ),
        (
            "db_tanarukk_do",
            ResType::UTC,
            &["Feats", "Spells", "Special Abilities", "Inventory"][..],
        ),
    ] {
        let mut app = Moonglow::new(
            Some(GameInstall::new(&root, None, "en")),
            Box::new(NoDialogs::default()),
        );
        let game = app.game.as_deref().unwrap();
        let data = game.resman.get_named(name, t).unwrap().into_owned();
        let mut m = mg_module::Module::new();
        m.set(ResKey::parse(name, t).unwrap(), data);
        let path = dir.join(format!("{name}.mod"));
        m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
        app.open_module(&path);
        let key = ResKey::parse(name, t).unwrap();
        app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Blueprint(key)));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 760.0))
            .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
        for page in pages {
            h.state_mut().blueprint_pages.insert((key, mg_edit::GffPath::root()), page);
            let page_name = page.to_lowercase().replace(' ', "-");
            shoot(&mut h, &dir, &format!("{name}-{page_name}"));
        }
    }
}

/// An item's General page at several widths: its two columns side by
/// side only where the first one's fields fit beside the second.
#[test]
#[ignore]
fn item_general_widths() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let (name, t) = ("nw_cloth009", ResType::UTI);
    for width in [820.0, 960.0, 1100.0, 1240.0, 1500.0] {
        let mut app = Moonglow::new(
            Some(GameInstall::new(&root, None, "en")),
            Box::new(NoDialogs::default()),
        );
        let game = app.game.as_deref().unwrap();
        let data = game.resman.get_named(name, t).unwrap().into_owned();
        let mut m = mg_module::Module::new();
        m.set(ResKey::parse(name, t).unwrap(), data);
        let path = dir.join(format!("{name}.mod"));
        m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
        app.open_module(&path);
        let key = ResKey::parse(name, t).unwrap();
        app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Blueprint(key)));
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 420.0))
            .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
        shoot(&mut h, &dir, &format!("item-general-{width}"));
    }
}

/// A weapon's Appearance page: its parts' pictures, at two window sizes.
#[test]
#[ignore]
fn weapon_parts() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let t = ResType::UTI;
    for (name, size) in [
        ("nw_wswgs001", egui::vec2(1300.0, 1500.0)),
        ("nw_wswgs001", egui::vec2(1000.0, 640.0)),
        ("nw_wswdg001", egui::vec2(1000.0, 640.0)),
    ] {
        let mut app = Moonglow::new(
            Some(GameInstall::new(&root, None, "en")),
            Box::new(NoDialogs::default()),
        );
        let game = app.game.as_deref().unwrap();
        let data = game.resman.get_named(name, t).unwrap().into_owned();
        let mut m = mg_module::Module::new();
        m.set(ResKey::parse(name, t).unwrap(), data);
        let path = dir.join(format!("{name}.mod"));
        m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
        app.open_module(&path);
        let key = ResKey::parse(name, t).unwrap();
        app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Blueprint(key)));
        let mut h = Harness::builder()
            .with_size(size)
            .wgpu()
            .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
        h.run_steps(4);
        h.get_by_label("Appearance").click();
        h.run_steps(6);
        shoot(&mut h, &dir, &format!("weapon-parts-{name}-{}", size.y));
    }
}

/// Export Resources: the module kind by kind.
#[test]
#[ignore]
fn export_resources() {
    mg_testkit::gpu::hold();
    let dir = mg_testkit::scratch_dir("screens-export");
    let mut m = mg_module::Module::new();
    let mut info = mg_gff::Gff::new(*b"IFO ");
    info.root.set("Mod_Name", mg_gff::Value::String(b"x".to_vec()));
    m.set_info(&info).unwrap();
    for (name, t) in [
        ("guard", ResType::UTC),
        ("smith", ResType::UTC),
        ("longsword", ResType::UTI),
        ("chest", ResType::UTP),
        ("on_enter", ResType::NSS),
        ("on_enter", ResType::NCS),
        ("spells", ResType::TWODA),
    ] {
        m.set(ResKey::parse(name, t).unwrap(), Vec::new());
    }
    let path = dir.join("e.mod");
    m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.open_module(&path);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    h.state_mut().actions.push(mg_ui::Action::ExportDialog(Vec::new()));
    h.run_steps(4);
    shoot(&mut h, &dir, "export-resources");
}

/// A creature's Appearance list, opened: at the appearance it has, its
/// Filter field above the rows.
#[test]
#[ignore]
fn appearance_list_opened() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    app.open_palette = false;
    let utc = ResKey::parse("m1q1faribeth", ResType::UTC).unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Blueprint(utc)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(4);
    let combo = egui_kittest::kittest::by().role(egui::accesskit::Role::ComboBox);
    // (The lists of the Basic page in order: the appearance's is the one
    // whose value is an appearance's name.)
    let at = h
        .query_all(combo)
        .find(|n| n.accesskit_node().value().is_some_and(|v| v.contains("Aribeth")))
        .map(|n| n.rect().center())
        .expect("the Appearance dropdown");
    h.hover_at(at);
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
    shoot(&mut h, &dir, "appearance-list-opened");
}

/// The model beside the fields, in a window wide enough for both.
#[test]
#[ignore]
fn model_beside_the_fields() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-model-beside");
    for (name, t, page) in [
        ("plc_chest1", ResType::UTP, "Basic"),
        ("nw_door_ttr_01", ResType::UTD, "Basic"),
        ("nw_wswmls010", ResType::UTI, "Appearance"),
    ] {
        let mut app = Moonglow::new(
            Some(GameInstall::new(&root, None, "en")),
            Box::new(NoDialogs::default()),
        );
        let game = app.game.as_deref().unwrap();
        let data = game.resman.get_named(name, t).unwrap().into_owned();
        let mut m = mg_module::Module::new();
        m.set(ResKey::parse(name, t).unwrap(), data);
        let path = dir.join(format!("{name}.mod"));
        m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
        app.open_module(&path);
        let key = ResKey::parse(name, t).unwrap();
        app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Blueprint(key)));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 860.0))
            .wgpu()
            .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
        h.state_mut().blueprint_pages.insert((key, mg_edit::GffPath::root()), page);
        h.run();
        shoot(&mut h, &dir, &format!("{name}-{}", page.to_lowercase()));
    }
}

/// The symbols the interface uses, as the fonts draw them (a square is a
/// glyph the fonts lack).
#[test]
#[ignore]
fn glyphs() {
    mg_testkit::gpu::hold();
    let dir = mg_testkit::scratch_dir("screens-glyphs");
    let app = Moonglow::new(None, Box::new(NoDialogs::default()));
    let mut h = Harness::builder().with_size(egui::vec2(700.0, 200.0)).wgpu().build_ui_state(
        |ui, app: &mut Moonglow| {
            // (The application's fonts.)
            app.ui(ui);
            egui::Window::new("glyphs").fixed_pos([20.0, 60.0]).show(ui.ctx(), |ui| {
                ui.heading("▶ 💡 ◎ ⏵ ⏷ 🗐 📦 ⚙ 🗻 🗑 ↕ 🔁 🏁 🌙 ☁ ⊞ 🍃 🎵 👣 ⛶ ℹ 🎥 🏃");
            });
        },
        app,
    );
    h.run();
    h.run();
    shoot(&mut h, &dir, "glyphs");
}

#[test]
#[ignore]
fn store_chosen_item() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let (name, t) = ("nw_storebar01", ResType::UTM);
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    let game = app.game.as_deref().unwrap();
    let data = game.resman.get_named(name, t).unwrap().into_owned();
    // A magic armor: statistics and properties.
    let armor = game.resman.get_named("nw_maarcl002", ResType::UTI).unwrap();
    let armor = mg_gff::Gff::read(&armor).unwrap().root;
    let armor = game.locstring(armor.locstring("LocalizedName").unwrap()).unwrap();
    let mut m = mg_module::Module::new();
    m.set(ResKey::parse(name, t).unwrap(), data);
    let path = dir.join(format!("{name}-chosen.mod"));
    m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    app.open_module(&path);
    let key = ResKey::parse(name, t).unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Blueprint(key)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 900.0))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.state_mut().blueprint_pages.insert((key, GffPath::root()), "Inventory");
    h.run_steps(3);
    place(&mut h, &mg_ui::Tab::Blueprint(key), (20.0, 60.0), (1150.0, 800.0));
    h.run_steps(3);
    let by_hint = |n: &egui_kittest::kittest::AccessKitNode<'_>| n.placeholder() == Some("Find");
    h.get(egui_kittest::kittest::by().predicate(by_hint)).click();
    h.run_steps(2);
    h.get(egui_kittest::kittest::by().predicate(by_hint)).type_text("nw_maarcl002");
    h.run_steps(3);
    let row = |n: &egui_kittest::kittest::AccessKitNode<'_>| {
        n.role() == egui::accesskit::Role::Button && n.label().as_deref() == Some(armor.as_str())
    };
    h.get(egui_kittest::kittest::by().predicate(row)).click();
    shoot(&mut h, &dir, "store-chosen-item");
    // Added, on the Armor page beside the bar's potions.
    h.get_by_label("Add Item").click();
    shoot(&mut h, &dir, "store-armor-added");
}

#[test]
#[ignore]
fn user_manual() {
    mg_testkit::gpu::hold();
    use egui_kittest::kittest::{NodeT, Queryable};
    let dir = mg_testkit::scratch_dir("screens-manual");
    let app = Moonglow::new(None, Box::new(NoDialogs::default()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Manual));
    shoot(&mut h, &dir, "manual-contents");
    let listed =
        |n: &egui_kittest::Node<'_>| n.accesskit_node().role() == egui::accesskit::Role::Button;
    h.get_all_by_label("Areas").find(|n| listed(n)).unwrap().click();
    shoot(&mut h, &dir, "manual-areas");
    h.get_all_by_label("Coming from Aurora").find(|n| listed(n)).unwrap().click();
    shoot(&mut h, &dir, "manual-coming-from-aurora");
    // (The menu bar has a Plugins too: the chapter is the later one.)
    for (chapter, shot) in [
        ("The main window", "manual-main-window"),
        ("Plugins", "manual-plugins"),
        ("Writing plugins", "manual-writing-plugins"),
        ("Plugin API reference", "manual-plugin-api"),
    ] {
        h.get_all_by_label(chapter).rfind(|n| listed(n)).unwrap().click();
        shoot(&mut h, &dir, shot);
    }
    h.state_mut().about = true;
    shoot(&mut h, &dir, "about");
}

#[test]
#[ignore]
fn script_beside_the_palette_pane() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-script-narrow");
    let mut m = mg_module::Module::new();
    let mut info = mg_gff::Gff::new(*b"IFO ");
    info.root.set("Mod_Name", mg_gff::Value::String(b"x".to_vec()));
    m.set_info(&info).unwrap();
    m.set(ResKey::parse("hello", ResType::NSS).unwrap(), b"void main() { }\n".to_vec());
    let path = dir.join("s.mod");
    m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.open_module(&path);
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 700.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    for tab in [
        mg_ui::Tab::Palette,
        mg_ui::Tab::Manual,
        mg_ui::Tab::Script(ResKey::parse("hello", ResType::NSS).unwrap()),
    ] {
        h.state_mut().actions.push(mg_ui::Action::OpenTab(tab));
        h.run();
    }
    shoot(&mut h, &dir, "script-800");
}

#[test]
#[ignore]
fn script_editor() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-script");
    let mut m = mg_module::Module::new();
    let mut info = mg_gff::Gff::new(*b"IFO ");
    info.root.set("Mod_Name", mg_gff::Value::String(b"x".to_vec()));
    m.set_info(&info).unwrap();
    m.set(ResKey::parse("hello", ResType::NSS).unwrap(), b"void main()\n{\n\n}\n".to_vec());
    let path = dir.join("s.mod");
    m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.open_module(&path);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1700.0, 900.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    let script = mg_ui::Tab::Script(ResKey::parse("hello", ResType::NSS).unwrap());
    h.state_mut().actions.push(mg_ui::Action::OpenTab(script));
    h.state_mut().script_tools.side = mg_ui::script_tools::SideTab::Constants;
    h.state_mut().script_tools.filter = "PROPERTY_".into();
    h.run();
    h.run();
    shoot(&mut h, &dir, "script-editor");
}

/// A selection in the script editor: whole lines (to their line ends),
/// and part of one.
#[test]
#[ignore]
fn script_selection() {
    mg_testkit::gpu::hold();
    let dir = mg_testkit::scratch_dir("screens-script-selection");
    let mut m = mg_module::Module::new();
    let mut info = mg_gff::Gff::new(*b"IFO ");
    info.root.set("Mod_Name", mg_gff::Value::String(b"x".to_vec()));
    m.set_info(&info).unwrap();
    let text = "void main()\n{\n    int nGold = 5;\n    string sName = \"x\";\n\n    return;\n}\n";
    let key = ResKey::parse("hello", ResType::NSS).unwrap();
    m.set(key, text.as_bytes().to_vec());
    let path = dir.join("s.mod");
    m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.open_module(&path);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 420.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Script(key)));
    h.run();
    // From the start of "int nGold" line to the start of the empty line:
    // two whole lines with their line ends.
    let from = text.find("    int").unwrap();
    let to = text.find("\n\n").unwrap() + 1;
    for (name, range) in [("lines", (from, to)), ("part", (from + 4, from + 13))] {
        let id = egui::Id::new(("script", key));
        let mut state = egui::text_edit::TextEditState::load(&h.ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(range.0),
            egui::text::CCursor::new(range.1),
        )));
        state.store(&h.ctx, id);
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.run_steps(3);
        shoot(&mut h, &dir, &format!("script-selection-{name}"));
    }
}

#[test]
#[ignore]
fn creature_wizard() {
    use egui_kittest::kittest::Queryable;
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-creature-wizard");
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.actions.push(mg_ui::Action::NewModule("wiz".into()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.state_mut().creature_wizard = Some(Default::default());
    h.run();
    for page in 0..8 {
        if page == 1 {
            h.get_by_label("Human").click();
            h.run();
        }
        if page == 3 {
            // As it opens: the first portrait chosen.
            shoot(&mut h, &dir, "page-3-opened");
            h.get_by_label("po_hu_m_01_").click();
            h.run();
        }
        if page == 6 {
            h.get_by_label("Tutorial").click();
            h.run();
        }
        shoot(&mut h, &dir, &format!("page-{page}"));
        h.get_by_label("Next >").click();
        h.run();
    }
}

#[test]
#[ignore]
fn references() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-references");
    let path = dir.join("chess.mod");
    std::fs::copy(root.join("data/mod/Neverwinter Chess.mod"), &path).unwrap();
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.open_module(&path);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.state_mut()
        .actions
        .push(mg_ui::Action::FindReferences(ResKey::parse("pawn_b", ResType::UTC).unwrap()));
    h.run();
    shoot(&mut h, &dir, "references");
    h.state_mut()
        .actions
        .push(mg_ui::Action::RenameDialog(ResKey::parse("pawn_b", ResType::UTC).unwrap()));
    h.run();
    shoot(&mut h, &dir, "rename");
}

#[test]
#[ignore]
fn talk_table_and_2da_layers() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-talk");
    let user = dir.join("user");
    std::fs::create_dir_all(user.join("override")).unwrap();
    // A talk table with a feminine table, named by a copy of Chess.
    let mut t = mg_module::talk::Table::create(
        "chess_tlk",
        &user.join("tlk"),
        mg_core::Language::ENGLISH,
        true,
    );
    let lines = [
        ("Knight-Errant", "Knight-Errant", "vs_hello"),
        ("Welcome to the board, <FirstName>.", "Welcome to the board, my lady.", ""),
        ("The pawns advance at dawn.", "The pawns advance at dawn.", ""),
    ];
    for (text, f, sound) in lines {
        let line = mg_module::talk::Line {
            text: text.into(),
            feminine: Some(f.into()),
            sound: sound.into(),
            sound_length: if sound.is_empty() { 0.0 } else { 1.2 },
        };
        t.add_line(&line).unwrap();
    }
    t.save().unwrap();
    let path = dir.join("chess.mod");
    let mut m = mg_module::Module::open(&root.join("data/mod/Neverwinter Chess.mod")).unwrap();
    let mut ifo = m.info().unwrap();
    ifo.root.set("Mod_CustomTlk", mg_gff::Value::String(b"chess_tlk".to_vec()));
    m.set_info(&ifo).unwrap();
    m.save_as(&mg_module::ModuleLocation::Archive(path.clone())).unwrap();
    // A 2DA in override over the game's: a row changed, one added.
    let game = mg_resman::ResMan::for_game(&GameInstall::new(&root, None, "en")).unwrap();
    let key = ResKey::parse("ambientmusic", ResType::TWODA).unwrap();
    let text = String::from_utf8(game.get(&key).unwrap().into_owned()).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let last = lines.iter().rposition(|l| !l.trim().is_empty()).unwrap();
    let n: usize = lines[last].split_whitespace().next().unwrap().parse().unwrap();
    let row1 = lines.iter().position(|l| l.split_whitespace().next() == Some("1")).unwrap();
    let mut cells: Vec<String> = lines[row1].split_whitespace().map(str::to_string).collect();
    cells[2] = "mus_mine".into();
    lines[row1] = cells.join(" ");
    cells[0] = (n + 1).to_string();
    cells[2] = "mus_new".into();
    lines.insert(last + 1, cells.join(" "));
    std::fs::write(user.join("override/ambientmusic.2da"), lines.join("\n")).unwrap();

    let mut app = Moonglow::new(
        Some(GameInstall::new(&root, Some(user), "en")),
        Box::new(NoDialogs::default()),
    );
    app.open_module(&path);
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::TalkTable));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.state_mut().talk_view.selected = Some(1);
    h.run();
    shoot(&mut h, &dir, "talk-table");
    let tab = h.state().dock.find_tab(&mg_ui::Tab::TalkTable).unwrap();
    h.state_mut().dock.remove_tab(tab);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Resource(key)));
    h.run();
    shoot(&mut h, &dir, "2da-layers");
}

#[test]
#[ignore]
fn hak_editor_and_attach() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-hak");
    let user = dir.join("user");
    std::fs::create_dir_all(user.join("hak")).unwrap();
    // A shipped hak, opened in the editor, with a file added.
    let shipped = root.join("data/hk/id_resources.hak");
    let added = dir.join("mg_chair.mdl");
    std::fs::write(&added, b"model").unwrap();
    let path = dir.join("chess.mod");
    std::fs::copy(root.join("data/mod/Neverwinter Chess.mod"), &path).unwrap();
    let tlk = dir.join("mg_text.tlk");
    std::fs::write(&tlk, mg_tlk::Tlk::new(mg_core::Language::ENGLISH).to_bytes().unwrap()).unwrap();
    let dialogs = NoDialogs {
        open: vec![shipped.clone()],
        open_many: vec![vec![shipped, tlk]],
        ..Default::default()
    };
    let mut app = Moonglow::new(Some(GameInstall::new(&root, Some(user), "en")), Box::new(dialogs));
    app.open_module(&path);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    mg_ui::hak_view::open_hak(h.state_mut());
    h.state_mut().haks[0].hak.add_files(&[added]);
    h.run();
    shoot(&mut h, &dir, "hak-editor");
    let tab = h.state().dock.find_tab(&mg_ui::Tab::Hak(h.state().haks[0].id)).unwrap();
    h.state_mut().dock.remove_tab(tab);
    h.state_mut().haks.clear();
    h.run();
    use egui_kittest::kittest::Queryable;
    h.get_by_label("Custom Content").click();
    h.run();
    h.get_by_label("Add Haks and Talk Table…").click();
    h.run();
    shoot(&mut h, &dir, "attach");
}

#[test]
#[ignore]
fn object_walkmeshes() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-walkmesh");
    let path = dir.join("contest.mod");
    std::fs::copy(root.join("data/mod/Contest Of Champions 0492.mod"), &path).unwrap();
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&path);
    // The area with the most placeables and doors.
    let ws = app.ws.as_mut().unwrap();
    let areas = ws.module.areas().unwrap();
    let count = |ws: &mut mg_edit::Workspace, a: mg_core::ResRef| {
        let git = ws.doc(&ResKey::new(a, ResType::GIT)).unwrap();
        ["Placeable List", "Door List"]
            .iter()
            .map(|l| match git.root.get(l) {
                Some(mg_gff::Value::List(v)) => v.len(),
                _ => 0,
            })
            .sum::<usize>()
    };
    let area = *areas.iter().max_by_key(|a| count(ws, **a)).unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    let view = h.state_mut().area_views.get_mut(&area).unwrap();
    view.walkmesh = true;
    view.object_walkmesh = true;
    h.run_steps(5);
    shoot(&mut h, &dir, "object-walkmeshes");
}

#[test]
#[ignore]
fn tileset_minimap_pictures() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("screens-minimaps");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let install = GameInstall::new(&root, None, "en");
    let game = mg_resman::ResMan::for_game(&install).unwrap();
    // A copy of a game tileset, its pictures rendered beside it.
    let name = "tcn01";
    let set = game.get(&ResKey::parse(name, ResType::SET).unwrap()).unwrap().into_owned();
    let set_dir = dir.join("set");
    let _ = std::fs::remove_dir_all(&set_dir);
    std::fs::create_dir_all(&set_dir).unwrap();
    let path = set_dir.join(format!("{name}.set"));
    std::fs::write(&path, &set).unwrap();
    let mut app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    mg_ui::tileset_view::open_path(h.state_mut(), path);
    h.run();
    h.state_mut().tilesets[0].page = mg_ui::tileset_view::Page::Tiles;
    h.state_mut().tilesets[0].tile = Some(3);
    h.run();
    shoot(&mut h, &dir, "tileset-editor");
    let mut doc = h.state_mut().tilesets.remove(0);
    let tiles = doc.tileset().unwrap().tiles.clone();
    mg_ui::tileset_view::render_minimaps(h.state_mut(), &mut doc, 32);
    // A sheet: the game's picture beside Moonglow's, for the first tiles.
    let shown: Vec<_> = tiles.iter().filter(|t| t.image_map_2d.is_some()).take(12).collect();
    let mut sheet = mg_image::Rgba::new(32 * 2 + 8, 40 * shown.len() as u32);
    let mut lit = 0;
    for (row, t) in shown.iter().enumerate() {
        let pic = t.image_map_2d.clone().unwrap().to_lowercase();
        let ours = std::fs::read(set_dir.join(format!("{pic}.tga"))).unwrap();
        let ours = mg_image::read(ResType::TGA, &ours).unwrap().to_rgba();
        lit += usize::from(
            ours.data.as_chunks::<4>().0.iter().any(|p| p[0] > 20 || p[1] > 20 || p[2] > 20),
        );
        let theirs = game
            .texture(mg_core::ResRef::from_str(&pic).unwrap())
            .and_then(|(t, d)| mg_image::read(t, &d).ok())
            .map(|t| t.to_rgba());
        let y0 = sheet.height - 40 * (row as u32 + 1) + 4;
        for (x0, img) in [(0u32, theirs.as_ref()), (40, Some(&ours))] {
            let Some(img) = img else { continue };
            for y in 0..32 {
                for x in 0..32 {
                    let p = img.pixel(x * img.width / 32, y * img.height / 32);
                    let i = (((y0 + y) * sheet.width + x0 + x) * 4) as usize;
                    sheet.data[i..i + 4].copy_from_slice(&p);
                }
            }
        }
    }
    assert!(lit >= shown.len() - 1, "{lit} of {} pictures have anything in them", shown.len());
    // Laid out as the game's pictures: for each tile, Moonglow's picture
    // correlates best with the game's as it is, not flipped or turned.
    let lum = |img: &mg_image::Rgba, flip_x: bool, flip_y: bool| -> Vec<f32> {
        let mut v = Vec::with_capacity(256);
        for y in 0..16 {
            for x in 0..16 {
                let (sx, sy) = (if flip_x { 15 - x } else { x }, if flip_y { 15 - y } else { y });
                let p = img.pixel(sx * img.width / 16, sy * img.height / 16);
                v.push(0.3 * f32::from(p[0]) + 0.59 * f32::from(p[1]) + 0.11 * f32::from(p[2]));
            }
        }
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let sd =
            (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt().max(1e-3);
        v.iter().map(|x| (x - mean) / sd).collect()
    };
    let corr =
        |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>() / a.len() as f32;
    let (mut best_as_is, mut judged) = (0, 0);
    for t in tiles.iter().filter(|t| t.image_map_2d.is_some()).take(60) {
        let pic = t.image_map_2d.clone().unwrap().to_lowercase();
        let Ok(ours) = std::fs::read(set_dir.join(format!("{pic}.tga"))) else { continue };
        let ours = mg_image::read(ResType::TGA, &ours).unwrap().to_rgba();
        let Some(theirs) = game
            .texture(mg_core::ResRef::from_str(&pic).unwrap())
            .and_then(|(t, d)| mg_image::read(t, &d).ok())
            .map(|t| t.to_rgba())
        else {
            continue;
        };
        let reference = lum(&theirs, false, false);
        let scores: Vec<f32> = [(false, false), (false, true), (true, false), (true, true)]
            .iter()
            .map(|&(fx, fy)| corr(&lum(&ours, fx, fy), &reference))
            .collect();
        // Symmetric pictures say nothing about the way up.
        if scores.iter().cloned().fold(f32::MIN, f32::max)
            - scores.iter().cloned().fold(f32::MAX, f32::min)
            < 0.2
        {
            continue;
        }
        judged += 1;
        best_as_is += usize::from(scores[1..].iter().all(|s| scores[0] >= *s));
    }
    println!("{best_as_is} of {judged} pictures match best as they are");
    assert!(judged >= 20 && best_as_is * 10 >= judged * 8, "{best_as_is} of {judged}");
    std::fs::write(dir.join("minimaps.png"), mg_module::minimap::png(&sheet).unwrap()).unwrap();
}

/// Candidate areas for the README: the first outdoor areas of a campaign
/// module, from a three-quarter view. `MG_README_MODULE` picks the module
/// (default Chapter 1).
#[test]
#[ignore]
fn readme_area_candidates() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("readme-candidates");
    let module = std::env::var("MG_README_MODULE").unwrap_or("Chapter1.nwm".into());
    let path = root.join("data/nwm").join(&module);
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&path);
    let areas: Vec<_> = {
        let ws = app.ws.as_mut().unwrap();
        let all = ws.module.areas().unwrap();
        all.into_iter()
            .filter(|a| {
                let are = ws.doc(&ResKey::new(*a, ResType::ARE)).unwrap();
                let interior = are.root.integer("Flags").unwrap_or(0) & 1 != 0;
                !interior
            })
            .take(10)
            .collect()
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 960.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    for area in areas {
        h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
        h.run_steps(5);
        if let Some(view) = h.state_mut().area_views.get_mut(&area)
            && let Some(o) = &mut view.orbit
        {
            o.pitch = 38f32.to_radians();
            o.yaw = -60f32.to_radians();
            o.distance *= 0.55;
        }
        h.run_steps(8);
        shoot(&mut h, &dir, &format!("{area}"));
    }
}

/// The README's screenshots, of the original campaign's first chapter,
/// into `target/test-output/readme/`. They're cropped above the log pane
/// (which shows the install's path) when copied to `docs/images`.
#[test]
#[ignore]
fn area_grid() {
    // The tile grid among buildings and trees: hidden behind them.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let area = mg_core::ResRef::from_str("map_m1q1a").unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.grid = true;
        if let Some(o) = &mut view.orbit {
            o.pitch = 30f32.to_radians();
            o.yaw = -55f32.to_radians();
            o.distance *= 0.3;
            o.target.x -= 8.0;
        }
    }
    h.run_steps(10);
    shoot(&mut h, &dir, "area-grid");
}

#[test]
#[ignore]
fn area_toolbar_narrow() {
    // The area's toolbar in a narrow tab: its rows end at the tab's edge.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let area = mg_core::ResRef::from_str("map_m1q1a").unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    h.run_steps(5);
    h.run_steps(10);
    shoot(&mut h, &dir, "area-toolbar-narrow");
}

#[test]
#[ignore]
fn lighting_off_and_sound_ranges() {
    // The area with the most placed sounds, at night: with the area's
    // lighting, then without it and with the sounds' ranges, the camera
    // raised.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let ws = app.ws.as_mut().unwrap();
    let areas = ws.module.areas().unwrap();
    let sounds = |ws: &mut mg_edit::Workspace, a: mg_core::ResRef| {
        let git = ws.doc(&ResKey::new(a, ResType::GIT)).unwrap();
        git.root.list("SoundList").map_or(0, <[_]>::len)
    };
    let area = *areas.iter().max_by_key(|a| sounds(ws, **a)).unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.night = true;
        if let Some(o) = &mut view.orbit {
            o.pitch = 50f32.to_radians();
            o.distance *= 0.6;
        }
    }
    h.run_steps(5);
    shoot(&mut h, &dir, "lighting-on");
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.lit = false;
        view.sound_ranges = true;
        if let Some(o) = &mut view.orbit {
            o.target.z += 5.0;
        }
    }
    h.run_steps(5);
    shoot(&mut h, &dir, "lighting-off-sound-ranges");
}

#[test]
#[ignore]
fn areas_by_name_and_area_properties() {
    // Options › General › Show areas by name: the tree and the tabs; and
    // Area Properties' Visual and Audio pages (headings, field labels).
    mg_testkit::gpu::hold();
    use egui_kittest::kittest::Queryable;
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    app.settings.area_names = true;
    app.settings.name_resrefs = true;
    app.settings.light_theme = std::env::var_os("MG_LIGHT").is_some();
    let area = mg_core::ResRef::from_str("map_m1q1a").unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::AreaProperties(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    h.get_by_label("Visual").click();
    h.run_steps(3);
    shoot(&mut h, &dir, "area-names-visual");
    h.get_by_label("Audio").click();
    h.run_steps(3);
    shoot(&mut h, &dir, "area-names-audio");
}

#[test]
#[ignore]
fn merchant_markers() {
    // A merchant among what stands around it: its arrow, or the game's
    // sign, is in the scene, hidden by what is in front of it.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let ws = app.ws.as_mut().unwrap();
    let areas = ws.module.areas().unwrap();
    let store = |ws: &mut mg_edit::Workspace, a: mg_core::ResRef| {
        let git = ws.doc(&ResKey::new(a, ResType::GIT)).unwrap();
        let s = git.root.list("StoreList")?.first()?;
        Some(glam::Vec3::new(s.float("XPosition")?, s.float("YPosition")?, s.float("ZPosition")?))
    };
    let (area, at) = areas.iter().find_map(|a| store(ws, *a).map(|p| (*a, p))).unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.lit = false;
        if let Some(o) = &mut view.orbit {
            o.target = at;
            o.pitch = 50f32.to_radians();
            o.yaw = -2.2;
            o.distance = 6.0;
        }
    }
    h.run_steps(5);
    shoot(&mut h, &dir, "merchant-arrow");
    // Options › Area: the game's $ sign.
    h.state_mut().settings.merchant_signs = true;
    h.run_steps(5);
    shoot(&mut h, &dir, "merchant-sign");
}

#[test]
#[ignore]
fn areas_edited_together() {
    // Edit › Edit Areas Together…: the chooser, every area ticked, and the
    // Area Properties of those ticked.
    mg_testkit::gpu::hold();
    use egui_kittest::kittest::Queryable;
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    app.settings.area_names = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    h.get_by_label("Edit").click();
    h.run_steps(2);
    h.get_by_label("Edit Areas Together…").click();
    h.run_steps(2);
    h.get_by_label("Tick Shown").click();
    h.run_steps(2);
    shoot(&mut h, &dir, "areas-chooser");
    h.get_all_by_label_contains(" Together").last().unwrap().click();
    h.run_steps(3);
    h.get_all_by_label("Audio").last().unwrap().click();
    h.run_steps(3);
    shoot(&mut h, &dir, "areas-together-audio");
}

#[test]
#[ignore]
fn raw_fields_with_row_names() {
    // Fields of an area's GIT: its music and sounds by name.
    mg_testkit::gpu::hold();
    use egui_kittest::kittest::Queryable;
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let git = ResKey::parse("map_m1q1a", ResType::GIT).unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Gff(git)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(4);
    h.get_all_by_label_contains(" fields").next().unwrap().click();
    h.run_steps(4);
    shoot(&mut h, &dir, "raw-fields");
}

#[test]
#[ignore]
fn spawn_point_markers() {
    // An encounter's spawn points: the game's marker, its post and the
    // arrow of its facing.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let ws = app.ws.as_mut().unwrap();
    let areas = ws.module.areas().unwrap();
    let spawn = |ws: &mut mg_edit::Workspace, a: mg_core::ResRef| {
        let git = ws.doc(&ResKey::new(a, ResType::GIT)).unwrap();
        let p = git
            .root
            .list("Encounter List")?
            .iter()
            .find_map(|e| e.list("SpawnPointList")?.first())?;
        Some(glam::Vec3::new(p.float("X")?, p.float("Y")?, p.float("Z")?))
    };
    let (area, at) = areas.iter().find_map(|a| spawn(ws, *a).map(|p| (*a, p))).unwrap();
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.lit = false;
        if let Some(o) = &mut view.orbit {
            o.target = at;
            o.pitch = 40f32.to_radians();
            o.distance = 8.0;
        }
    }
    h.run_steps(5);
    shoot(&mut h, &dir, "spawn-points");
}

#[test]
#[ignore]
fn creature_appearance_inline() {
    // A creature's Appearance page with its model in the page.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    app.open_palette = false;
    let utc = ResKey::parse("m1q1faribeth", ResType::UTC).unwrap();
    app.blueprint_pages.insert((utc, mg_edit::GffPath::root()), "Appearance");
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Blueprint(utc)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(4);
    place(&mut h, &mg_ui::Tab::Blueprint(utc), (250.0, 60.0), (800.0, 900.0));
    h.run_steps(20);
    shoot(&mut h, &dir, "creature-appearance");
}

#[test]
#[ignore]
fn crosser_cursor() {
    // The Road brush's quarter under a pointer resting on the ground of a
    // rural area.
    use mg_module::ModuleLocation;
    use mg_module::new::{AreaSpec, add_area, new_module};
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let install = GameInstall::new(&root, None, "en");
    let game = mg_rules::GameData::open(&install).unwrap();
    let mut rng = fastrand::Rng::with_seed(7);
    let mut m = new_module(&game, "Crossers", &mut rng).unwrap();
    let spec = AreaSpec {
        name: "Field".into(),
        tileset: mg_core::ResRef::from_str("ttr01").unwrap(),
        width: 4,
        height: 4,
    };
    let area = add_area(&mut m, &game, &spec, &mut rng).unwrap();
    let path = dir.join("crossers.mod");
    m.save_as(&ModuleLocation::Archive(path.clone())).unwrap();
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&path);
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    h.get_by_label("🗻").click();
    h.run_steps(2);
    // Corners (1, 1) and (1, 2) raised twice: tile (1, 1) slopes up to its
    // west.
    h.get_by_label("↕ Raise/Lower").click();
    h.run_steps(2);
    for corner in [(10.0, 10.0), (10.0, 20.0)] {
        for _ in 0..2 {
            let at = h.state().area_views[&area]
                .screen_pos(glam::Vec3::new(corner.0, corner.1, 0.0))
                .unwrap();
            h.hover_at(at);
            for pressed in [true, false] {
                let (button, modifiers) = (egui::PointerButton::Primary, egui::Modifiers::NONE);
                h.event(egui::Event::PointerButton { pos: at, button, pressed, modifiers });
            }
            h.run_steps(3);
        }
    }
    h.get_by_label("Road").click();
    h.run_steps(2);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        if let Some(o) = &mut view.orbit {
            o.pitch = 40f32.to_radians();
            o.yaw = -60f32.to_radians();
        }
    }
    h.run_steps(3);
    // The pointer over tile (1, 1), near its west edge: the triangle should
    // be under it.
    let at = h.state().area_views[&area].screen_pos(glam::Vec3::new(13.0, 15.0, 2.0)).unwrap();
    h.hover_at(at);
    h.run_steps(3);
    shoot(&mut h, &dir, "crosser-cursor");
}

#[test]
#[ignore]
fn area_fog() {
    // Medieval City 2's thick fog, from a toolset's usual distance.
    use mg_module::ModuleLocation;
    use mg_module::new::{AreaSpec, add_area, new_module, tilesets};
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let install = GameInstall::new(&root, None, "en");
    let game = mg_rules::GameData::open(&install).unwrap();
    let city = tilesets(&game).into_iter().find(|t| t.name == "Medieval City 2").unwrap();
    let mut rng = fastrand::Rng::with_seed(7);
    let mut m = new_module(&game, "Fog", &mut rng).unwrap();
    let spec = AreaSpec { name: "City".into(), tileset: city.resref, width: 16, height: 16 };
    let area = add_area(&mut m, &game, &spec, &mut rng).unwrap();
    let path = dir.join("fog.mod");
    m.save_as(&ModuleLocation::Archive(path.clone())).unwrap();
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&path);
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.fog = true;
        if let Some(o) = &mut view.orbit {
            o.pitch = 25f32.to_radians();
            o.distance = 60.0;
        }
    }
    h.run_steps(5);
    shoot(&mut h, &dir, "area-fog");
}

#[test]
#[ignore]
fn city_tile_selection() {
    // Medieval City 2 builds its ground above its tiles' heights: the
    // selection lies on it, not under it.
    use mg_module::ModuleLocation;
    use mg_module::new::{AreaSpec, add_area, new_module, tilesets};
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let install = GameInstall::new(&root, None, "en");
    let game = mg_rules::GameData::open(&install).unwrap();
    let city = tilesets(&game).into_iter().find(|t| t.name == "Medieval City 2").unwrap();
    let mut rng = fastrand::Rng::with_seed(7);
    let mut m = new_module(&game, "Tiles", &mut rng).unwrap();
    let spec = AreaSpec { name: "City".into(), tileset: city.resref, width: 8, height: 8 };
    let area = add_area(&mut m, &game, &spec, &mut rng).unwrap();
    let path = dir.join("city-tiles.mod");
    m.save_as(&ModuleLocation::Archive(path.clone())).unwrap();
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&path);
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        view.tile_mode = true;
        view.tile_selection = (1..7).flat_map(|x| [(x, 2), (x, 3)]).collect();
        if let Some(o) = &mut view.orbit {
            o.pitch = 12f32.to_radians();
            o.distance = 70.0;
        }
    }
    h.run_steps(5);
    shoot(&mut h, &dir, "city-tile-selection");
}

#[test]
#[ignore]
fn tile_preview() {
    // A barn under the pointer before it is placed.
    use mg_module::ModuleLocation;
    use mg_module::new::{AreaSpec, add_area, new_module};
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let install = GameInstall::new(&root, None, "en");
    let game = mg_rules::GameData::open(&install).unwrap();
    let mut rng = fastrand::Rng::with_seed(7);
    let mut m = new_module(&game, "Preview", &mut rng).unwrap();
    let spec = AreaSpec {
        name: "Field".into(),
        tileset: mg_core::ResRef::from_str("ttr01").unwrap(),
        width: 6,
        height: 6,
    };
    let area = add_area(&mut m, &game, &spec, &mut rng).unwrap();
    let path = dir.join("preview.mod");
    m.save_as(&ModuleLocation::Archive(path.clone())).unwrap();
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&path);
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        if let Some(o) = &mut view.orbit {
            o.pitch = 40f32.to_radians();
            o.yaw = -60f32.to_radians();
            o.distance = 55.0;
        }
    }
    h.get_by_label("Groups").click();
    h.run_steps(2);
    h.get_by_label("Barn 1 2x2").click();
    h.run_steps(2);
    let at = h.state().area_views[&area].screen_pos(glam::Vec3::new(25.0, 25.0, 0.0)).unwrap();
    h.hover_at(at);
    h.run_steps(5);
    shoot(&mut h, &dir, "tile-preview");
}

#[test]
#[ignore]
fn area_properties_advanced() {
    // The shader flags in rows of their own, clear of Variables.
    use mg_module::ModuleLocation;
    use mg_module::new::{AreaSpec, add_area, new_module};
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let install = GameInstall::new(&root, None, "en");
    let game = mg_rules::GameData::open(&install).unwrap();
    let mut rng = fastrand::Rng::with_seed(7);
    let mut m = new_module(&game, "Props", &mut rng).unwrap();
    let spec = AreaSpec {
        name: "Field".into(),
        tileset: mg_core::ResRef::from_str("ttr01").unwrap(),
        width: 4,
        height: 4,
    };
    let area = add_area(&mut m, &game, &spec, &mut rng).unwrap();
    let path = dir.join("props.mod");
    m.save_as(&ModuleLocation::Archive(path.clone())).unwrap();
    let mut app = Moonglow::new(Some(install), Box::new(NoDialogs::default()));
    app.open_module(&path);
    app.open_palette = false;
    let are = ResKey::new(area, ResType::ARE);
    app.blueprint_pages.insert((are, GffPath::root()), "Advanced");
    // (A loading screen with a picture, to see it under the choice.)
    let edit = mg_edit::Edit::SetField {
        key: are,
        path: GffPath::root(),
        label: "LoadScreenID".into(),
        value: Some(mg_gff::Value::Word(2)),
    };
    app.actions.push(mg_ui::Action::Apply(mg_edit::Command::new("Setup", vec![edit])));
    app.actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::AreaProperties(area)));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    place(&mut h, &mg_ui::Tab::AreaProperties(area), (260.0, 70.0), (780.0, 650.0));
    h.run_steps(3);
    shoot(&mut h, &dir, "area-properties-advanced");
}

#[test]
#[ignore]
fn area_ghost() {
    // A placeable chosen in the palette: see-through under the pointer.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let area = mg_core::ResRef::from_str("map_m1q1a").unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        if let Some(o) = &mut view.orbit {
            // Close, on open cobbles, the square behind it. (Under the
            // middle of the view the other area shots have, it stood
            // half in the tower's wall, before a banner, and hardly
            // showed.)
            o.pitch = 45f32.to_radians();
            o.yaw = 125f32.to_radians();
            o.distance = 16.0;
            o.target = glam::Vec3::new(50.3, 37.4, 4.0);
        }
    }
    h.state_mut().palette.selected = Some(ResKey::parse("plc_statue1", ResType::UTP).unwrap());
    h.run_steps(3);
    let rect = h.state().area_views[&area].rect;
    h.event(egui::Event::PointerMoved(rect.center()));
    h.run_steps(5);
    shoot(&mut h, &dir, "area-ghost");
}

#[test]
#[ignore]
fn area_items() {
    // Items lying in an area, selected: a cloak (the game's folded one, in
    // the cloak's colors), a healer's kit, a gem and a potion, each with
    // its box.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let area = mg_core::ResRef::from_str("map_m1q1a").unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    h.run_steps(5);
    let git = ResKey::new(area, ResType::GIT);
    let first = {
        let ws = h.state_mut().ws.as_mut().unwrap();
        ws.doc(&git).unwrap().root.list("List").map_or(0, <[_]>::len)
    };
    let mut edits = Vec::new();
    {
        let app = h.state();
        let game = app.game.as_deref().unwrap();
        let item =
            |r: mg_core::ResRef| mg_module::gff_root(None, game, &ResKey::new(r, ResType::UTI));
        let placing = mg_module::instances::Placing { game, item: &item };
        let blueprint = |name: &str| item(mg_core::ResRef::from_str(name).unwrap()).unwrap();
        let mut cloak = blueprint("nw_it_medkit001");
        cloak.set("BaseItem", mg_gff::Value::Int(80));
        cloak.set("ModelPart1", mg_gff::Value::Byte(1));
        // (Shiny colors: their palettes' alpha, about half, is how much
        // of the environment map shows, as the game draws them.)
        for (field, color) in
            [("Cloth1Color", 56), ("Cloth2Color", 57), ("Leather1Color", 58), ("Leather2Color", 59)]
        {
            cloak.set(field, mg_gff::Value::Byte(color));
        }
        let items = [
            cloak,
            blueprint("nw_it_medkit001"),
            blueprint("nw_it_gem001"),
            blueprint("nw_it_mpotion001"),
        ];
        for (i, uti) in items.iter().enumerate() {
            let at = mg_module::instances::Placement {
                position: [49.0 + i as f32 * 1.2, 37.4, 4.0],
                rotation: 0.0,
            };
            let mut s =
                mg_module::instances::instance(&placing, ResType::UTI, uti, at, &[]).unwrap();
            // (On the cobbles there.)
            s.set("ZPosition", mg_gff::Value::Float(4.0));
            edits.push(mg_edit::Edit::insert_item(git, GffPath::root(), "List", first + i, s));
        }
    }
    h.state_mut().actions.push(mg_ui::Action::Apply(mg_edit::Command::new("Items", edits)));
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        // (By their places in the area's list of items.)
        view.selection = (0..4).map(|i| (mg_area::ObjectKind::Item, first + i)).collect();
        if let Some(o) = &mut view.orbit {
            o.pitch = 40f32.to_radians();
            o.yaw = 125f32.to_radians();
            o.distance = 6.0;
            o.target = glam::Vec3::new(50.8, 37.4, 4.0);
        }
    }
    h.run_steps(5);
    shoot(&mut h, &dir, "area-items");
}

#[test]
#[ignore]
fn area_dropped_cloaks() {
    // Aurora's probe of cloaks lying in an area (captures/items/, made for
    // a builder's report after 1.32.1): the game's cloak, colors 0, the
    // shiny 56 to 59, 61 (all environment map) and a potion, in a row
    // across the middle of a grass area, from above as Aurora's
    // `cloaks-top.png` shows them.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let module = mg_testkit::aurora_capture!("items/zz_cloaks.mod");
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&module);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let area = mg_core::ResRef::from_str("field").unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    h.run_steps(5);
    for (name, pitch, distance) in [("top", 89.0f32, 9.0), ("tilted", 55.0, 9.0)] {
        {
            let view = h.state_mut().area_views.get_mut(&area).unwrap();
            if let Some(o) = &mut view.orbit {
                o.pitch = pitch.to_radians();
                o.yaw = -90f32.to_radians();
                o.distance = distance;
                o.target = glam::Vec3::new(20.5, 20.0, 0.0);
            }
        }
        h.run_steps(5);
        shoot(&mut h, &dir, &format!("area-dropped-cloaks-{name}"));
    }
}

#[test]
#[ignore]
fn area_paint() {
    // The placeable brush: two placeables chosen, Paint on; its circle on
    // the ground and the dab a press would lay, see-through.
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut app =
        Moonglow::new(Some(GameInstall::new(&root, None, "en")), Box::new(NoDialogs::default()));
    app.set_render_state(rs.clone());
    app.open_module(&root.join("data/nwm/Chapter1.nwm"));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    let area = mg_core::ResRef::from_str("map_m1q1a").unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        if let Some(o) = &mut view.orbit {
            o.pitch = 35f32.to_radians();
            o.yaw = -55f32.to_radians();
            o.distance *= 0.25;
            o.target.x -= 8.0;
        }
    }
    {
        let app = h.state_mut();
        app.palette.tiles = false;
        app.palette.kind = mg_module::palette::BlueprintKind::Placeable;
        app.palette.selected = Some(ResKey::parse("plc_tree", ResType::UTP).unwrap());
        app.palette.chosen = vec![ResKey::parse("plc_shrub", ResType::UTP).unwrap()];
        app.paint = true;
        app.settings.paint.radius_cm = 600;
        app.settings.paint.density = 3;
    }
    h.run_steps(3);
    // A stroke across the square, let go; then the pointer beside it.
    let rect = h.state().area_views[&area].rect;
    let (from, to) =
        (rect.center() + egui::vec2(-260.0, 40.0), rect.center() + egui::vec2(-60.0, 120.0));
    let button = |h: &mut Harness<'_, Moonglow>, pos, pressed| {
        let (button, modifiers) = (egui::PointerButton::Primary, egui::Modifiers::NONE);
        h.event(egui::Event::PointerButton { pos, button, pressed, modifiers });
        h.run_steps(1);
    };
    h.event(egui::Event::PointerMoved(from));
    h.run_steps(2);
    button(&mut h, from, true);
    for k in 1..=10 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 10.0)));
        h.run_steps(1);
    }
    button(&mut h, to, false);
    h.event(egui::Event::PointerMoved(rect.center() + egui::vec2(-120.0, -60.0)));
    h.run_steps(5);
    shoot(&mut h, &dir, "area-paint");
    // A row of benches dragged out.
    {
        let app = h.state_mut();
        app.palette.selected = Some(ResKey::parse("plc_bench", ResType::UTP).unwrap());
        app.palette.chosen.clear();
        app.settings.paint.row = true;
        app.settings.paint.gap_cm = 40;
    }
    let (from, to) =
        (rect.center() + egui::vec2(-160.0, 170.0), rect.center() + egui::vec2(100.0, 200.0));
    h.event(egui::Event::PointerMoved(from));
    h.run_steps(2);
    button(&mut h, from, true);
    for k in 1..=10 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 10.0)));
        h.run_steps(1);
    }
    h.run_steps(3);
    // (The row is shot laid: see-through, it hardly shows here.)
    button(&mut h, to, false);
    h.event(egui::Event::PointerMoved(to + egui::vec2(40.0, -40.0)));
    h.run_steps(3);
    shoot(&mut h, &dir, "area-paint-row");
    // A fill of barrels and woodpiles being dragged out, its grid turned
    // 30° by E: its rectangle on the ground, the pieces see-through, each
    // by its own size.
    {
        let app = h.state_mut();
        app.palette.selected = Some(ResKey::parse("plc_barrel", ResType::UTP).unwrap());
        app.palette.chosen = vec![ResKey::parse("plc_woodpile", ResType::UTP).unwrap()];
        (app.settings.paint.row, app.settings.paint.fill) = (false, true);
        app.settings.paint.gap_cm = 20;
    }
    let (from, to) =
        (rect.center() + egui::vec2(-330.0, 60.0), rect.center() + egui::vec2(-170.0, 170.0));
    h.event(egui::Event::PointerMoved(from));
    h.run_steps(2);
    for _ in 0..2 {
        h.key_press(egui::Key::E);
        h.run_steps(1);
    }
    button(&mut h, from, true);
    for k in 1..=10 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 10.0)));
        h.run_steps(1);
    }
    h.run_steps(3);
    shoot(&mut h, &dir, "area-paint-fill");
    button(&mut h, to, false);
    h.run_steps(3);
}

#[test]
#[ignore]
fn readme_screenshots() {
    use egui_kittest::kittest::Queryable;
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::scratch_dir("readme");
    let chapter = root.join("data/nwm/Chapter1.nwm");
    let harness = || {
        let rs = egui_kittest::wgpu::create_render_state(
            egui_kittest::wgpu::default_wgpu_setup(),
            egui_wgpu::RendererOptions::PREDICTABLE,
        );
        let mut app = Moonglow::new(
            Some(GameInstall::new(&root, None, "en")),
            Box::new(NoDialogs::default()),
        );
        app.set_render_state(rs.clone());
        app.open_module(&chapter);
        // (The area's shot opens the palette beside it; the others are
        // windows alone.)
        app.open_palette = false;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 960.0))
            .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
            .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
        h.run_steps(3);
        h
    };
    let key = |name: &str, t: ResType| ResKey::parse(name, t).unwrap();

    // The area view, with the palette beside it.
    let mut h = harness();
    let area = mg_core::ResRef::from_str("map_m1q1a").unwrap();
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Area(area)));
    h.run_steps(3);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Palette));
    h.state_mut().palette.kind = mg_module::palette::BlueprintKind::Placeable;
    h.state_mut().palette.tiles = false;
    h.run_steps(5);
    {
        let view = h.state_mut().area_views.get_mut(&area).unwrap();
        for k in [
            mg_area::ObjectKind::Trigger,
            mg_area::ObjectKind::Encounter,
            mg_area::ObjectKind::Sound,
            mg_area::ObjectKind::Waypoint,
        ] {
            view.show[k.index()] = false;
        }
        view.show_start = false;
        view.grid = false;
        if let Some(o) = &mut view.orbit {
            o.pitch = 34f32.to_radians();
            o.yaw = -55f32.to_radians();
            o.distance *= 0.42;
            o.target.x -= 8.0;
        }
    }
    h.run_steps(10);
    shoot(&mut h, &dir, "area");

    // Aribeth's conversation.
    let mut h = harness();
    let dlg = mg_ui::Tab::Dialog(key("m1q1faribeth", ResType::DLG));
    h.state_mut().actions.push(mg_ui::Action::OpenTab(dlg.clone()));
    h.run_steps(3);
    place(&mut h, &dlg, (250.0, 70.0), (1320.0, 725.0));
    h.run_steps(5);
    shoot(&mut h, &dir, "conversation");

    // Aribeth: her blueprint beside her model.
    let mut h = harness();
    let utc = key("m1q1faribeth", ResType::UTC);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Blueprint(utc)));
    h.run_steps(3);
    h.state_mut().actions.push(mg_ui::Action::OpenTab(mg_ui::Tab::Model(utc)));
    h.run_steps(3);
    for (tab, x) in [(mg_ui::Tab::Blueprint(utc), 250.0), (mg_ui::Tab::Model(utc), 960.0)] {
        let path = h.state().dock.find_tab(&tab).unwrap();
        if let Some(w) = h.state_mut().dock.get_window_state_mut(path.surface) {
            w.set_position(egui::pos2(x, 70.0))
                .set_size(egui::vec2(if x < 500.0 { 700.0 } else { 600.0 }, 725.0));
        }
    }
    h.run_steps(20);
    shoot(&mut h, &dir, "creature");

    // A script.
    let mut h = harness();
    let script = mg_ui::Tab::Script(key("m1q3_formosa_9", ResType::NSS));
    h.state_mut().actions.push(mg_ui::Action::OpenTab(script.clone()));
    h.run_steps(3);
    place(&mut h, &script, (250.0, 70.0), (1320.0, 725.0));
    h.run_steps(5);
    let _ = h.query_by_label("Compile");
    shoot(&mut h, &dir, "script");
}

/// Moves and sizes a tab's window.
fn place(h: &mut Harness<'_, Moonglow>, tab: &mg_ui::Tab, at: (f32, f32), size: (f32, f32)) {
    let path = h.state().dock.find_tab(tab).unwrap();
    if let Some(w) = h.state_mut().dock.get_window_state_mut(path.surface) {
        w.set_position(egui::pos2(at.0, at.1)).set_size(egui::vec2(size.0, size.1));
    }
}

/// The Command Palette over the window, something typed
/// (`target/test-output/screens/command-palette.png`).
#[test]
#[ignore]
fn command_palette() {
    mg_testkit::gpu::hold();
    let dir = mg_testkit::scratch_dir("screens-command-palette");
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    let mut finder = mg_ui::commands::Finder::default();
    finder.filter = "mod".into();
    app.command_palette = Some(finder);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    shoot(&mut h, &dir, "command-palette");
}

/// The Plugins window with the plugin host's fixture plugins installed and
/// one enabled, the console open (`target/test-output/screens-plugins/`).
#[test]
#[ignore]
fn plugins_window() {
    mg_testkit::gpu::hold();
    let dir = mg_testkit::scratch_dir("screens-plugins");
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.plugin_dir =
        Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mg-plugin/tests/fixtures"));
    app.load_plugins();
    app.enable_plugin("example.tag-conventions", true);
    app.plugins.window = true;
    app.plugins.console = "return ctx.module:resources(\"utc\")".into();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 760.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run();
    h.get_by_label("Console").click();
    h.run();
    shoot(&mut h, &dir, "plugins");
    h.state_mut().plugins.window = false;
    h.run();
    h.get_by_label("Plugins").click();
    h.run();
    shoot(&mut h, &dir, "plugins-menu");
    h.key_press(egui::Key::Escape);
    h.run();

    // Install from File: what it said, and the question before it
    // replaces one installed (in a plugins folder of this test's own).
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mg-plugin/tests/fixtures/tag-conventions");
    let plugin = mg_plugin::Plugin::load(&fixture).unwrap();
    let archive = dir.join("tag-conventions-1.0.0.zip");
    std::fs::write(&archive, mg_plugin::pack(&plugin).unwrap()).unwrap();
    h.state_mut().plugin_dir = Some(dir.join("plugins"));
    h.state_mut().load_plugins();
    h.state_mut().install_plugin_from(&archive);
    h.run();
    shoot(&mut h, &dir, "plugins-installed");
    h.state_mut().install_plugin_from(&archive);
    h.run();
    h.run();
    shoot(&mut h, &dir, "plugins-replace");
}

#[path = "support/live_windows.rs"]
mod live_windows;

/// Hexxar windows only: fake Live store, installed game read-only, no service/client.
#[test]
#[ignore]
fn hexxar_windows() {
    mg_testkit::gpu::hold();
    let root = std::path::PathBuf::from(
        std::env::var("MGLIVE_UI_GAME_ROOT").expect("read-only game resource root"),
    );
    let (mut app, store, dir) = live_windows::app("pictures", Some(&root));
    app.live_library.data_dir = Some(dir.join("data"));
    let rs = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    app.set_render_state(rs.clone());
    let feed = store.clone();
    let mut report_seq = 1;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400., 950.))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(
            move |ui, app: &mut Moonglow| {
                report_seq += 1;
                live_windows::report(&feed, report_seq, vec![]);
                app.ui(ui);
            },
            app,
        );
    live_windows::prime(&mut h, mg_ui::Tab::LiveVfx);
    // An actual click in the exposed area records the ground point, while
    // later clicks inside floating windows must not replace it.
    let point = egui::pos2(1200., 650.);
    h.event(egui::Event::PointerMoved(point));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
    }
    let ground = h.state().live_vfx.last_ground.clone().expect("ground click recorded");
    assert_eq!(ground.0, live_windows::area());
    assert_eq!(ground.2, "windows");
    assert!(ground.1.is_finite());
    h.state_mut().area_views.get_mut(&live_windows::area()).unwrap().live.selected =
        Some("7".into());

    h.state_mut().live_vfx.row = 2;
    h.state_mut().live_vfx.search = "ENTANGLE".into();
    shoot(&mut h, &dir, "vfx-models");
    let source = mg_ui::model_view::Source::LiveVfx;
    let view = &h.state().model_views[&source];
    assert!(view.preview().unwrap().parts.iter().any(|p| p.model.starts_with("vdr_entangle")));
    let before = view.time;
    h.run_steps(50);
    assert!(h.state().model_views[&source].time > before);
    shoot(&mut h, &dir, "vfx-animated");
    let fire = h
        .state()
        .live_vfx
        .rows
        .iter()
        .find(|r| r.label == "VFX_DUR_AURA_FIRE")
        .expect("stock fire aura")
        .id;
    h.state_mut().live_vfx.row = fire;
    h.state_mut().live_vfx.search = "AURA_FIRE".into();
    h.run_steps(60);
    assert!(
        h.state().model_views[&source].vfx_particle_vertices() > 0,
        "attached effect particles"
    );
    shoot(&mut h, &dir, "vfx-particles");
    h.state_mut().live_vfx.row = 411;
    h.state_mut().live_vfx.search = "GLOW_RED".into();
    shoot(&mut h, &dir, "vfx-glow");
    h.state_mut().actions.push(mg_ui::Action::CloseTab(mg_ui::Tab::LiveVfx));
    live_windows::prime(&mut h, mg_ui::Tab::PlaceableGallery);
    let appearances = h
        .state()
        .game
        .as_ref()
        .unwrap()
        .choices(
            "appearance",
            mg_rules::ChoiceColumns { name: Some("STRING_REF"), label: Some("LABEL") },
        )
        .unwrap();
    let name = appearances.iter().find(|c| c.row == 6).unwrap().text.clone();
    h.get_by_label("Creatures").click();
    h.run_steps(2);
    h.state_mut().placeable_gallery.as_mut().unwrap().filter = name.clone();
    h.run_steps(3);
    // The existing gallery retains its previous scroll offset when filtered.
    h.event(egui::Event::PointerMoved(egui::pos2(300., 310.)));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0., 20000.),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(30);
    assert!(h.query_by_label(&name).is_some(), "filtered human appearance must be visible");
    h.get_by_label(&name).click();
    shoot(&mut h, &dir, "appearance-confirmation");
    h.get_by_label("Hexxar's guard (7)");
    assert_eq!(h.state().live_vfx.last_ground.as_ref(), Some(&ground));
    h.get_by_label("Cancel").click();
    h.state_mut().actions.push(mg_ui::Action::CloseTab(mg_ui::Tab::PlaceableGallery));
    live_windows::prime(&mut h, mg_ui::Tab::LiveLibrary);
    h.state_mut().live_library.name = "Hexxar guard".into();
    let saved = dir.join("data/live/library/fixture/Hexxar guard.json");
    if !saved.exists() {
        mg_ui::live_library::save(
            &dir.join("data"),
            "fixture",
            "Hexxar guard",
            &live_windows::snapshot(),
        )
        .unwrap();
    }
    h.get_by_label("Refresh library").click();
    h.run_steps(2);
    h.get_by_label("Hexxar guard").click();
    shoot(&mut h, &dir, "library");
    h.state_mut().actions.push(mg_ui::Action::CloseTab(mg_ui::Tab::LiveLibrary));
    live_windows::prime(&mut h, mg_ui::Tab::LiveSkill);
    store.set_policy(mg_live::store::Policy { players: true, ..Default::default() });
    h.state_mut().live_skill.success_text = "You notice fresh tracks by the gate.".into();
    h.state_mut().live_skill.fail_text = "The wind has covered the trail.".into();
    h.state_mut().live_skill.result = Some(
        serde_json::json!({"players":[{"id":"9","passed":true,"nui_token":7},{"id":"a","passed":false,"nui_token":0}],"truncated":false}),
    );
    shoot(&mut h, &dir, "skill-results");
    assert!(h.state().live_service.is_none());
    assert!(!h.state().has_unsaved_work());
}

#[path = "support/live_dm_pictures.rs"]
mod live_dm_pictures;

#[path = "support/live_skill_picture.rs"]
mod live_skill_picture;

#[path = "support/live_inventory_picture.rs"]
mod live_inventory_picture;

#[path = "support/live_appearance_restore_picture.rs"]
mod live_appearance_restore_picture;

#[path = "support/live_vfx_transform_picture.rs"]
mod live_vfx_transform_picture;

#[path = "support/live_session_factions_picture.rs"]
mod live_session_factions_picture;
/// The application with `app`'s NUI window `key` open in a window of
/// `size`, at the top left of the room under the toolbar.
fn nui_harness(app: Moonglow, key: ResKey, size: (f32, f32)) -> Harness<'static, Moonglow> {
    let mut app = app;
    let tab = mg_ui::Tab::Nui(key);
    app.actions.push(mg_ui::Action::OpenTab(tab.clone()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(size.0 + 40.0, size.1 + 110.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut Moonglow| app.ui(ui), app);
    h.run_steps(3);
    place(&mut h, &tab, (20.0, 90.0), size);
    h.run_steps(3);
    h
}

/// The NUI Creator on a window of settings (`nui-editor.png`), the preview
/// in its Hover state at 150% (`nui-hover-150.png`), a list
/// (`nui-list.png`), Interact (`nui-interact.png`, `-narrow`), groups,
/// a chart and draw layers (`nui-controls.png`), Events & variants
/// (`nui-events.png`) and a narrow window (`nui-compact.png`). Pictures of
/// the editor to look at: what the game draws is the client's to say.
#[test]
#[ignore]
fn nui_creator() {
    use mg_nui::{Binding, Settings};
    use serde_json::json;
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = mg_testkit::output_dir("screens");
    let key = ResKey::parse("nui_test", ResType::JUI).unwrap();
    let config_key = ResKey::parse("nui_test", ResType::TXT).unwrap();
    let new = |install: Option<GameInstall>| {
        let mut app = Moonglow::new(install, Box::new(NoDialogs::default()));
        app.ws = Some(mg_edit::Workspace::new(mg_module::Module::new()));
        app.run(mg_ui::Action::NewNui("nui_test".into()));
        app
    };
    let mut sample = new(Some(GameInstall::new(&root, None, "en")));
    let mut window = mg_nui::window();
    window["title"] = "Character settings".into();
    window["geometry"]["w"] = 460.0.into();
    window["geometry"]["h"] = 330.0.into();
    window["root"]["children"][0]["value"] = "Customize your interface".into();
    let mut input = mg_nui::template("textedit");
    input["value"] = json!({"bind":"player_name"});
    let mut check = mg_nui::template("check");
    check["label"] = "Show tooltips".into();
    check["value"] = json!({"bind":"show_tooltips"});
    let mut button = mg_nui::template("button");
    button["label"] = "Save settings".into();
    button["id"] = "save_settings".into();
    let mut combo = mg_nui::template("combo");
    combo["elements"] = json!([["Polski", 0], ["English", 1]]);
    let mut picture = mg_nui::template("image");
    picture["value"] = "nui_button_a".into();
    picture["height"] = 42.0.into();
    window["root"]["children"]
        .as_array_mut()
        .unwrap()
        .splice(1..2, [input, check, combo, mg_nui::template("progress"), picture, button]);
    let mut settings = Settings::default();
    settings.bindings.insert(
        "player_name".into(),
        Binding { value: "Łukasz — Żółw".into(), ..Default::default() },
    );
    settings
        .bindings
        .insert("show_tooltips".into(), Binding { value: true.into(), ..Default::default() });
    let set = |app: &mut Moonglow, window: &serde_json::Value, settings: &Settings| {
        let module = &mut app.ws.as_mut().unwrap().module;
        module.set(key, serde_json::to_vec_pretty(window).unwrap());
        module.set(config_key, settings.bytes());
    };
    set(&mut sample, &window, &settings);
    let tab = mg_ui::Tab::Nui(key);
    let mut h = nui_harness(sample, key, (1350.0, 960.0));
    place(&mut h, &tab, (20.0, 90.0), (1200.0, 850.0));
    h.run_steps(3);
    h.get_by_label("Canvas Button · Save settings").click();
    h.run();
    shoot(&mut h, &dir, "nui-editor");
    h.get_by_label_contains("State: ").click();
    h.run();
    h.get_by_label("Hover").click();
    h.run();
    h.get_by_label("UI scale 100%").click();
    h.run();
    h.get_by_label("150%").click();
    h.run();
    h.get_by_label("Clean view").click();
    h.run();
    shoot(&mut h, &dir, "nui-hover-150");

    // A list and controls that take input.
    let mut list = mg_nui::template("list");
    list["row_count"] = 5.into();
    list["row_height"] = 28.0.into();
    list["row_template"] = json!([
        [{"type":"label","value":{"bind":"items"},"text_halign":1},220.0,true],
        [{"type":"check","label":"Active","value":{"bind":"active"}},110.0,false]
    ]);
    window["geometry"]["h"] = 380.0.into();
    window["root"]["children"] = json!([
        {"type":"label","value":"Inventory settings","height":30.0},
        list,
        mg_nui::template("slider"),
        {"type":"button","label":"Apply","id":"apply","height":30.0}
    ]);
    settings.bindings.insert(
        "items".into(),
        Binding {
            value: json!(["Miecz", "Łuk", "Tarcza", "Zbroja", "Pierścień"]),
            ..Default::default()
        },
    );
    settings.bindings.insert(
        "active".into(),
        Binding { value: json!([true, false, false, true, false]), ..Default::default() },
    );
    set(h.state_mut(), &window, &settings);
    h.get_by_label("UI scale 150%").click();
    h.run();
    h.get_all_by_label("100%").last().unwrap().click();
    h.run();
    h.get_by_label("Clean view").click();
    h.run();
    h.get_by_label("Canvas List").click();
    h.run();
    h.get_by_label("Collapse List").click();
    h.run();
    shoot(&mut h, &dir, "nui-list");
    h.get_by_label("Interact").click();
    h.run();
    h.get_by_label("Canvas Checkbox · Active · row 1").click();
    h.run();
    shoot(&mut h, &dir, "nui-interact");
    place(&mut h, &tab, (20.0, 90.0), (1100.0, 800.0));
    h.run_steps(3);
    h.get_by_label("Canvas window").click();
    h.run();
    shoot(&mut h, &dir, "nui-interact-narrow");

    // A group that shows a variant, a chart, a color picker and draw layers.
    window["title"] = json!("Views, charts and drawing");
    window["geometry"] = json!({"x":-1.0,"y":-1.0,"w":480.0,"h":450.0});
    window["root"]["children"] = json!([
        {"type":"group","id":"view_host","height":70.0,"children":[{"type":"label","value":"Main view · native group"}]},
        {"type":"row","children":[{"type":"button","id":"next","label":"Show details","height":32.0},{"type":"button","id":"mg_close","label":"Close","height":32.0}]},
        {"type":"chart","height":100.0,"value":[{"type":0,"legend":"XP","color":{"r":80,"g":180,"b":240,"a":255},"data":[0.0,2.0,1.0,4.0]}]},
        {"type":"color_picker","height":45.0,"value":{"r":80,"g":180,"b":240,"a":255}},
        {"type":"spacer","height":100.0,"draw_list_scissor":true,"draw_list":[
            {"type":7,"rect":{"x":8.0,"y":8.0,"w":160.0,"h":75.0},"color":{"r":173,"g":142,"b":96,"a":255},"line_thickness":2.0},
            {"type":4,"rect":{"x":18.0,"y":26.0,"w":145.0,"h":40.0},"text":"Draw layer","font":""},
            {"type":0,"points":[195.0,70.0,260.0,20.0,320.0,65.0,395.0,15.0],"color":{"r":80,"g":180,"b":240,"a":255},"line_thickness":3.0}
        ]}
    ]);
    settings.views.insert("details".into(), json!({"type":"label","value":"Details view"}));
    settings.actions = vec![mg_nui::Route {
        event: "click".into(),
        element: "next".into(),
        action: mg_nui::Action::View { group: "view_host".into(), view: "details".into() },
        extra: Default::default(),
    }];
    set(h.state_mut(), &window, &settings);
    place(&mut h, &tab, (20.0, 90.0), (1350.0, 960.0));
    h.run_steps(3);
    h.get_by_label("Canvas Button · Show details").click();
    h.run();
    shoot(&mut h, &dir, "nui-controls");
    h.get_by_label("Events & variants").click();
    h.run();
    shoot(&mut h, &dir, "nui-events");

    // One GPU device at a time: this harness goes before the next is made.
    drop(h);
    let mut compact = nui_harness(new(None), key, (640.0, 720.0));
    compact.get_by_label("Properties").click();
    compact.run();
    shoot(&mut compact, &dir, "nui-compact");
}

/// Properties on a dropdown with a long option, in the Creator's window as
/// it opens and in a narrow one (`nui-inspector-options.png`, `-narrow`):
/// Properties stays beside the canvas, not under it.
#[test]
#[ignore]
fn nui_inspector_options() {
    use serde_json::json;
    mg_testkit::gpu::hold();
    let dir = mg_testkit::output_dir("screens");
    let key = ResKey::parse("options", ResType::JUI).unwrap();
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.ws = Some(mg_edit::Workspace::new(mg_module::Module::new()));
    app.run(mg_ui::Action::NewNui("options".into()));
    let mut window = mg_nui::window();
    window["root"]["children"] = json!([{
        "type":"row","children":[{
            "type":"combo","id":"language","value":0,"height":30.0,
            "elements":[["First",0],["An intentionally long option label that must remain editable inside Properties",2147483647]]
        }]
    }]);
    app.ws.as_mut().unwrap().module.set(key, serde_json::to_vec_pretty(&window).unwrap());
    let mut h = nui_harness(app, key, (1130.0, 760.0));
    h.get_by_label_contains("Canvas Dropdown").click();
    h.run();
    shoot(&mut h, &dir, "nui-inspector-options");
    place(&mut h, &mg_ui::Tab::Nui(key), (20.0, 90.0), (800.0, 760.0));
    shoot(&mut h, &dir, "nui-inspector-options-narrow");
}

/// A module's NUI windows for [`nui_saved_window`] and [`nui_saved_windows`]:
/// the module at NUI_PREVIEW_MODULE, read and never written, and its file
/// as it was.
fn nui_saved_module() -> (std::path::PathBuf, Vec<u8>, Vec<ResKey>) {
    let path =
        std::path::PathBuf::from(std::env::var("NUI_PREVIEW_MODULE").expect("NUI_PREVIEW_MODULE"));
    let before = std::fs::read(&path).unwrap();
    let only = std::env::var("NUI_PREVIEW_RESREF").ok();
    let keys = mg_module::Module::open(&path)
        .unwrap()
        .keys()
        .filter(|k| k.restype == ResType::JUI)
        .filter(|k| only.as_ref().is_none_or(|n| k.resref.to_string() == *n))
        .copied()
        .collect();
    (path, before, keys)
}

/// The application on the module at `path`, held in memory.
fn nui_saved_app(root: &std::path::Path, path: &std::path::Path) -> Moonglow {
    let mut app =
        Moonglow::new(Some(GameInstall::new(root, None, "en")), Box::new(NoDialogs::default()));
    app.ws = Some(mg_edit::Workspace::new(mg_module::Module::open(path).unwrap()));
    app
}

/// The Creator on a window of a module made with it, as it opens: the
/// window NUI_PREVIEW_RESREF of the module NUI_PREVIEW_MODULE, to the file
/// NUI_PREVIEW_PNG. The module is not written.
#[test]
#[ignore]
fn nui_saved_window() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let output = std::env::var("NUI_PREVIEW_PNG").expect("NUI_PREVIEW_PNG");
    let (path, before, keys) = nui_saved_module();
    let [key] = keys[..] else { panic!("NUI_PREVIEW_RESREF names one window of the module") };
    let mut h = nui_harness(nui_saved_app(&root, &path), key, (1600.0, 1040.0));
    h.run_steps(3);
    h.render().expect("render").save(output).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before, "the module is not written");
}

/// Every window of a module (NUI_PREVIEW_MODULE; one with
/// NUI_PREVIEW_RESREF) as the clean preview alone, at 100%, a PNG each in
/// NUI_PREVIEW_DIR: what is put beside the game client's screenshots.
/// NUI_PREVIEW_CLICK="Canvas Dropdown#2" clicks the second such control in
/// Interact first.
#[test]
#[ignore]
fn nui_saved_windows() {
    mg_testkit::gpu::hold();
    let root = mg_testkit::corpus!();
    let dir = std::path::PathBuf::from(std::env::var("NUI_PREVIEW_DIR").expect("NUI_PREVIEW_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let (path, before, keys) = nui_saved_module();
    for key in keys {
        // (One GPU device at a time: a window's harness goes with its turn.)
        let mut h = nui_harness(nui_saved_app(&root, &path), key, (1300.0, 1150.0));
        h.get_by_value("Editor").click();
        h.run();
        h.get_by_label("Preview only").click();
        h.run();
        h.get_by_label("Clean view").click();
        h.run();
        h.get_by_label("100%").click();
        h.run_steps(3);
        if let Ok(click) = std::env::var("NUI_PREVIEW_CLICK") {
            let (label, nth) = click.split_once('#').unwrap_or((&click, "1"));
            h.get_by_label("Interact").click();
            h.run();
            h.get_all_by_label(label).nth(nth.parse::<usize>().unwrap() - 1).unwrap().click();
            h.run_steps(3);
        }
        shoot(&mut h, &dir, &key.resref.to_string());
    }
    assert_eq!(std::fs::read(&path).unwrap(), before, "the module is not written");
}
