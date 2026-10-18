//! Revisioned gateway books. Unsaved edits survive invalidations and CAS conflicts.
use crate::Moonglow;
use mg_live_link::{BookKind, LinkError, LinkService, PendingCall};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum Target {
    Gm,
    Npc(String),
    Note(String),
}
#[derive(Clone)]
struct Version {
    key: String,
    value: Value,
    rev: u64,
}
enum Pending {
    Read(PendingCall),
    Save(PendingCall),
}
#[derive(Default)]
struct Editor {
    base: Option<Version>,
    newer: Option<Version>,
    mine: Value,
    dirty: bool,
    seen: Option<(u64, u64)>,
    pending: Option<Pending>,
    notice: Option<String>,
}
#[derive(Default)]
pub(crate) struct Books {
    source: std::sync::Weak<mg_live::store::LiveStore>,
    editors: HashMap<Target, Editor>,
}
impl Target {
    fn read(&self, link: &LinkService) -> Result<PendingCall, LinkError> {
        match self {
            Self::Gm => link.gm_card(),
            Self::Npc(id) => link.npc_card(id),
            Self::Note(id) => link.object_note(id),
        }
    }
    fn version(&self, value: Value) -> Result<Version, LinkError> {
        if matches!(self, Self::Note(_)) && !value["notes"].is_object() {
            return Err(LinkError::Protocol);
        }
        let (key, card, rev) = match self {
            Self::Gm => ("".into(), value["card"].clone(), value["rev"].as_u64()),
            Self::Npc(_) => (
                value["card"]["id"].as_str().ok_or(LinkError::Protocol)?.into(),
                value["card"].clone(),
                value["rev"].as_u64(),
            ),
            Self::Note(id) => (
                value["keys"][id].as_str().ok_or(LinkError::Protocol)?.into(),
                value["notes"].get(id).cloned().unwrap_or_else(|| json!("")),
                value["revs"][id].as_u64(),
            ),
        };
        if (matches!(self, Self::Note(_)) && !card.is_string())
            || (!matches!(self, Self::Note(_)) && !card.is_object())
        {
            return Err(LinkError::Protocol);
        }
        Ok(Version { key, value: card, rev: rev.ok_or(LinkError::Protocol)? })
    }
    fn kind(&self) -> BookKind {
        match self {
            Self::Gm => BookKind::Gm,
            Self::Npc(_) => BookKind::Npc,
            Self::Note(_) => BookKind::Note,
        }
    }
}
impl Editor {
    fn accept(&mut self, incoming: Version) {
        if self.dirty
            && self.base.as_ref().is_some_and(|b| b.rev != incoming.rev || b.key != incoming.key)
        {
            self.newer = Some(incoming);
        } else if !self.dirty {
            self.mine = incoming.value.clone();
            self.base = Some(incoming);
            self.newer = None;
        }
    }
    fn poll(&mut self, target: &Target, link: &LinkService) {
        if let Some(pending) = &self.pending {
            let (call, saving) = match pending {
                Pending::Read(c) => (c, false),
                Pending::Save(c) => (c, true),
            };
            match call.try_recv() {
                Ok(None) => return,
                Ok(Some(value)) => {
                    self.pending = None;
                    if saving {
                        if let Some(rev) = value["rev"].as_u64() {
                            if let Some(base) = &mut self.base {
                                if let Some(newer) = &self.newer {
                                    base.key = newer.key.clone();
                                }
                                base.rev = rev;
                                base.value = self.mine.clone();
                            }
                            self.dirty = false;
                            self.newer = None;
                            self.notice = Some("Saved on gateway.".into());
                        } else {
                            self.notice = Some(
                                "Invalid gateway response; reload to verify the saved version."
                                    .into(),
                            );
                        }
                    } else {
                        match target.version(value) {
                            Ok(version) => self.accept(version),
                            Err(error) => self.notice = Some(error.to_string()),
                        }
                    }
                }
                Err(error) => {
                    self.pending = None;
                    self.notice = Some(error.to_string());
                    if error == LinkError::Conflict {
                        self.seen = None;
                    }
                }
            }
        }
        let generation = (link.book_revision(), link.snapshot().connections);
        if self.pending.is_none() && self.seen != Some(generation) {
            self.seen = Some(generation);
            match target.read(link) {
                Ok(call) => self.pending = Some(Pending::Read(call)),
                Err(error) => self.notice = Some(error.to_string()),
            }
        }
    }
    fn save(&mut self, target: &Target, link: &LinkService, newer: bool) {
        let Some(base) = (if newer { self.newer.as_ref() } else { self.base.as_ref() }) else {
            return;
        };
        match link.book_put(target.kind(), &base.key, self.mine.clone(), base.rev) {
            Ok(call) => self.pending = Some(Pending::Save(call)),
            Err(error) => self.notice = Some(error.to_string()),
        }
    }
}

