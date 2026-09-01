mod files;
mod level;

pub use files::{
    ROTATE_BYTES, ROTATE_GENERATIONS, log_path, log_path_from, prepare, rotate_if_large,
    secure_mode,
};
pub use level::{LogLevel, level_from};

#[cfg(feature = "facade")]
mod facade;
#[cfg(feature = "facade")]
pub use facade::init_facade;

#[cfg(feature = "alloc")]
pub mod alloc;

#[cfg(feature = "proc")]
pub mod proc;
