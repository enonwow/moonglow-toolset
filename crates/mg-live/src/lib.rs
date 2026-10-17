//! Live reports and commands: HTTPS for local clients, Redis pub/sub for the game.
pub mod access;
pub mod agents;
pub mod api;
pub mod bridge;
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
