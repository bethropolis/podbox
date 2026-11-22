pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod daemon;
pub mod entry;
pub mod error;
pub mod interceptors;
pub mod protocol;
pub mod socket;
