//! Desktop gateway authentication, WebSocket link and passive views.
//! Browser opening belongs to the caller.

mod avatar;
pub mod credentials;
mod facade;
mod link;
mod login;
mod state;
mod tls;
mod websocket;
mod worker;

pub use avatar::AvatarClient;
pub use facade::BookKind;
pub use link::{LinkConfig, LinkService, PendingCall, StartError};
pub use state::{LinkError, LinkSnapshot, LinkStatus};

pub use login::{Error, Gateway, Login, SessionToken};
pub use tls::{TrustPrompt, fingerprint};

/// Blocking, cancellable NWSync download API for GUI job workers.
pub mod resources;

/// Typed identity response decoding for the gateway owner controls.
pub mod identities;
