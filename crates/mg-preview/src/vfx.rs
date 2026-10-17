//! Visual effects (`visualeffects.2da`): which models an effect shows and on
//! which of an object's nodes, so a live view can draw what the game shows
//! (glows, auras, impacts, spells).
//!
//! A row names up to six models where it lands (`Imp_*`) and six where it
//! ends (`Ces_*`): one at the head (`headconjure`), one at the impact point
//! (`impact`) and one at the feet chosen by the creature's size
//! (`Imp_Root_S/M/L/H_Node`). `Type_FD` says how long it shows: D lasts while
//! the effect does, F plays once, B is a beam, P a projectile.

use crate::{Part, Preview};
use mg_2da::TwoDa;

/// How long an effect shows (`Type_FD`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfxKind {
    /// D: while the effect lasts.
    Duration,
    /// F: once, as it lands.
    Instant,
    /// B: a beam between two objects.
    Beam,
    /// P: a projectile in flight.
    Projectile,
}

/// A creature's size, which picks the root model (appearance.2da
/// `SIZECATEGORY`: 1 tiny, 2 small, 3 medium, 4 large, 5 huge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
    Large,
    Huge,
}

impl Size {
    pub fn from_category(category: i64) -> Size {
        match category {
            ..=2 => Size::Small,
            3 => Size::Medium,
            4 => Size::Large,
            _ => Size::Huge,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The node a head model hangs from.
pub const HEAD_NODE: &str = "headconjure";
/// The node an impact model hangs from.
pub const IMPACT_NODE: &str = "impact";

/// One model of an effect and the node it hangs from (`None`: the object's
/// origin, at its feet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VfxModel {
    pub model: String,
    pub node: Option<&'static str>,
}

/// The models of one place in a row: head, impact, and root by size.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Models {
    head: Option<String>,
    impact: Option<String>,
    /// Small, medium, large, huge.
    root: [Option<String>; 4],
}

impl Models {
    fn read(table: &TwoDa, row: usize, prefix: &str) -> Models {
        let model = |column: &str| {
            table
                .get(row, &format!("{prefix}_{column}"))
                .map(str::trim)
                .filter(|s| !s.is_empty() && *s != "****")
                .map(str::to_ascii_lowercase)
        };
        Models {
            head: model("HeadCon_Node"),
            impact: model("Impact_Node"),
            root: [
                model("Root_S_Node"),
                model("Root_M_Node"),
                model("Root_L_Node"),
                model("Root_H_Node"),
            ],
        }
    }

    /// The models for a creature of `size`: a missing root model of that
    /// size falls back to the medium one, then any other.
    fn for_size(&self, size: Size) -> Vec<VfxModel> {
        let mut out = Vec::new();
        if let Some(m) = &self.head {
            out.push(VfxModel { model: m.clone(), node: Some(HEAD_NODE) });
        }
        if let Some(m) = &self.impact {
            out.push(VfxModel { model: m.clone(), node: Some(IMPACT_NODE) });
        }
        let order = [size.index(), Size::Medium.index(), 0, 2, 3];
        if let Some(m) = order.iter().find_map(|&i| self.root[i].as_ref()) {
            out.push(VfxModel { model: m.clone(), node: None });
        }
        out
    }
}

/// One row of `visualeffects.2da`.
#[derive(Debug, Clone, PartialEq)]
pub struct Vfx {
    pub row: usize,
    pub label: String,
    pub kind: VfxKind,
    pub orient_with_ground: bool,
    pub orient_with_object: bool,
    /// Left out when the game runs with low quality (`LowQuality`).
    pub low_quality: bool,
    impact: Models,
    cessation: Models,
}

impl Vfx {
    /// Row `row`, or `None` when it is empty or of an unknown kind.
    pub fn read(table: &TwoDa, row: usize) -> Option<Vfx> {
        let kind = match table.get(row, "Type_FD").map(str::trim)? {
            "D" | "d" => VfxKind::Duration,
            "F" | "f" => VfxKind::Instant,
            "B" | "b" => VfxKind::Beam,
            "P" | "p" => VfxKind::Projectile,
            _ => return None,
        };
        let flag = |column: &str| table.get_int(row, column).is_some_and(|v| v != 0);
        Some(Vfx {
            row,
            label: table.get(row, "Label").unwrap_or_default().to_string(),
            kind,
            orient_with_ground: flag("OrientWithGround"),
            orient_with_object: flag("OrientWithObject"),
            low_quality: flag("LowQuality"),
            impact: Models::read(table, row, "Imp"),
            cessation: Models::read(table, row, "Ces"),
        })
    }

    /// What it shows where it lands (and, for a lasting effect, while it
    /// lasts) on a creature of `size`.
    pub fn models(&self, size: Size) -> Vec<VfxModel> {
        self.impact.for_size(size)
    }

    /// What it shows as it ends.
    pub fn cessation_models(&self, size: Size) -> Vec<VfxModel> {
        self.cessation.for_size(size)
    }

