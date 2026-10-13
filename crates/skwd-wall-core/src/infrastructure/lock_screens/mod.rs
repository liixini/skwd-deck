#[cfg(feature = "daemon")]
mod adapter;
pub mod backup;
mod ownership;
pub mod sddm;
#[cfg(feature = "daemon")]
mod shells;
#[cfg(feature = "daemon")]
mod text;

#[cfg(feature = "daemon")]
const ERROR_SEPARATOR: &str = "; ";

#[cfg(feature = "daemon")]
pub use adapter::{
    DIRECTORY, check_path, config_home, enabled, follows, requested, sync, sync_target,
    target_available,
};

#[cfg(all(test, feature = "daemon"))]
mod tests;

pub const IMAGE_FILE: &str = "wallpaper.png";
