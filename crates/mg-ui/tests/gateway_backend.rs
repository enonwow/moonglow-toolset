#[path = "support/gateway_ui.rs"]
mod support;

use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use mg_ui::{Action, Tab};
use serde_json::json;
use support::{connected, focus, host::Host, presence, pump};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_gui_mirror_command_presence_reconnect_and_unknown() {
    let host = Host::new().await;
    support::report(&host, 1, json!([])).await;
    let data = tempfile::tempdir().unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200., 850.))
        .build_ui_state(support::draw(), support::app(&host, data.path()));
    connected(&mut h).await;
    assert!(h.state().live.as_ref().unwrap().is_mirror());
    let mut mirror = h.state().live.as_ref().unwrap().area("square").unwrap();
    let mut authoritative = host.store.area("square").unwrap();
    mirror.received_ms = 0;
    authoritative.received_ms = 0;
    assert_eq!(mirror, authoritative);
    focus(&mut h);
    pump(&mut h, |_| {
        host.frames().iter().any(|f| {
            f["op"] == "focus.set" && f["a"]["objects"].as_array().is_some_and(|a| !a.is_empty())
        })
    })
    .await;
    presence(&host, &mut h).await;
    h.get_by_label("Guide · DM");
    h.get_by_label("Scribe · AI");
    assert!(!h.get_by_label("NPC card").accesskit_node().is_disabled());
    assert!(h.get_by_label("Guide · DM").rect().height() >= 11.);
    h.state_mut().live_focus.text = "A test greeting".into();
    h.run_steps(2);
    assert!(!h.get_by_label("Send reply").accesskit_node().is_disabled());
    h.get_by_label("Send reply").click();
    h.run_steps(3);
    assert!(
        h.state().live_focus.text.is_empty(),
        "reply click must submit the composer: {:?}",
        h.state().live_focus.notice
    );
    pump(&mut h, |_| {
        host.store.commands(mg_live::now_ms())["commands"].as_array().is_some_and(|c| !c.is_empty())
    })
    .await;
    let command = host.store.commands(mg_live::now_ms())["commands"][0].clone();
    assert_eq!(command["op"], "npc_say");
    let id = command["id"].as_str().unwrap().to_owned();
    let frame = host.frames().into_iter().find(|f| f["op"] == "cmds").unwrap();
    assert_eq!(frame["a"]["commands"][0]["boot"], "boot");
    assert!(frame["a"]["commands"][0]["ttl_ms"].as_u64().is_some_and(|n| n > 0));
    support::report(&host, 2, json!([{"id":id,"ok":true}])).await;
    pump(&mut h, |app| app.live.as_ref().unwrap().result(&id).is_some_and(|r| r.ok)).await;

    host.drop_sockets();
    pump(&mut h, |app| app.live_backend().connection_status()["state"] == "reconnecting").await;
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("Gateway: reconnecting");
    h.key_press(egui::Key::Escape);
    connected(&mut h).await;
    // A genuine unknown result stays visible even after the per-frame command log drains.
    h.state_mut().live_focus.text = "A second greeting".into();
    h.run_steps(2);
    h.get_by_label("Send reply").click();
    pump(&mut h, |_| {
        host.store.commands(mg_live::now_ms())["commands"]
            .as_array()
            .is_some_and(|c| c.iter().any(|v| v["id"] != id))
    })
    .await;
    let next = host.store.commands(mg_live::now_ms())["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] != id)
        .unwrap()["id"]
        .clone();
    support::report(
        &host,
        3,
        json!([{"id":next,"ok":false,"code":"unknown","err":"Outcome unknown"}]),
    )
    .await;
    pump(&mut h, |app| app.live.as_ref().unwrap().result(next.as_str().unwrap()).is_some()).await;
    h.get_by_label("Live").click();
    h.run_steps(3);
    h.get_by_label("Unknown results: 1 — commands may have run; inspect before retrying.");
    h.key_press(egui::Key::Escape);
    h.state_mut().actions.push(Action::OpenTab(Tab::LiveGmCard));
    h.run_steps(3);
    h.get_by_label("Shared with authorized DMs on this server. Changes are checked against the saved revision.");
}
