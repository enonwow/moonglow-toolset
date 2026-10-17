//! Live marker colours and the module's editable faction palette.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::live::LiveObject;
use egui::Color32;
use mg_area::ObjectKind;
use mg_module::factions::Factions;

pub type Overrides = BTreeMap<PathBuf, BTreeMap<u32, [u8; 3]>>;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    #[default]
    Faction,
    Hostility,
    Kind,
}

impl ColorMode {
    pub const ALL: [Self; 3] = [Self::Faction, Self::Hostility, Self::Kind];
    pub fn name(self) -> &'static str {
        match self {
            Self::Faction => "Faction",
            Self::Hostility => "Hostility",
            Self::Kind => "Kind",
        }
    }
}

const PLAYER: Color32 = Color32::from_rgb(95, 175, 255);
const DM: Color32 = Color32::from_rgb(195, 130, 255);
const NEUTRAL: Color32 = Color32::from_rgb(255, 170, 65);
const UNKNOWN: Color32 = Color32::from_gray(185);

pub fn default_color(id: u32) -> Color32 {
    match id {
        0 => PLAYER,
        1 => Color32::LIGHT_RED,
        2 => NEUTRAL,
        3 => Color32::GOLD,
        4 => Color32::LIGHT_GREEN,
        _ => {
            const CUSTOM: [[u8; 3]; 8] = [
                [90, 220, 210],
                [225, 125, 225],
                [195, 205, 100],
                [120, 155, 255],
                [245, 160, 165],
                [155, 215, 160],
                [215, 180, 125],
                [180, 155, 220],
            ];
            let [r, g, b] = CUSTOM[(id as usize - 5) % CUSTOM.len()];
            Color32::from_rgb(r, g, b)
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Palette(pub Vec<(String, Color32)>);

impl Palette {
    pub(crate) fn for_app(app: &mut crate::Moonglow) -> Self {
        app.factions()
            .map(|factions| {
                let module = app.module_path();
                let overrides =
                    module.as_ref().and_then(|p| app.settings.live_faction_colors.get(p));
                Self::new(&factions, overrides)
            })
            .unwrap_or_default()
    }

    pub(crate) fn new(factions: &Factions, overrides: Option<&BTreeMap<u32, [u8; 3]>>) -> Self {
        Self(
            factions
                .factions
                .iter()
                .enumerate()
                .map(|(id, faction)| {
                    let color = overrides.and_then(|o| o.get(&(id as u32))).map_or_else(
                        || default_color(id as u32),
                        |&[r, g, b]| Color32::from_rgb(r, g, b),
                    );
                    (faction.name.clone(), color)
                })
                .collect(),
        )
    }

    pub(crate) fn color(
        &self,
        mode: ColorMode,
        kind: ObjectKind,
        object: &LiveObject,
        pc: Option<&LiveObject>,
    ) -> Color32 {
        if object.dead {
            return Color32::GRAY;
        }
        if kind != ObjectKind::Creature {
            return world_color(kind);
        }
        match mode {
            ColorMode::Faction => {
                object.faction.and_then(|id| self.0.get(id as usize)).map_or(UNKNOWN, |(_, c)| *c)
            }
            ColorMode::Hostility => {
                let Some(pc) = pc.filter(|pc| pc.pc || pc.dm || pc.possessed) else {
                    return UNKNOWN;
                };
                if object.id == pc.id {
                    return PLAYER;
                }
                if object.hostile.contains(&pc.id) {
                    return Color32::LIGHT_RED;
                }
                match &object.friendly {
                    Some(friendly) if friendly.contains(&pc.id) => Color32::LIGHT_GREEN,
                    Some(_) => NEUTRAL,
                    None => UNKNOWN,
                }
            }
            ColorMode::Kind => kind_color(object),
        }
    }
}

/// World objects carry engine factions too, but these are not useful DM categories.
pub(crate) fn world_color(kind: ObjectKind) -> Color32 {
    match kind {
        ObjectKind::Placeable => Color32::from_rgb(160, 190, 205),
        ObjectKind::Door => Color32::from_rgb(195, 185, 160),
        ObjectKind::Item => Color32::from_rgb(205, 205, 205),
        _ => UNKNOWN,
    }
}

fn kind_color(object: &LiveObject) -> Color32 {
    if object.dm || object.possessed {
        DM
    } else if object.pc {
        PLAYER
    } else if object.own {
        Color32::LIGHT_GREEN
    } else {
        NEUTRAL
    }
}

pub(crate) fn options(
    ui: &mut egui::Ui,
    overrides: &mut Overrides,
    module: Option<&Path>,
    factions: Option<&Factions>,
) {
    egui::CollapsingHeader::new("Factions").show(ui, |ui| {
        let (Some(module), Some(factions)) = (module, factions) else {
            ui.weak("Open and save a module to customise its Live faction colours.");
            return;
        };
        ui.weak(format!(
            "Colours for {} (Apply to save)",
            module.file_name().unwrap_or_default().to_string_lossy()
        ));
        let palette = Palette::new(factions, overrides.get(module));
        for (id, (name, color)) in palette.0.iter().enumerate() {
            ui.push_id(id, |ui| {
                ui.horizontal(|ui| {
                    let mut rgb = [color.r(), color.g(), color.b()];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        overrides.entry(module.to_path_buf()).or_default().insert(id as u32, rgb);
                    }
                    ui.label(format!("{id}: {name}"));
                })
            });
        }
        if ui.button("Reset faction colours").clicked() {
            overrides.remove(module);
        }
    });
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_use_module_ids_overrides_and_explicit_relationships() {
        let factions = Factions::read(&mg_module::new::default_factions());
        let palette = Palette::new(&factions, Some(&BTreeMap::from([(2, [12, 34, 56])])));
        assert_eq!(palette.0[2], ("Commoner".into(), Color32::from_rgb(12, 34, 56)));
        let pc = LiveObject { id: "1".into(), pc: true, ..Default::default() };
        let mut o = LiveObject { id: "2".into(), faction: Some(2), ..Default::default() };
        let color = |mode, o: &LiveObject, pc| palette.color(mode, ObjectKind::Creature, o, pc);
        assert_eq!(color(ColorMode::Faction, &o, None), palette.0[2].1);
        o.faction = Some(999);
        assert_eq!(color(ColorMode::Faction, &o, None), UNKNOWN);
        assert_eq!(color(ColorMode::Hostility, &o, Some(&pc)), UNKNOWN);
        o.friendly = Some(vec![]);
        assert_eq!(color(ColorMode::Hostility, &o, Some(&pc)), NEUTRAL);
        o.friendly = Some(vec![pc.id.clone()]);
        assert_eq!(color(ColorMode::Hostility, &o, Some(&pc)), Color32::LIGHT_GREEN);
        o.hostile.push(pc.id.clone());
        assert_eq!(color(ColorMode::Hostility, &o, Some(&pc)), Color32::LIGHT_RED);
        assert_eq!(color(ColorMode::Hostility, &o, None), UNKNOWN);
        o.dead = true;
        assert_eq!(color(ColorMode::Hostility, &o, Some(&pc)), Color32::GRAY);
    }

    #[test]
    fn world_objects_use_kind_colours_even_with_a_commoner_faction() {
        let palette = Palette::new(&Factions::read(&mg_module::new::default_factions()), None);
        let o = LiveObject { faction: Some(2), own: true, ..Default::default() };
        for kind in [ObjectKind::Placeable, ObjectKind::Door, ObjectKind::Item] {
            for mode in ColorMode::ALL {
                assert_eq!(palette.color(mode, kind, &o, None), world_color(kind));
                assert_ne!(palette.color(mode, kind, &o, None), palette.0[2].1);
            }
        }
    }
}
