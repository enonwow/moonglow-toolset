#[path = "../../mg-live-link/tests/support/mod.rs"]
mod gateway;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use mg_live_link::SessionToken;
use mg_live_link::credentials::{Backend, StoreError};
use mg_ui::OptionsDraft;

#[derive(Default)]
struct Memory(Mutex<HashMap<String, SessionToken>>);
impl Backend for Memory {
    fn get(&self, id: &str) -> Result<Option<SessionToken>, StoreError> {
        Ok(self.0.lock().unwrap().get(id).cloned())
    }
    fn set(&self, id: &str, token: &SessionToken) -> Result<(), StoreError> {
        self.0.lock().unwrap().insert(id.into(), token.clone());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}

fn pump(h: &mut Harness<'_, OptionsDraft>, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.run_steps(2);
        if h.query_by_label(label).is_some() {
            return;
        }
        assert!(Instant::now() < deadline, "UI never displayed {label}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn live_servers_add_select_remove_and_apply() {
    let memory = Arc::new(Memory::default());
    let directory = tempfile::tempdir().unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(850., 500.)).build_ui_state(
        |ui, draft: &mut OptionsDraft| {
            OptionsDraft::set_live_credential_backend(
                ui.ctx(),
                memory.clone(),
                directory.path().into(),
            );
            draft.live_servers_ui(ui);
        },
        OptionsDraft::from_settings(&mg_ui::Settings::default()),
    );
    pump(&mut h, "Disconnected");
    h.get_by_label("Add server").click();
    h.run_steps(2);
    pump(&mut h, "Disconnected");
    assert_eq!(h.state().live_servers.servers.len(), 2);
    assert_eq!(h.state().live_servers.selected().unwrap().name, "New server");
    h.get_by_label("Local (dev)").click();
    h.run_steps(2);
    pump(&mut h, "Disconnected");
    assert_eq!(h.state().live_servers.selected().unwrap().id, "local-dev");
    h.get_by_label("New server").click();
    h.run_steps(2);
    pump(&mut h, "Disconnected");
    h.get_by_label("Remove server").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.state().live_servers.servers.len() != 1 {
        h.run_steps(2);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        h.state().apply(&mg_ui::Settings::default()).live_servers.selected().unwrap().id,
        "local-dev"
    );
}

#[test]
fn sign_in_opens_browser_redeems_fake_gateway_and_sign_out_erases_session() {
    let fake = gateway::FakeGateway::start();
    let memory = Arc::new(Memory::default());
    let directory = tempfile::tempdir().unwrap();
    let mut draft = OptionsDraft::from_settings(&mg_ui::Settings::default());
    draft.live_servers.servers[0].url = fake.url.clone();
    draft.live_servers.servers[0].cert_pin = Some(fake.pin.clone());
    let mut h = Harness::builder().with_size(egui::vec2(850., 500.)).build_ui_state(
        |ui, draft: &mut OptionsDraft| {
            OptionsDraft::set_live_credential_backend(
                ui.ctx(),
                memory.clone(),
                directory.path().into(),
            );
            draft.live_servers_ui(ui);
        },
        draft,
    );
    pump(&mut h, "Disconnected");
    h.get_by_label("Sign in with Discord").click();
    let deadline = Instant::now() + Duration::from_secs(5);
    let url = loop {
        h.run_steps(1);
        if let Some(url) = h.output().platform_output.commands.iter().find_map(|c| match c {
            egui::OutputCommand::OpenUrl(url) => Some(url.url.clone()),
            _ => None,
        }) {
            break url;
        }
        assert!(Instant::now() < deadline, "no browser URL");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(url.starts_with(&fake.url));
    gateway::callback(&url, false);
    pump(&mut h, "Session saved on this device");
    assert_eq!(memory.get("local-dev").unwrap().unwrap().expose(), "fixture-session");
    assert!(
        !serde_json::to_string(&h.state().apply(&mg_ui::Settings::default()))
            .unwrap()
            .contains("fixture-session")
    );
    h.get_by_label("Sign out").click();
    pump(&mut h, "Not signed in");
    assert!(memory.get("local-dev").unwrap().is_none());
    assert_eq!(
        fake.request.recv_timeout(Duration::from_secs(1)).unwrap()["grant"],
        "fixture-grant"
    );
    fake.finish();
}
