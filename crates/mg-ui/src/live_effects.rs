//! Bounded visual cues for the running game's effects. Marker colours describe
//! the reported effect; they do not emulate the engine's procedural shaders.
use egui::Color32;
use mg_preview::vfx::{Size, Vfx, VfxModel};

pub(crate) const MAX_MODELS: usize = 64;
pub(crate) const MAX_PARTICLES: usize = 20_000;

/// Localized name from the loaded module's spells.2da and talk tables.
/// Shared with Live Log; unknown/custom rows retain an explicit fallback.
pub fn spell_name(game: &mg_rules::GameData, spell: i64) -> String {
    let row = usize::try_from(spell).ok();
    let table = game.table("spells").ok();
    if let (Some(row), Some(table)) = (row, table) {
        if let Some(name) = table
            .get(row, "Name")
            .and_then(|n| n.parse::<u32>().ok())
            .and_then(|n| game.string(mg_core::StrRef(n)))
            .filter(|n| !n.is_empty())
        {
            return name;
        }
        if let Some(label) = table.get(row, "Label").filter(|n| !n.is_empty() && *n != "****") {
            return label.replace('_', " ");
        }
    }
    format!("Unknown spell ({spell})")
}

/// EE's targeting sphere radius in metres. Range is casting reach, not radius;
/// cones/lines and absent metadata must not be presented as circular areas.
pub fn spell_radius(game: &mg_rules::GameData, spell: i64) -> Option<f32> {
    let row = usize::try_from(spell).ok()?;
    let table = game.table("spells").ok()?;
    if !table.get(row, "TargetShape")?.eq_ignore_ascii_case("sphere") {
        return None;
    }
    let radius: f32 = table.get(row, "TargetSizeX")?.parse().ok()?;
    (radius.is_finite() && radius > 0.).then_some(radius)
}

/// Per-area Effects control, shared by the toolbar and Live menu.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EffectsMode {
    Off,
    Markers,
    #[default]
    Full,
}

impl EffectsMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Markers, Self::Full];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Markers => "Markers",
            Self::Full => "Full",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Effect {
    pub name: String,
    pub color: Color32,
    vfx: Option<Vfx>,
}

impl Effect {
    pub(crate) fn read(game: &mg_rules::GameData, row: i32) -> Self {
        let table = game.table("visualeffects").ok();
        let label = usize::try_from(row)
            .ok()
            .and_then(|row| table.as_ref()?.get(row, "Label"))
            .filter(|label| !label.is_empty() && *label != "****");
        let mut effect = Self::from_label(row, label);
        effect.vfx = usize::try_from(row).ok().and_then(|row| Vfx::read(table.as_deref()?, row));
        effect
    }

    fn from_label(row: i32, label: Option<&str>) -> Self {
        let name = label
            .map(|label| {
                label
                    .trim_start_matches("VFX_")
                    .trim_start_matches("DUR_")
                    .trim_start_matches("IMP_")
                    .replace('_', " ")
                    .to_ascii_lowercase()
            })
            .unwrap_or_else(|| format!("VFX {row}"));
        let color = effect_color(&name);
        Self { name, color, vfx: None }
    }

    pub(crate) fn models(&self, size: Size) -> Vec<VfxModel> {
        self.vfx.as_ref().map_or_else(Vec::new, |vfx| vfx.models(size))
    }
}

/// Read the same appearance row as the creature preview, including player looks.
pub(crate) fn size(game: &mg_rules::GameData, creature: &mg_gff::Struct) -> Size {
    creature
        .int("Appearance_Type")
        .and_then(|row| usize::try_from(row).ok())
        .and_then(|row| game.table("appearance").ok()?.get_int(row, "SIZECATEGORY"))
        .map_or(Size::Medium, |category| Size::from_category(i64::from(category)))
}

/// Batches already arrive nearest-object first from ghost_particles. A quad
/// or trail segment is six vertices; never cut a triangle pair in half.
pub(crate) fn cap_particles(batches: &mut Vec<mg_render::particles::ParticleBatch>) -> usize {
    let mut remaining = MAX_PARTICLES;
    for batch in batches.iter_mut() {
        let count = (batch.vertices.len() / 6).min(remaining);
        batch.vertices.truncate(count * 6);
        remaining -= count;
    }
    batches.retain(|batch| !batch.vertices.is_empty());
    MAX_PARTICLES - remaining
}

fn effect_color(name: &str) -> Color32 {
    let has = |word| name.split_whitespace().any(|s| s == word);
    if has("red") || has("fire") || has("flame") {
        Color32::from_rgb(255, 100, 80)
    } else if has("blue") || has("cold") || has("ice") {
        Color32::from_rgb(100, 180, 255)
    } else if has("green") || has("acid") || has("poison") {
        Color32::from_rgb(100, 240, 130)
    } else if has("purple") || has("violet") || has("negative") {
        Color32::from_rgb(200, 130, 255)
    } else if has("white") || has("healing") || has("holy") {
        Color32::from_rgb(245, 245, 255)
    } else if has("orange") || has("yellow") || has("electrical") {
        Color32::from_rgb(255, 215, 85)
    } else {
        Color32::from_rgb(210, 180, 255)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_budget_keeps_nearest_batches_and_whole_quads() {
        use mg_render::particles::{ParticleBatch, ParticleBlend, ParticleVertex};
        let batch = |count: usize| ParticleBatch {
            texture: None,
            blend: ParticleBlend::Normal,
            render_order: 0,
            tint: None,
            vertices: vec![ParticleVertex { pos: [0.; 3], uv: [0.; 2], color: [1.; 4] }; count * 6],
        };
        let mut batches = vec![batch(12_000), batch(12_000), batch(1)];
        assert_eq!(cap_particles(&mut batches), MAX_PARTICLES);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].vertices.len(), 12_000 * 6);
        assert_eq!(batches[1].vertices.len(), 8_000 * 6);
    }

    #[test]
    fn missing_rows_and_procedural_glows_keep_a_marker() {
        let red = Effect::from_label(411, Some("VFX_DUR_GLOW_RED"));
        let blue = Effect::from_label(410, Some("VFX_DUR_GLOW_BLUE"));
        assert_eq!(red.name, "glow red");
        assert_ne!(red.color, blue.color);
        assert_eq!(Effect::from_label(-1, None).name, "VFX -1");
        assert_eq!(EffectsMode::default(), EffectsMode::Full);
    }
}
