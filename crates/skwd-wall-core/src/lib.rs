pub mod backend;
pub mod composition;
pub mod domain;
pub mod infrastructure;

#[cfg(feature = "daemon")]
pub use composition::state;
#[cfg(feature = "daemon")]
pub use composition::state::WallState;
pub use domain::random::xorshift64;
pub use domain::version::version;

#[cfg(feature = "daemon")]
pub use infrastructure::audio;
#[cfg(feature = "daemon")]
pub use infrastructure::awww;
#[cfg(feature = "media")]
pub use infrastructure::blocks;
#[cfg(feature = "daemon")]
pub use infrastructure::bridge_preview;
pub use infrastructure::config;
pub use infrastructure::database::repository as db;
pub use infrastructure::diagnostics as diag;
#[cfg(feature = "daemon")]
pub use infrastructure::dms;
#[cfg(feature = "daemon")]
pub use infrastructure::matugen;
#[cfg(feature = "media")]
pub use infrastructure::media;
#[cfg(feature = "daemon")]
pub use infrastructure::noctalia;
pub use infrastructure::outputs;
pub use infrastructure::pack;
pub use infrastructure::paths;
#[cfg(feature = "daemon")]
pub use infrastructure::plasma;
#[cfg(feature = "daemon")]
pub use infrastructure::postprocess;
pub use infrastructure::process as proc;
#[cfg(feature = "media")]
pub use infrastructure::scan;
#[cfg(feature = "daemon")]
pub use infrastructure::shell_adapter;
#[cfg(feature = "daemon")]
pub use infrastructure::static_templates;
pub use infrastructure::synchronization::lock;
#[cfg(feature = "daemon")]
pub use infrastructure::theme;
#[cfg(feature = "daemon")]
pub use infrastructure::theme::{material, style};
#[cfg(feature = "daemon")]
pub use infrastructure::theme_provider;
#[cfg(feature = "daemon")]
pub use infrastructure::theme_sink;
#[cfg(feature = "daemon")]
pub use infrastructure::wallpaper::apply;
#[cfg(feature = "daemon")]
pub use infrastructure::we;
#[cfg(feature = "obs-heap")]
pub use skwd_log::alloc as countalloc;
