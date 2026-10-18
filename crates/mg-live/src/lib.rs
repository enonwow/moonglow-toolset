//! Live reports and commands: HTTPS for local clients, Redis pub/sub for the game.
pub mod access;
pub mod agents;
pub mod api;
pub mod bridge;
#[cfg(feature = "build")]
pub mod build;
mod command_ids;
mod descriptor;
pub mod events;
pub mod logs;
pub mod operations;
pub mod resp;
pub mod scene;
pub mod service;
pub mod store;
mod wire_text;
mod wsl;

pub use agents::AgentPresence;
pub use service::{LiveConfig, LiveService};

/// Milliseconds since the Unix epoch, shared by the transport and the viewer.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Version of the finished-state mirror schema (independent of the game wire).
pub const MIRROR_VERSION: u32 = 1;

/// In-process gateway core; transport and authentication are supplied by the host.
pub use store::gateway;

/// JSON envelopes and bounded decoding for moonglow-gw/1.
pub mod gateway_frames;
