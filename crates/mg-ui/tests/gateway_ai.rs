#[path = "support/gateway_ui.rs"]
mod gateway_ui;
#[path = "support/gateway_ops.rs"]
mod ops;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use gateway_ui as support;
use mg_live_link::LinkService;
use ops::{host, input};
use std::sync::Arc;
use support::{
    connected,
    host::{reply_async, until},
    pump,
};
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn consent_targets_one_ai_and_leaves_the_other_off() {
    let data = tempfile::tempdir().unwrap();
    let host = host(data.path()).await;
    let reader = ops::link(&host, support::host::SECRET).await;
    for name in ["Scribe", "Guide"] {
        reply_async(reader.ai_create(serde_json::json!({"name":name})).unwrap()).await.unwrap();
    }
    drop(reader);
    let reader = ops::link(&host, "fixture-admin").await;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300., 1000.))
        .build_ui_state(support::draw(), support::app(&host, data.path()));
    connected(&mut h).await;
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("AI agents").click();
    h.run_steps(3);
    support::label(&mut h, "AI name").await;
    let checkbox = h.query_all_by_label("Allow LLM for this AI").next().unwrap();
    assert!(!checkbox.accesskit_node().is_disabled(), "per-AI consent must be enabled");
    checkbox.click();
    h.run_steps(2);
    support::label(&mut h, "Updated on gateway.").await;
    let request = host.frames().into_iter().find(|f| f["op"] == "llm.allow").unwrap();
    let rows = reply_async(reader.ai_list().unwrap()).await.unwrap();
    assert_eq!(rows.as_array().unwrap().iter().filter(|a| a["llm"] == true).count(), 1);
    assert!(
        rows.as_array().unwrap().iter().any(|a| a["id"] == request["a"]["id"] && a["llm"] == true)
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn avatar_validates_previews_updates_presence_and_clears() {
    let data = tempfile::tempdir().unwrap();
    let host = host(data.path()).await;
    let reader = ops::link(&host, support::host::SECRET).await;
    let created = reply_async(
        reader.ai_create(serde_json::json!({"name":"Scribe","groups":["read","focus"]})).unwrap(),
    )
    .await
    .unwrap();
    host.core.with_gateway(|g| g.set_llm_enabled(true)).await.unwrap();
    reply_async(reader.ai_allow_llm(Some(created["ai"]["id"].as_str().unwrap()), true).unwrap())
        .await
        .unwrap();
    let ai = ops::link(&host, created["token"].as_str().unwrap()).await;
    drop(reader);
    let reader = ops::link(&host, "fixture-admin").await;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300., 1000.))
        .build_ui_state(support::draw(), support::app(&host, data.path()));
    h.ctx.include_bytes(ops::AVATAR, ops::avatar_bytes());
    connected(&mut h).await;
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("AI agents").click();
    h.run_steps(3);
    support::label(&mut h, "AI avatar URL").await;
    input(&mut h, "AI avatar URL", "http://example.invalid/portrait.png");
    assert!(h.get_by_label("Save AI").accesskit_node().is_disabled());
    h.get_by_label("Use an HTTPS URL without credentials, or leave it empty.");
    assert!(!host.frames().iter().any(|f| f["op"] == "ai.update"));
    input(&mut h, "AI avatar URL", ops::AVATAR);
    h.get_by_label("Preview avatar").click();
    h.run_steps(2);
    support::label(&mut h, "Avatar preview").await;
    h.get_by_label("Save AI").click();
    h.run_steps(2);
    support::label(&mut h, "Updated on gateway.").await;
    let rows = reply_async(reader.ai_list().unwrap()).await.unwrap();
    assert_eq!(rows[0]["avatar"], ops::AVATAR);
    h.get_by_label("Close window").click();
    h.run_steps(3);
    support::focus(&mut h);
    reply_async(ai.set_focus(&[("square".into(), "a1".into())]).unwrap()).await.unwrap();
    support::label(&mut h, "Avatar for Scribe \u{b7} AI").await;
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("AI agents").click();
    h.run_steps(3);
    support::label(&mut h, "AI avatar URL").await;
    input(&mut h, "AI avatar URL", "");
    h.get_by_label("Save AI").click();
    h.run_steps(2);
    support::label(&mut h, "Updated on gateway.").await;
    let rows = reply_async(reader.ai_list().unwrap()).await.unwrap();
    assert!(rows[0]["avatar"].is_null());
    assert!(h.state().log.entries.iter().all(|(_, s)| !s.contains("mgai_")));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ai_token_is_shown_once_update_and_revoke_round_trip() {
    let data = tempfile::tempdir().unwrap();
    let host = host(data.path()).await;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300., 1000.))
        .build_ui_state(support::draw(), support::app(&host, data.path()));
    connected(&mut h).await;
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("AI agents").click();
    h.run_steps(3);
    support::label(&mut h, "New AI name").await;
    // Wait for initial list before creating; disabled controls do not submit.
    pump(&mut h, |_| host.frames().iter().any(|f| f["op"] == "ai.list")).await;
    support::label(&mut h, "New AI name").await;
    input(&mut h, "New AI name", "Scribe");
    h.get_by_label("Create AI").click();
    h.run_steps(2);
    support::label(&mut h, "Save this token now. It will not be shown again.").await;
    h.get_by_label("Copy AI token").click();
    h.step();
    assert!(
        h.output()
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c,egui::OutputCommand::CopyText(text) if text.starts_with("mgai_")))
    );
    h.get_by_label("Close window").click();
    h.run_steps(3);
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("AI agents").click();
    h.run_steps(3);
    support::label(&mut h, "AI name").await;
    assert!(h.query_by_label("Copy AI token").is_none(), "reopening cannot reveal the token again");
    input(&mut h, "AI name", "Guide");
    h.get_by_label("Save AI").click();
    h.run_steps(2);
    pump(&mut h, |_| {
        host.frames().iter().any(|f| f["op"] == "ai.update" && f["a"]["name"] == "Guide")
    })
    .await;
    support::label(&mut h, "Updated on gateway.").await;
    // Completion and refresh never reveal the credential again.
    assert!(h.query_by_label("Copy AI token").is_none());
    support::label(&mut h, "Revoke AI").await;
    h.get_by_label("Revoke AI").click();
    h.run_steps(2);
    pump(&mut h, |_| host.frames().iter().any(|f| f["op"] == "ai.revoke")).await;
    support::label(&mut h, "Updated on gateway.").await;
    let reader =
        LinkService::start(host.config(), &support::host::tokens(), Arc::new(|| {})).unwrap();
    until(|| reader.is_connected()).await;
    let list = reply_async(reader.ai_list().unwrap()).await.unwrap();
    let items = list.get("items").unwrap_or(&list).as_array().expect("AI list response");
    assert!(items.is_empty());
    assert!(h.state().log.entries.iter().all(|(_, s)| !s.contains("mgai_")));
}
