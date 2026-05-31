pub const VERSION: &str = env!("PODMGR_VERSION");

pub mod daemon;
pub mod entry;
pub mod error;
pub mod interceptors;
pub mod protocol;
pub mod socket;
