use std::env;
use std::path::PathBuf;

pub fn socket_path() -> PathBuf {
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(runtime_dir).join("skwd-wall-v2").join("wall.sock")
}

pub fn resolve_socket() -> PathBuf {
    env::var_os("SKWD_WALL_V2_SOCK").map_or_else(socket_path, PathBuf::from)
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
