//! Desktop gateway login. Browser opening belongs to the caller.

pub mod credentials;
mod link;
mod login;
mod state;
mod tls;
mod websocket;
mod worker;

pub use link::{LinkConfig, LinkService, PendingCall, StartError};
pub use state::{LinkError, LinkSnapshot, LinkStatus};

pub use login::{Error, Gateway, Login, SessionToken};
pub use tls::{TrustPrompt, fingerprint};
