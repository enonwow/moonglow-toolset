//! Versioned resources: URLs are transport locations, never generation identity.
use mg_core::sha1::{hex, sha1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceGeneration {
    pub boot: String,
    /// The module-only manifest digest is its content version.
    pub module_manifest: String,
    pub haks_manifest: String,
    /// Every custom talk table in the hak manifest, keyed by resource name.
    pub tlk: BTreeMap<String, String>,
    pub module_url: String,
    pub haks_url: String,
}
pub fn digest(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl ResourceGeneration {
    pub fn validate(&self) -> Result<(), String> {
        if self.boot.is_empty()
            || self.boot.len() > 256
            || !digest(&self.module_manifest)
            || !digest(&self.haks_manifest)
            || self.tlk.len() > 64
            || self.tlk.iter().any(|(k, v)| {
                k.is_empty()
                    || k.len() > 16
                    || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    || !digest(v)
            })
        {
            return Err("invalid resource generation".into());
        }
        Ok(())
    }
    /// Stable identity independent of URL changes and map insertion order.
    pub fn id(&self) -> String {
        hex(&sha1(
            &serde_json::to_vec(&(
                &self.boot,
                &self.module_manifest,
                &self.haks_manifest,
                &self.tlk,
            ))
            .expect("serializable resource identity"),
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_binds_every_resource_version_but_not_urls() {
        let g = ResourceGeneration {
            boot: "boot".into(),
            module_manifest: "a".repeat(40),
            haks_manifest: "b".repeat(40),
            tlk: BTreeMap::new(),
            module_url: "/files/v1/".into(),
            haks_url: "https://assets.example.invalid/".into(),
        };
        assert!(g.validate().is_ok());
        assert_eq!(g.id(), "8b2a34de8613fca324a0b749572bdadda777ca9b");
        for field in 0..4 {
            let mut changed = g.clone();
            match field {
                0 => changed.boot.push('2'),
                1 => changed.module_manifest = "c".repeat(40),
                2 => changed.haks_manifest = "c".repeat(40),
                _ => {
                    changed.tlk.insert("custom".into(), "c".repeat(40));
                }
            }
            assert_ne!(g.id(), changed.id());
        }
        let mut moved = g.clone();
        moved.haks_url.push_str("other/");
        assert_eq!(g.id(), moved.id());
        moved.module_manifest = "../escape".into();
        assert!(moved.validate().is_err());
        assert_eq!(
            serde_json::from_slice::<ResourceGeneration>(&serde_json::to_vec(&g).unwrap()).unwrap(),
            g
        );
    }
}
