use std::path::{Path, PathBuf};

pub fn target_bin(name: &str) -> PathBuf {
    if let Some(dir) = std::env::var_os("SKWD_E2E_BIN_DIR") {
        return PathBuf::from(dir).join(name);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release").join(name)
}

#[macro_export]
macro_rules! stub_renderer {
    () => {
        ::std::env::var("SKWD_E2E_STUB")
            .unwrap_or_else(|_| env!("CARGO_BIN_EXE_fake_renderer").to_string())
    };
}
