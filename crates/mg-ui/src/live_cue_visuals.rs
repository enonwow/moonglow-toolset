//! Models named by spell/VFX resources. Events give timing and endpoints; they
//! do not give the native animation pose, so these are short visual cues.
use mg_area::{AreaObject, ObjectKind};
use mg_preview::{
    Preview,
    vfx::{Size, Vfx, VfxModel},
};
use mg_rules::GameData;

#[derive(Debug, Default)]
pub(super) struct Visuals {
    pub(super) cast: Option<AreaObject>,
    pub(super) projectile: Option<AreaObject>,
    pub(super) impact: Option<AreaObject>,
}

fn object(game: &GameData, mut preview: Preview, animation: &str) -> AreaObject {
    preview.idle = Some(animation.into());
    preview.base.animated = true;
    let mut object =
        AreaObject::read(game, ObjectKind::Placeable, usize::MAX, &mg_gff::Struct::new(0));
    object.preview = Some(preview);
    object.problem = None;
    object.is_static = false;
    object
}

fn effect(game: &GameData, row: i32, lasting: bool) -> Option<AreaObject> {
    let table = game.table("visualeffects").ok()?;
    let vfx = Vfx::read(&table, usize::try_from(row).ok()?)?;
    let preview = mg_preview::vfx::at_location(&vfx.models(Size::Medium))?;
    Some(object(game, preview, if lasting { "duration" } else { "impact" }))
}

impl Visuals {
    pub(super) fn read(
        game: &GameData,
        spell: Option<i64>,
        vfx: Option<i32>,
        lasting: bool,
    ) -> Self {
        if let Some(row) = vfx {
            return Self { impact: effect(game, row, lasting), ..Self::default() };
        }
        let Some(row) = spell.and_then(|n| usize::try_from(n).ok()) else { return Self::default() };
        let Ok(table) = game.table("spells") else { return Self::default() };
        let cast: Vec<_> = ["Head", "Hand", "Grnd"]
            .into_iter()
            .filter_map(|node| {
                let model = table
                    .get(row, &format!("Cast{node}Visual"))
                    .or_else(|| table.get(row, &format!("Conj{node}Visual")))?;
                Some(VfxModel { model: model.to_ascii_lowercase(), node: None })
            })
            .collect();
        let cast = mg_preview::vfx::at_location(&cast).map(|p| object(game, p, "conjure01"));
        let projectile =
            table.get(row, "ProjModel").map(|name| object(game, Preview::model(name), "travel01"));
        // The stock Fireball impact is explicit in its shipped script. Other
        // spells' scripted impacts cannot be inferred from their display name.
        let impact = (|| {
            if !table.get(row, "Label")?.eq_ignore_ascii_case("Fireball") {
                return None;
            }
            let script = game
                .resman
                .get_named(table.get(row, "ImpactScript")?, mg_core::ResType::NSS)
                .ok()?;
            let script = String::from_utf8_lossy(&script);
            if !script.contains("EffectVisualEffect(VFX_FNF_FIREBALL)") {
                return None;
            }
            let effects = game.table("visualeffects").ok()?;
            let row = (0..effects.len())
                .find(|&r| effects.get(r, "Label") == Some("VFX_FNF_FIREBALL"))?;
            effect(game, row as i32, false)
        })();
        Self { cast, projectile, impact }
    }
}

pub(crate) struct Draw {
    pub(crate) id: String,
    pub(crate) object: AreaObject,
    pub(crate) time: f32,
}
