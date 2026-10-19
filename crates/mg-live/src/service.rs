//! What a host needs to link the store to a game server's Redis, and its credentials.
use crate::{bridge::RedisConfig, store::Policy};
use std::net::SocketAddr;

/// The game link: where the server's Redis is, and which commands are allowed through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveConfig {
    pub redis_host: String,
    pub redis_port: u16,
    pub prefix: String,
    pub policy: Policy,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            redis_host: "127.0.0.1".into(),
            redis_port: 6379,
            prefix: "mglive".into(),
            policy: Policy::default(),
        }
    }
}

/// Cryptographic credentials; never derive bearer tokens from command ids.
pub fn generate_token() -> Result<String, String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("Live token randomness: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

impl LiveConfig {
    pub fn from_env(self) -> Result<Self, String> {
        self.with_overrides(|name| std::env::var(name).ok())
    }

    fn with_overrides(mut self, get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        if let Some(value) = get("MOONGLOW_LIVE_REDIS") {
            if let Ok(address) = value.parse::<SocketAddr>() {
                self.redis_host = address.ip().to_string();
                self.redis_port = address.port();
            } else {
                let (host, port) =
                    value.rsplit_once(':').ok_or("MOONGLOW_LIVE_REDIS needs host:port")?;
                self.redis_host = host.into();
                self.redis_port = port.parse().map_err(|_| "Invalid Redis port")?;
            }
        }
        if let Some(value) = get("MOONGLOW_LIVE_PREFIX") {
            self.prefix = value;
        }
        if let Some(value) = get("MOONGLOW_LIVE_CHUNKS") {
            self.policy.chunks = value == "1";
        }
        Ok(self)
    }

    pub fn redis(&self) -> RedisConfig {
        RedisConfig {
            host: self.redis_host.clone(),
            port: self.redis_port,
            prefix: self.prefix.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_overrides_are_explicit() {
        let defaults = LiveConfig::default();
        assert!(defaults.policy.scripts && !defaults.policy.chunks);
        assert_eq!(defaults.clone().with_overrides(|_| None).unwrap(), defaults);
        let changed = defaults
            .with_overrides(|key| match key {
                "MOONGLOW_LIVE_REDIS" => Some("localhost:1234".into()),
                "MOONGLOW_LIVE_PREFIX" => Some("demo".into()),
                "MOONGLOW_LIVE_CHUNKS" => Some("1".into()),
                _ => None,
            })
            .unwrap();
        assert!(changed.policy.chunks);
        assert_eq!(
            (&*changed.redis_host, changed.redis_port, &*changed.prefix),
            ("localhost", 1234, "demo")
        );
        assert!(LiveConfig::default().with_overrides(|_| Some("invalid".into())).is_err());
    }

    #[test]
    fn tokens_are_random_hex() {
        let token = generate_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(token, generate_token().unwrap());
    }
}
