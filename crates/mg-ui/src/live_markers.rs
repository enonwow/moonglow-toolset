//! Per-view readability controls affect overlays, never live models or the report.
use crate::live::LiveObject;
use mg_area::ObjectKind;

#[derive(Debug, Default)]
pub struct Markers {
    pub hidden: std::collections::BTreeSet<String>,
}
impl Markers {
    pub fn shows(&self, category: &str) -> bool {
        !self.hidden.contains(category)
    }
    pub fn object(&self, kind: ObjectKind, o: &LiveObject) -> bool {
        let category = match kind {
            ObjectKind::Creature if o.dm || o.possessed => "DM / possessed",
            ObjectKind::Creature if o.pc => "Players",
            ObjectKind::Creature => "NPCs",
            ObjectKind::Placeable => "Placeables",
            ObjectKind::Door => "Doors",
            ObjectKind::Item => "Items",
            _ => return true,
        };
        self.shows(category)
            && (!o.dead || self.shows("Dead"))
            && (kind != ObjectKind::Creature
                || o.pc
                || o.dm
                || o.possessed
                || o.faction.is_none_or(|n| self.shows(&format!("faction:{n}"))))
    }
    pub(crate) fn checkbox(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        label: &str,
        color: egui::Color32,
    ) {
        let mut on = self.shows(key);
        if ui.checkbox(&mut on, egui::RichText::new(label).color(color)).changed() {
            if on {
                self.hidden.remove(key);
            } else {
                self.hidden.insert(key.into());
            }
        }
    }
}
