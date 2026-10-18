#![allow(dead_code)]
use super::gateway_ui as support;

use egui_kittest::{Harness, kittest::Queryable};
use mg_live_link::LinkService;
use serde_json::json;
use std::sync::Arc;
use support::host::{Host, until};

pub(crate) const AVATAR: &str = "https://example.invalid/scribe.png";
pub(crate) fn avatar_bytes() -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in 0i32..32 {
        for x in 0i32..32 {
            let inside = (x - 16).pow(2) + (y - 16).pow(2) < 225;
            let star = (x - 16).abs() + (y - 16).abs() < 9;
            pixels.extend_from_slice(if star {
                &[230, 245, 255, 255]
            } else if inside {
                &[31, 137, 167, 255]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 32, 32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    bytes
}

pub(crate) struct Credential(pub(crate) String);
impl mg_live_link::credentials::Backend for Credential {
    fn get(
        &self,
        _: &str,
    ) -> Result<Option<mg_live_link::SessionToken>, mg_live_link::credentials::StoreError> {
        Ok(Some(mg_live_link::SessionToken::new(self.0.clone()).unwrap()))
    }
    fn set(
        &self,
        _: &str,
        _: &mg_live_link::SessionToken,
    ) -> Result<(), mg_live_link::credentials::StoreError> {
        Ok(())
    }
    fn delete(&self, _: &str) -> Result<(), mg_live_link::credentials::StoreError> {
        Ok(())
    }
}
pub(crate) async fn link(host: &Host, token: &str) -> LinkService {
    let tokens = mg_live_link::credentials::TokenStore::new(
        Credential(token.into()),
        std::path::Path::new("unused-test-credentials"),
    );
    let link = LinkService::start(host.config(), &tokens, Arc::new(|| {})).unwrap();
    until(|| link.is_connected() && link.store().area("square").is_some()).await;
    link
}

#[derive(Debug)]
struct Auth;
impl mg_gateway::Authenticator for Auth {
    fn authenticate(&self, bearer: &str) -> Option<mg_gateway::Principal> {
        self.principal(match bearer {
            support::host::SECRET => "dm:test",
            "fixture-other" => "dm:other",
            "fixture-admin" => "dm:admin",
            _ => return None,
        })
    }
    fn principal(&self, id: &str) -> Option<mg_gateway::Principal> {
        ["dm:test", "dm:other", "dm:admin"].contains(&id).then(|| mg_gateway::Principal {
            id: id.into(),
            name: "Test".into(),
            groups: if id == "dm:admin" {
                vec!["admin".into(), "read".into(), "read_chat".into()]
            } else {
                [
                    "read",
                    "read_chat",
                    "npc",
                    "speech",
                    "focus",
                    "manage_ai",
                    "allow_llm",
                    "approve_drafts",
                ]
                .map(str::to_owned)
                .to_vec()
            },
        })
    }
}
pub(crate) async fn host(data: &std::path::Path) -> Host {
    let host = Host::with_auth(Arc::new(Auth)).await;
    let data = data.to_owned();
    host.core
        .with_store(move |s| {
            s.assistant_context(data.clone(), data.join("fixture.mod"), "fixture".into()).unwrap()
        })
        .await
        .unwrap();
    support::report(&host, 1, json!([])).await;
    host
}
pub(crate) fn input(h: &mut Harness<'_, mg_ui::Moonglow>, label: &str, text: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    if text.is_empty() {
        h.key_press(egui::Key::Backspace);
    } else {
        h.get_by_label(label).type_text(text);
    }
    h.run_steps(2);
}
