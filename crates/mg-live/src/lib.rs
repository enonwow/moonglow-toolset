//! Live reports and commands: the store, the agent API's routing and the Redis link to the
//! game server. Hosts (the gateway) supply transport and authentication.
pub mod access;
pub mod api;
pub mod bridge;
#[cfg(feature = "build")]
pub mod build;
mod command_ids;
pub mod events;
pub mod logs;
pub mod operations;
pub mod resp;
pub mod scene;
pub mod service;
pub mod store;
mod wire_text;

pub use service::LiveConfig;

/// Milliseconds since the Unix epoch, shared by the transport and the viewer.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Version of the finished-state mirror schema (independent of the game wire).
pub const MIRROR_VERSION: u32 = 2;

/// In-process gateway core; transport and authentication are supplied by the host.
pub use store::gateway;

/// Avatar URL and address rules shared by the gateway and the desktop.
pub mod avatar;

/// JSON envelopes and bounded decoding for moonglow-gw/1.
pub mod gateway_frames;

mod command_history;
pub use command_history::{command_counter, command_issued_ms};

/// Immutable resource generation descriptors for desktop downloads.
pub mod resource_files;
