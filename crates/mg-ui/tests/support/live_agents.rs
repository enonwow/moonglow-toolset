use super::*;

fn picture(h: &mut Harness<'_, Moonglow>, name: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/live-llm-ui")
        .join(format!("{name}.png"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    h.render().unwrap().save(path).unwrap();
}

#[test]
fn live_agents_controls() {
    let Some((mut h, area)) = area_harness("live-effects") else { return };
    review_live_start(
        &mut h,
        area,
        serde_json::json!({"areas":[{"cre":[
            {"id":"a","name":"Watch captain","app":6,"x":20,"y":20}
        ]}]}),
    );
    if std::env::var_os("MGLIVE_LLM_BEFORE").is_some() {
        picture(&mut h, "toolbar-before");
        h.get_by_label("Live").click();
        h.run_steps(2);
        picture(&mut h, "menu-before");
        return;
    }
    assert!(!h.state().settings.live.allow_agents);
    h.get_by_label("No LLM connected");
    let port = h.state().live_service.as_ref().unwrap().config().api_port;
    let cert = h.state().live_service.as_ref().unwrap().certificate().to_vec();
    assert!(ping(h.state()).starts_with("HTTP/1.1 403"));
    h.get_by_label("⚡ LLM").click();
    h.run_steps(2);
    assert!(h.state().settings.live.allow_agents);
    h.get_by_label("No LLM connected");
    picture(&mut h, "toolbar-on-red");
    assert!(ping(h.state()).starts_with("HTTP/1.1 200"));
    h.run_steps(2);
    h.get_by_label("LLM connected");
    picture(&mut h, "toolbar-after");
    h.get_by_label("Live").click();
    h.run_steps(2);
    h.get_by_label("✔ Allow LLM · connected");
    picture(&mut h, "menu-after");
    h.get_by_label("✔ Allow LLM · connected").click();
    h.run_steps(2);
    assert!(!h.state().settings.live.allow_agents);
    assert!(ping(h.state()).starts_with("HTTP/1.1 403"));
    h.get_by_label("No LLM connected");
    h.state_mut().actions.push(mg_ui::Action::LiveOptions);
    h.run_steps(3);
    h.get_by_label("Allow LLM connections (MCP / agent API)").click();
    h.run_steps(2);
    assert!(h.state().settings.live.allow_agents);
    assert!(h.state().options.as_ref().unwrap().live.allow_agents);
    picture(&mut h, "options-after");
    h.get_by_label("Apply").click();
    h.run_steps(2);
    assert_eq!(
        h.state().live_service.as_ref().unwrap().certificate(),
        cert,
        "Apply after an immediate consent change must not restart Live"
    );
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(h.state().settings.live.allow_agents, "Cancel does not undo explicit consent");
    let back: mg_ui::Settings =
        serde_json::from_str(&serde_json::to_string(&h.state().settings).unwrap()).unwrap();
    assert!(back.live.allow_agents, "consent persists in normal settings");
    let service = h.state().live_service.as_ref().unwrap();
    assert_eq!(service.config().api_port, port);
    assert_eq!(service.certificate(), cert, "consent never restarts the API");
    assert!(!h.state().has_unsaved_work());
    h.state_mut().settings.live.enabled = false;
    h.state_mut().apply_live(&egui::Context::default());
    h.run_steps(2);
    assert!(h.get_by_label("⚡ LLM").accesskit_node().is_disabled());
    h.get_by_label("No LLM connected");
    h.get_by_label("Live").click();
    h.run_steps(2);
    assert!(h.get_by_label_contains("Allow LLM · no LLM connected").accesskit_node().is_disabled());
}

fn ping(app: &Moonglow) -> String {
    use std::io::{Read, Write};
    use std::sync::Arc;
    let service = app.live_service.as_ref().unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(service.certificate().to_vec())).unwrap();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let client =
        rustls::ClientConnection::new(Arc::new(config), "moonglow-live".try_into().unwrap())
            .unwrap();
    let socket = std::net::TcpStream::connect(("127.0.0.1", service.config().api_port)).unwrap();
    socket.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
    let mut tls = rustls::StreamOwned::new(client, socket);
    write!(tls, "GET /v1/ping HTTP/1.1\r\nHost: moonglow-live\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n", service.config().token).unwrap();
    tls.flush().unwrap();
    let mut response = String::new();
    tls.read_to_string(&mut response).unwrap();
    response
}
