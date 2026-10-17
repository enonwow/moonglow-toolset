//! Opt-in real service host for the private native soak; never starts NWN.
use mg_live::{
    LiveConfig, LiveService,
    access::{ClientToken, DeliveryMode},
    store::LiveStore,
};
use serde_json::json;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn session_stress_service() {
    let Ok(label) = std::env::var("MGLIVE_STRESS_RUN") else { return };
    assert!(
        !label.is_empty()
            && label.len() < 40
            && label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    );
    assert!(!PathBuf::from(std::env::var_os("NWN_ROOT").expect("required native guard")).exists());
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output/live-stress");
    assert_eq!(fs::read_to_string(root.join(".owner")).unwrap().trim(), "live-stress-v1");
    let out = root.join(label);
    fs::create_dir_all(&out).unwrap();
    let mut auto =
        ClientToken::conversation("stress-auto", mg_live::service::generate_token().unwrap());
    auto.delivery = DeliveryMode::Auto;
    auto.per_minute = 600;
    let mut draft =
        ClientToken::conversation("stress-draft", mg_live::service::generate_token().unwrap());
    draft.per_minute = 600;
    let watch =
        ClientToken::conversation("stress-watch", mg_live::service::generate_token().unwrap());
    let clients = vec![auto, draft, watch];
    let store = Arc::new(LiveStore::default());
    let mut service = LiveService::start(
        LiveConfig {
            enabled: true,
            allow_agents: true,
            api_port: 0,
            redis_port: 6392,
            prefix: "mglive-events".into(),
            keep_wsl: Some(false),
            keep_logs: true,
            log_chat: true,
            clients: clients.clone(),
            ..Default::default()
        },
        store.clone(),
        Arc::new(|| {}),
    )
    .unwrap();
    service.publish_descriptor(&out.join("data")).unwrap();
    fs::write(out.join("certificate.der"), service.certificate()).unwrap();
    let secret = out.join("clients.private.json");
    fs::write(&secret,serde_json::to_vec(&json!({"port":service.config().api_port,"pid":std::process::id(),"default":service.config().token,"clients":clients})).unwrap()).unwrap();
    let start = Instant::now();
    let mut reviewed = 0;
    let mut refused = 0;
    let mut drafts = fs::File::create(out.join("drafts.jsonl")).unwrap();
    while !out.join("stop-service").exists() && start.elapsed() < Duration::from_secs(3900) {
        // The fixture acts as the consenting local DM; approval is never exposed
        // to either agent token or an HTTP operation.
        for draft in store.drafts() {
            writeln!(drafts,"{}",json!({"at_ms":mg_live::now_ms(),"channel":"draft","data":{"command":draft.command,"client":draft.client}})).unwrap();
            let result =
                store.decide_draft(&draft.command.id, Some(draft.text()), mg_live::now_ms());
            writeln!(drafts,"{}",json!({"at_ms":mg_live::now_ms(),"channel":"decision","data":{"id":draft.command.id,"action":"send","text":draft.text(),"ok":result.is_ok(),"native_id":result.as_ref().ok(),"error":result.as_ref().err().map(|e| e.to_string())}})).unwrap();
            if result.is_ok() {
                reviewed += 1
            } else {
                refused += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    fs::write(out.join("service.json"),serde_json::to_vec_pretty(&json!({"seconds":start.elapsed().as_secs_f64(),"approved":reviewed,"approval_refused":refused,"status":service.status()})).unwrap()).unwrap();
    service.stop();
    fs::remove_file(secret).unwrap();
}
