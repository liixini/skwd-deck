pub mod model;
#[cfg(feature = "daemon")]
mod store;

pub use model::{Config, Integration, config_path, detect_steam_root};
#[cfg(feature = "daemon")]
pub use store::ConfigStore;

#[cfg(all(test, feature = "daemon"))]
mod tests;
