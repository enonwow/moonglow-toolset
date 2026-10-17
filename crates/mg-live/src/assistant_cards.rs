//! Module-owned NPC/GM cards and server-scoped DM notes. No game resources are edited.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NpcCard {
    pub id: String,
    pub name: String,
    pub who: String,
    pub personality: String,
    pub knows: String,
    pub hides: String,
    pub speech: String,
    pub language: String,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GmCard {
    pub campaign_tone: String,
    pub house_rules: String,
    pub never_reveal: String,
    pub dm_replies: String,
    pub narration: String,
    pub language: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Cards {
    pub gm: GmCard,
    pub npcs: BTreeMap<String, NpcCard>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Notes {
    pub objects: BTreeMap<String, String>,
}

pub(super) fn bounded(fields: &[&str]) -> Result<(), String> {
    if fields.iter().any(|s| {
        s.chars().count() > 4000
            || s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    }) {
        return Err("Each field allows up to 4000 characters, without control characters.".into());
    }
    Ok(())
}
impl NpcCard {
    pub fn check(&self) -> Result<(), String> {
        bounded(&[
            &self.name,
            &self.who,
            &self.personality,
            &self.knows,
            &self.hides,
            &self.speech,
            &self.language,
        ])
    }
}
impl GmCard {
    pub fn check(&self) -> Result<(), String> {
        bounded(&[
            &self.campaign_tone,
            &self.house_rules,
            &self.never_reveal,
            &self.dm_replies,
            &self.narration,
            &self.language,
        ])
    }
}

pub(super) fn folder(data: &Path, module: &Path) -> PathBuf {
    let key = module.to_string_lossy();
    #[cfg(windows)]
    let key = key.to_lowercase();
    let stem: String = module
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .take(40)
        .collect();
    data.join("live").join("assistant").join(format!("{stem}-{:016x}", hash(&key)))
}
pub(super) fn hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}
pub(super) fn read<T: DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
        Ok(meta) if meta.len() > 32 * 1024 * 1024 => {
            return Err(format!("{} exceeds 32 MiB; preserved unchanged", path.display()));
        }
        Ok(_) => {}
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("{}: {e}; preserved unchanged", path.display()))
}
pub(super) fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("Assistant file exceeds 32 MiB".into());
    }
    std::fs::create_dir_all(path.parent().ok_or("Missing assistant directory")?)
        .map_err(|e| e.to_string())?;
    let part = path.with_extension("json.part");
    std::fs::write(&part, bytes).map_err(|e| format!("{}: {e}", part.display()))?;
    std::fs::rename(&part, path).map_err(|e| format!("{}: {e}", path.display()))
}
