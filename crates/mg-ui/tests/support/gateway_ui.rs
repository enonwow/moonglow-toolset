//! One in-process TLS gateway fixture shared by UI assertions and screen renders.
#![allow(dead_code)]
#[path = "../../../mg-live-link/tests/link_support/mod.rs"]
pub(crate) mod host;

use egui_kittest::{Harness, kittest::Queryable};
use mg_ui::{Action, Moonglow, NoDialogs, OptionsDraft, Tab};
use std::{sync::Arc, time::Duration};

pub(crate) fn app(host: &host::Host, data: &std::path::Path) -> Moonglow {
    let mut app = Moonglow::new(None, Box::new(NoDialogs::default()));
    app.recovery_dir = None;
    app.live_focus.data_dir = Some(data.into());
    app.live_assistant.data_dir = None;
    // Tiny authored area: no game installation, downloaded HAK or real user folder.
    let mut module = mg_module::Module::new();
    let mut info = mg_gff::Gff::new(*b"IFO ");
    let mut entry = mg_gff::Struct::new(6);
    entry.set("Area_Name", mg_gff::Value::ResRef(b"square".to_vec()));
    info.root.set("Mod_Area_list", mg_gff::Value::List(vec![entry]));
    module.set_info(&info).unwrap();
    let mut are = mg_gff::Gff::new(*b"ARE ");
    are.root.set("Width", mg_gff::Value::Int(2));
    are.root.set("Height", mg_gff::Value::Int(2));
    module.set(
        mg_resman::ResKey::parse("square", mg_core::ResType::ARE).unwrap(),
        are.to_bytes().unwrap(),
    );
    module.set(
        mg_resman::ResKey::parse("square", mg_core::ResType::GIT).unwrap(),
        mg_gff::Gff::new(*b"GIT ").to_bytes().unwrap(),
    );
    let path = data.join("fixture.mod");
    std::fs::write(&path, module.to_archive_bytes().unwrap()).unwrap();
    app.game = Some(Arc::new(mg_rules::GameData::new(
        mg_resman::ResMan::new(),
        mg_tlk::Tlk::new(mg_core::Language::ENGLISH),
    )));
    app.open_module(&path);
    app.open_palette = false;
    app.settings.live.enabled = true;
    let id = app.settings.live_servers.add();
    let server = app.settings.live_servers.servers.iter_mut().find(|s| s.id == id).unwrap();
    server.name = "Campaign server".into();
    server.url = host.origin.clone();
    server.cert_pin = Some(host.pin.clone());
    app.actions.push(Action::OpenTab(Tab::LiveChatArea("square".parse().unwrap())));
    app
}

pub(crate) fn draw() -> impl FnMut(&mut egui::Ui, &mut Moonglow) {
    let mut started = false;
    move |ui, app| {
        if !started {
            OptionsDraft::set_live_credential_backend(
                ui.ctx(),
                Arc::new(host::Memory),
                "unused-test-credentials".into(),
            );
            app.apply_live(ui.ctx());
            started = true;
        }
        app.ui(ui);
    }
}

pub(crate) async fn pump(h: &mut Harness<'_, Moonglow>, mut ready: impl FnMut(&Moonglow) -> bool) {
    let result = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            h.run_steps(2);
            if ready(h.state()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "gateway UI deadline: connection={}, focus={:?}, results={}",
        h.state().live_backend().connection_status(),
        h.state().live_focus.notice,
        h.state()
            .live
            .as_ref()
            .map(|s| s.status(mg_live::now_ms())["results"].clone())
            .unwrap_or_default()
    );
}

pub(crate) async fn connected(h: &mut Harness<'_, Moonglow>) {
    pump(h, |app| {
        app.live_backend().connection_status()["state"] == "connected"
            && app.live.as_ref().is_some_and(|s| s.area("square").is_some())
    })
    .await;
}

pub(crate) fn focus(h: &mut Harness<'_, Moonglow>) {
    let store = h.state().live.clone().unwrap();
    // The normal frame has already selected this server's isolated Focus context.
    h.state_mut().live_focus.add(&store, &["a1".into()]).unwrap();
    h.state_mut().actions.push(Action::CloseTab(Tab::LiveChatArea("square".parse().unwrap())));
    h.state_mut().actions.push(Action::OpenTab(Tab::LiveFocus));
    h.run_steps(3);
}

pub(crate) async fn report(host: &host::Host, seq: u64, results: serde_json::Value) {
    host.core.with_store(move |store| store.sync(serde_json::json!({"v":1,"boot":"boot","seq":seq,
        "areas":[{"area":"square","full":true,"cre":[{"id":"a1","tag":"guard","name":"Guard","r":"guard","hp":[5,9],"x":10,"y":10}]}],
        "results":results}).to_string().as_bytes(), mg_live::now_ms()).unwrap()).await.unwrap();
}

pub(crate) async fn presence(host: &host::Host, h: &mut Harness<'_, Moonglow>) {
    // Wait for the host to apply focus.set and deliver its automatic presence.
    // Observing the outgoing request alone races the next host snapshot, which
    // would overwrite the synthetic DM/AI actors used for visual assertions.
    label(h, "Test · DM").await;

    host.core.publish("dm:test".into(), mg_live::gateway::Push::Presence {
        rev: 10,
        actors: vec![serde_json::json!({"id":"dm:guide","kind":"dm","name":"Guide","avatar":null,"owner":null,"area":"square"}),
            serde_json::json!({"id":"ai:scribe","kind":"ai","name":"Scribe","avatar":null,"owner":"dm:guide","area":"square"})],
        focus: std::collections::BTreeMap::from([("a1".into(), vec!["dm:guide".into(), "ai:scribe".into()])]),
    }).await.unwrap();
    label(h, "Guide · DM").await;
}

pub(crate) async fn label(h: &mut Harness<'_, Moonglow>, label: &str) {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            h.run_steps(2);
            if h.query_by_label(label).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("gateway UI label deadline: {label}"));
}