    /// Whether it draws anything at all (some rows only play a sound).
    pub fn has_models(&self) -> bool {
        !self.models(Size::Medium).is_empty() || !self.cessation_models(Size::Medium).is_empty()
    }
}

/// An object's preview with an effect's models hung from its nodes; each
/// plays its own animation.
pub fn with_models(mut preview: Preview, models: &[VfxModel]) -> Preview {
    preview.parts.extend(models.iter().map(|m| {
        let mut part = Part::new(&m.model);
        part.attach = m.node.map(str::to_string);
        part.animated = true;
        part.animation = Some("duration".into());
        part
    }));
    preview
}

/// An effect at a place (no object): its first model as the base and the
/// others with it. `None` when it has no models.
pub fn at_location(models: &[VfxModel]) -> Option<Preview> {
    let (first, rest) = models.split_first()?;
    let mut preview = Preview::model(&first.model);
    preview.base.animated = true;
    preview.parts.extend(rest.iter().map(|m| {
        let mut part = Part::new(&m.model);
        part.animated = true;
        part
    }));
    Some(preview)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mg_core::Codepage;

    const TABLE: &str = "2DA V2.0\n\n\
        Label Type_FD OrientWithGround Imp_HeadCon_Node Imp_Impact_Node Imp_Root_S_Node \
        Imp_Root_M_Node Imp_Root_L_Node Imp_Root_H_Node Ces_HeadCon_Node Ces_Impact_Node \
        Ces_Root_S_Node Ces_Root_M_Node Ces_Root_L_Node Ces_Root_H_Node LowQuality OrientWithObject\n\
        0 VFX_DUR_BLUR D 0 **** **** **** **** **** **** **** **** **** **** **** **** 0 0\n\
        1 VFX_DUR_ENTANGLE D 0 **** **** **** vdr_Entangle vdr_entang01 vdr_entang02 **** **** **** **** **** **** 0 0\n\
        2 VFX_DUR_MIND F 1 vdr_mindhit vim_hit **** **** **** **** **** **** **** **** **** **** 1 1\n\
        3 VFX_ODD X 0 **** **** **** **** **** **** **** **** **** **** **** **** 0 0\n\
        4 VFX_SMALL_ONLY P 0 **** **** vfx_s **** **** **** **** **** **** **** **** **** 0 0\n\
        5 **** **** **** **** **** **** **** **** **** **** **** **** **** **** **** 0 0\n";

    fn table() -> TwoDa {
        TwoDa::parse(TABLE.as_bytes(), Codepage::WINDOWS_1252).expect("test table parses")
    }

    #[test]
    fn rows_give_kind_flags_and_models_by_node_and_size() {
        let t = table();
        let blur = Vfx::read(&t, 0).unwrap();
        assert_eq!(blur.kind, VfxKind::Duration);
        assert!(!blur.has_models(), "a sound-only row draws nothing");

        let entangle = Vfx::read(&t, 1).unwrap();
        assert_eq!(
            entangle.models(Size::Medium),
            vec![VfxModel { model: "vdr_entangle".into(), node: None }],
            "names are lower case"
        );
        assert_eq!(entangle.models(Size::Large)[0].model, "vdr_entang01");
        assert_eq!(entangle.models(Size::Huge)[0].model, "vdr_entang02");
        assert_eq!(entangle.models(Size::Small)[0].model, "vdr_entangle", "missing small: medium");

        let mind = Vfx::read(&t, 2).unwrap();
        assert_eq!(mind.kind, VfxKind::Instant);
        assert!(mind.orient_with_ground && mind.orient_with_object && mind.low_quality);
        assert_eq!(
            mind.models(Size::Medium),
            vec![
                VfxModel { model: "vdr_mindhit".into(), node: Some(HEAD_NODE) },
                VfxModel { model: "vim_hit".into(), node: Some(IMPACT_NODE) },
            ]
        );

        assert_eq!(
            Vfx::read(&t, 4).unwrap().models(Size::Huge)[0].model,
            "vfx_s",
            "any root as last resort"
        );
        assert!(Vfx::read(&t, 3).is_none(), "unknown Type_FD");
        assert!(Vfx::read(&t, 5).is_none(), "empty row");
        assert!(Vfx::read(&t, 99).is_none(), "past the table");
    }

    #[test]
    fn sizes_from_appearance_categories() {
        assert_eq!(Size::from_category(1), Size::Small);
        assert_eq!(Size::from_category(2), Size::Small);
        assert_eq!(Size::from_category(3), Size::Medium);
        assert_eq!(Size::from_category(4), Size::Large);
        assert_eq!(Size::from_category(5), Size::Huge);
        assert_eq!(Size::from_category(-7), Size::Small);
    }

    #[test]
    fn models_hang_from_an_object_or_stand_at_a_place() {
        let models = Vfx::read(&table(), 2).unwrap().models(Size::Medium);
        let on = with_models(Preview::model("c_ogre"), &models);
        assert_eq!(on.base.model, "c_ogre");
        assert_eq!(on.parts.len(), 2);
        assert_eq!(on.parts[0].attach.as_deref(), Some(HEAD_NODE));
        assert_eq!(on.parts[1].attach.as_deref(), Some(IMPACT_NODE));
        assert!(on.parts.iter().all(|p| p.animated));

        let at = at_location(&models).unwrap();
        assert_eq!(at.base.model, "vdr_mindhit");
        assert_eq!(at.parts.len(), 1);
        assert!(at.base.animated && at.parts[0].attach.is_none());
        assert!(at_location(&[]).is_none());
    }
}
