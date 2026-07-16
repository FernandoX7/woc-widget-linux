//! API service boundaries. Each service delegates transport hardening to
//! [`crate::http::fetch_decoded`] and performs only its feed-specific validation.

mod community;
mod crypto;
mod gecko_terminal;
mod status;

pub use community::CommunityService;
pub use crypto::CryptoService;
pub use gecko_terminal::GeckoTerminalService;
pub use status::StatusService;
