//! Trusted host configuration, separate from caller-controlled game operations.
use super::*;

#[derive(Debug)]
pub(super) struct GameTranslation {
    pub id: String,
    enabled: bool,
    prefixes: String,
    pub acknowledged_boot: Option<String>,
}

impl GameTranslation {
    pub(super) fn wire(&self, boot: &str) -> Option<Value> {
        (self.acknowledged_boot.as_deref() != Some(boot)).then(
            || json!({"id": self.id, "enabled": u8::from(self.enabled), "prefixes": self.prefixes}),
        )
    }
}

impl LiveStore {
    /// Trusted host configuration, never an agent operation. The bridge retries
    /// it after a fresh server report until that boot acknowledges the revision.
    /// Install/configure the translation hook before enabling interception.
    pub fn configure_translation(&self, enabled: bool, prefixes: &str) -> Result<(), String> {
        if prefixes.len() > 32 || !prefixes.bytes().all(|c| c.is_ascii_graphic()) {
            return Err("Translation prefixes must be at most 32 ASCII graphic characters".into());
        }
        let mut s = self.lock();
        if s.mirror.is_some() {
            return Err("Translation configuration requires the authoritative host".into());
        }
        if s.game_translation
            .as_ref()
            .is_some_and(|old| old.enabled == enabled && old.prefixes == prefixes)
        {
            return Ok(());
        }
        s.game_translation = Some(GameTranslation {
            id: crate::service::generate_token()?[..32].to_owned(),
            enabled,
            prefixes: prefixes.into(),
            acknowledged_boot: None,
        });
        if let Some(kick) = &s.kick {
            let _ = kick.send(());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trusted_translation_configuration_retries_until_reported_and_reapplies_on_boot() {
        let s = LiveStore::default();
        s.sync(br#"{"v":1,"boot":"one","seq":1}"#, 1).unwrap();
        assert!(s.peek_commands(1).get("translation").is_none());
        s.configure_translation(true, "/!#").unwrap();
        let first = s.peek_commands(2)["translation"].clone();
        assert_eq!(first["prefixes"], "/!#");
        assert_eq!(first["enabled"], 1);
        s.configure_translation(true, "/!#").unwrap();
        assert_eq!(s.peek_commands(3)["translation"], first);
        s.sync(
            json!({"v":1,"boot":"one","seq":2,"translation_config":"wrong"}).to_string().as_bytes(),
            4,
        )
        .unwrap();
        assert_eq!(s.peek_commands(4)["translation"], first);
        s.sync(
            json!({"v":1,"boot":"one","seq":3,"translation_config":first["id"]})
                .to_string()
                .as_bytes(),
            5,
        )
        .unwrap();
        assert!(s.peek_commands(5).get("translation").is_none());
        s.sync(br#"{"v":1,"boot":"two","seq":1}"#, 6).unwrap();
        assert_eq!(s.peek_commands(6)["translation"], first);
        s.configure_translation(false, "#").unwrap();
        assert_ne!(s.peek_commands(7)["translation"]["id"], first["id"]);
        assert_eq!(s.peek_commands(7)["translation"]["enabled"], 0);
        for bad in ["\n", "é", &"x".repeat(33)] {
            assert!(s.configure_translation(true, bad).is_err());
        }
    }
    #[test]
    fn trusted_translation_configuration_wakes_once_and_resets_on_endpoint_change() {
        let s = LiveStore::default();
        let mut endpoint = crate::bridge::RedisConfig {
            host: "localhost".into(),
            port: 6379,
            prefix: "fixture".into(),
        };
        s.bind_server(&endpoint);
        s.sync(br#"{"v":1,"boot":"one","seq":1}"#, 1).unwrap();
        let (send, recv) = std::sync::mpsc::channel();
        s.set_kick(send);
        s.configure_translation(true, "/!").unwrap();
        assert!(recv.try_recv().is_ok());
        s.configure_translation(true, "/!").unwrap();
        assert!(recv.try_recv().is_err());
        let wanted = s.peek_commands(1)["translation"].clone();
        // A stale report cannot retire a configuration that was just requested.
        s.sync(
            json!({"v":1,"boot":"one","seq":1,"translation_config":wanted["id"]})
                .to_string()
                .as_bytes(),
            2,
        )
        .unwrap();
        assert_eq!(s.peek_commands(2)["translation"], wanted);
        s.sync(
            json!({"v":1,"boot":"one","seq":2,"translation_config":wanted["id"]})
                .to_string()
                .as_bytes(),
            3,
        )
        .unwrap();
        assert!(s.peek_commands(3).get("translation").is_none());
        endpoint.port += 1;
        s.bind_server(&endpoint);
        s.sync(br#"{"v":1,"boot":"one","seq":1}"#, 4).unwrap();
        assert_eq!(s.peek_commands(4)["translation"], wanted);
    }
}
