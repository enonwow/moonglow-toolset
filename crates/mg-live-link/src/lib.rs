//! Desktop gateway login. Browser opening belongs to the caller.

mod login;
mod tls;

pub use login::{Error, Gateway, Login, SessionToken};
pub use tls::{TrustPrompt, fingerprint};
