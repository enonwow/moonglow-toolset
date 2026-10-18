#[path = "support/gateway_ui.rs"]
mod gateway_ui;
#[path = "support/gateway_ops.rs"]
mod ops;
use egui_kittest::{Harness, kittest::Queryable};
use gateway_ui as support;
use mg_live_link::BookKind;
use mg_ui::{Action, Tab};
use ops::{host, input, link};
use serde_json::json;
use support::{connected, host::reply_async, pump};
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gateway_book_conflict_preserves_mine_and_refreshes_invalidations() {
    let data = tempfile::tempdir().unwrap();
    let host = host(data.path()).await;
    let writer = link(&host, "fixture-other").await;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1300., 1000.))
        .build_ui_state(support::draw(), support::app(&host, data.path()));
    connected(&mut h).await;
    h.state_mut().area_views.get_mut(&"square".parse().unwrap()).unwrap().live.on = true;
    h.state_mut().actions.push(Action::CloseTab(Tab::LiveChatArea("square".parse().unwrap())));
    h.run_steps(3);
    h.state_mut().actions.push(Action::OpenTab(Tab::LiveGmCard));
    h.run_steps(3);
    support::label(&mut h, "Campaign tone").await;
    input(&mut h, "Campaign tone", "My version");
    let current = reply_async(writer.gm_card().unwrap()).await.unwrap();
    let mut theirs = current["card"].clone();
    theirs["campaign_tone"] = json!("Their version");
    reply_async(
        writer.book_put(BookKind::Gm, "", theirs, current["rev"].as_u64().unwrap()).unwrap(),
    )
    .await
    .unwrap();
    support::label(&mut h, "Take newer").await;
    h.get_by_label("Your version");
    h.get_by_label("Newer saved version");
    let rows: Vec<_> = h.get_all_by_label("Language").map(|n| n.rect().top()).collect();
    assert_eq!(rows.len(), 2, "both versions expose their read-only/editable fields");
    assert!((rows[0] - rows[1]).abs() < 1., "corresponding fields must align for comparison");
    assert!(h.get_all_by_value("My version").next().is_some());
    h.get_by_label("Keep mine").click();
    h.run_steps(2);
    pump(&mut h, |_| host.store.gm_card().unwrap().campaign_tone == "My version").await;
    support::label(&mut h, "Campaign tone").await;
    input(&mut h, "Campaign tone", "Another local edit");
    let current = reply_async(writer.gm_card().unwrap()).await.unwrap();
    let mut theirs = current["card"].clone();
    theirs["campaign_tone"] = json!("Newest version");
    // A concurrent writer advances the revision. Invalidation may reach the GUI
    // before its save click; both paths must preserve the newer value.
    host.store.save_gm_card(serde_json::from_value(theirs).unwrap()).unwrap();
    assert_eq!(
        reply_async(
            writer
                .book_put(
                    BookKind::Gm,
                    "",
                    current["card"].clone(),
                    current["rev"].as_u64().unwrap()
                )
                .unwrap()
        )
        .await,
        Err(mg_live_link::LinkError::Conflict),
    );
    h.get_by_label("Save card").click();
    h.run_steps(2);
    support::label(&mut h, "Take newer").await;
    h.get_by_label("Take newer").click();
    h.run_steps(3);
    assert!(h.get_all_by_value("Newest version").next().is_some());
    assert_eq!(host.store.gm_card().unwrap().campaign_tone, "Newest version");
    h.state_mut().actions.push(Action::OpenTab(Tab::LiveNpcCard("a1".into())));
    h.run_steps(3);
    support::label(&mut h, "Who they are").await;
    input(&mut h, "Who they are", "A patient guard");
    h.get_by_label("Save card").click();
    h.run_steps(2);
    pump(&mut h, |_| host.store.npc_card("a1").unwrap().who == "A patient guard").await;
    assert!(host.frames().iter().any(|f| f["op"] == "book.put" && f["a"]["key"] == "tag:guard"));
    support::focus(&mut h);
    h.get_by_label("DM note").click();
    h.run_steps(3);
    support::label(&mut h, "Note text").await;
    input(&mut h, "Note text", "Check the north gate");
    h.get_by_label("Save note").click();
    h.run_steps(2);
    pump(&mut h, |_| host.store.object_note("a1").unwrap() == "Check the north gate").await;
    let note = host
        .frames()
        .into_iter()
        .find(|f| f["op"] == "book.put" && f["a"]["kind"] == "note")
        .unwrap();
    assert_eq!(note["a"]["if_rev"], 0);
    assert_eq!(note["a"]["key"], host.store.object_note_key("a1").unwrap());
}