pub(crate) fn ui(app: &mut Moonglow, ui: &mut egui::Ui, target: Target) {
    let mut books = std::mem::take(&mut app.live_books);
    let backend = app.live_backend();
    let Some(link) = backend.link() else {
        ui.weak("Connect to the gateway to edit shared books.");
        app.live_books = books;
        return;
    };
    let store = link.store();
    if !books.source.upgrade().is_some_and(|old| std::sync::Arc::ptr_eq(&old, &store)) {
        books.editors.clear();
        books.source = std::sync::Arc::downgrade(&store);
    }
    let editor = books.editors.entry(target.clone()).or_default();
    editor.poll(&target, link);
    ui.heading(match target {
        Target::Gm => "GM card",
        Target::Npc(_) => "NPC card",
        Target::Note(_) => "DM note",
    });
    ui.weak("Shared with authorized DMs on this server. Changes are checked against the saved revision.");
    if let Some(message) = &editor.notice {
        ui.label(message);
        if editor.pending.is_none() && ui.button("Reload to verify").clicked() {
            editor.seen = None;
        }
    }
    let ready = editor.pending.is_none() && editor.base.is_some();
    if editor.pending.is_some() {
        ui.spinner();
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
    egui::ScrollArea::vertical().id_salt(("gateway-book", &target)).show(ui, |ui| {
        ui.add_enabled_ui(ready, |ui| {
            if let Some(newer) = editor.newer.clone() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "A newer version was saved. Choose which version to keep.",
                );
                ui.columns(2, |columns| {
                    columns[0].strong("Your version");
                    fields(&mut columns[0], &target, &mut editor.mine, true);
                    columns[1].strong("Newer saved version");
                    fields(&mut columns[1], &target, &mut newer.value.clone(), false);
                });
                ui.horizontal(|ui| {
                    if ui.button("Keep mine").clicked() {
                        editor.save(&target, link, true);
                    }
                    if ui.button("Take newer").clicked() {
                        editor.dirty = false;
                        editor.accept(newer);
                        editor.notice = None;
                    }
                });
            } else if editor.base.is_some() {
                editor.dirty |= fields(ui, &target, &mut editor.mine, true);
                ui.horizontal(|ui| {
                    let save =
                        if matches!(target, Target::Note(_)) { "Save note" } else { "Save card" };
                    if ui.add_enabled(editor.dirty, egui::Button::new(save)).clicked() {
                        editor.save(&target, link, false);
                    }
                    if ui.button("Reload saved version").clicked() {
                        editor.dirty = false;
                        editor.seen = None;
                        editor.notice = None;
                    }
                    if editor.dirty {
                        ui.weak("Unsaved changes");
                    }
                });
            }
        });
    });
    app.live_books = books;
}
fn fields(ui: &mut egui::Ui, target: &Target, value: &mut Value, editable: bool) -> bool {
    let fields: &[(&str, &str)] = match target {
        Target::Gm => &[
            ("campaign_tone", "Campaign tone"),
            ("house_rules", "House rules"),
            ("never_reveal", "Never reveal to players"),
            ("dm_replies", "How to answer on the DM channel"),
            ("narration", "Narration style"),
            ("language", "Language"),
        ],
        Target::Npc(_) => &[
            ("name", "Name"),
            ("who", "Who they are"),
            ("personality", "Personality"),
            ("knows", "What they know"),
            ("hides", "What they hide"),
            ("speech", "How they speak"),
            ("language", "Language"),
        ],
        Target::Note(_) => &[("", "Note text")],
    };
    let mut changed = false;
    for (key, label) in fields {
        let field = if key.is_empty() { &mut *value } else { &mut value[*key] };
        let mut text = field.as_str().unwrap_or_default().to_owned();
        let label = ui.label(*label);
        if editable {
            if ui
                .add(
                    egui::TextEdit::multiline(&mut text)
                        .char_limit(4000)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(label.id)
                .changed()
            {
                *field = json!(text);
                changed = true;
            }
        } else {
            ui.label(text);
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_note_still_uses_the_authoritative_key_and_revision() {
        let version = Target::Note("object".into())
            .version(json!({
                "notes":{}, "keys":{"object":"creature:tag:guard"}, "revs":{"object":0}
            }))
            .unwrap();
        assert_eq!(version.value, "");
        assert_eq!(version.rev, 0);
        assert_eq!(version.key, "creature:tag:guard");
        assert!(
            Target::Note("missing".into())
                .version(json!({"notes":{},"keys":{},"revs":{}}))
                .is_err()
        );
        assert!(
            Target::Note("object".into())
                .version(json!({
                    "notes":null,"keys":{"object":"key"},"revs":{"object":0}
                }))
                .is_err()
        );
    }
    #[test]
    fn invalidation_preserves_dirty_version_and_uses_new_revision_for_resolution() {
        let mut editor = Editor::default();
        editor.accept(Version { key: "".into(), value: json!({"campaign_tone":"Old"}), rev: 1 });
        editor.mine["campaign_tone"] = json!("Mine");
        editor.dirty = true;
        editor.accept(Version { key: "".into(), value: json!({"campaign_tone":"Theirs"}), rev: 2 });
        assert_eq!(editor.mine["campaign_tone"], "Mine");
        assert_eq!(editor.base.as_ref().unwrap().rev, 1);
        assert_eq!(editor.newer.as_ref().unwrap().rev, 2);
        editor.dirty = false;
        editor.accept(editor.newer.clone().unwrap());
        assert_eq!(editor.mine["campaign_tone"], "Theirs");
        assert!(editor.newer.is_none());
    }
}
