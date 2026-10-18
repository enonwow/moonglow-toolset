#[path = "support/gateway_ui.rs"]
mod gateway_ui;
#[path = "support/gateway_ops.rs"]
mod ops;
use egui_kittest::{Harness, kittest::Queryable};
use gateway_ui as support;
use ops::{Credential, host, input, link};
use serde_json::json;
use std::sync::Arc;
use support::{
    connected,
    host::{reply_async, until},
    pump,
};
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn draft_visibility_is_owner_scoped_admin_sees_all_and_decisions_round_trip() {
    let data = tempfile::tempdir().unwrap();
    let host = host(data.path()).await;
    let owner = link(&host, support::host::SECRET).await;
    let other = link(&host, "fixture-other").await;
    let own_ai = reply_async(owner.ai_create(json!({"name":"Own helper"})).unwrap()).await.unwrap();
    let other_ai =
        reply_async(other.ai_create(json!({"name":"Other helper"})).unwrap()).await.unwrap();
    host.core.with_gateway(|g| g.set_llm_enabled(true)).await.unwrap();
    // Trusted test setup uses the current host's owner-wide consent endpoint.
    reply_async(owner.set_allow_agents(true).unwrap()).await.unwrap();
    reply_async(other.set_allow_agents(true).unwrap()).await.unwrap();
    let own_ai = link(&host, own_ai["token"].as_str().unwrap()).await;
    let other_ai = link(&host, other_ai["token"].as_str().unwrap()).await;
    let command = |text: &str| {
        serde_json::from_value::<mg_live::store::Command>(
            json!({"id":"","area":"square","op":"npc_say","target":"a1","text":text}),
        )
        .unwrap()
    };
    own_ai.enqueue(command("Own proposal")).unwrap();
    other_ai.enqueue(command("Other proposal")).unwrap();
    until(|| host.store.drafts().len() == 2).await;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300., 1000.))
        .build_ui_state(support::draw(), support::app(&host, data.path()));
    connected(&mut h).await;
    support::label(&mut h, "Own proposal").await;
    assert!(h.query_by_label("Other proposal").is_none());
    let admin_data = tempfile::tempdir().unwrap();
    let mut admin = Harness::builder().with_size(egui::vec2(1300., 1000.)).build_ui_state(
        support::draw_with_credentials(Arc::new(Credential("fixture-admin".into()))),
        support::app(&host, admin_data.path()),
    );
    connected(&mut admin).await;
    support::label(&mut admin, "Own proposal").await;
    support::label(&mut admin, "Other proposal").await;
    h.get_by_label("Edit draft").click();
    h.run_steps(3);
    input(&mut h, "Edit proposed message", "DM edited proposal");
    h.get_by_label("Send draft").click();
    h.run_steps(2);
    pump(&mut h, |_| host.store.drafts().len() == 1).await;
    assert!(
        host.store.commands(mg_live::now_ms())["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["text"] == "DM edited proposal")
    );
    pump(&mut admin, |app| app.live_backend().connection_status()["connected"] == true).await;
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    admin.run_steps(3);
    admin.get_by_label("Reject draft").click();
    admin.run_steps(2);
    pump(&mut admin, |_| host.store.drafts().is_empty()).await;
    // Owner can reject their next draft as well; no automatic retry on completion.
    own_ai.enqueue(command("Reject my proposal")).unwrap();
    support::label(&mut h, "Reject my proposal").await;
    h.get_by_label("Reject draft").click();
    h.run_steps(2);
    pump(&mut h, |_| host.store.drafts().is_empty()).await;
}
