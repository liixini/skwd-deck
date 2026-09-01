use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::sync::Mutex;

fn walld_log_path() -> std::path::PathBuf {
    skwd_wall_core::paths::cache_dir().join("skwd-walld.log")
}

struct TeeLog {
    file: Mutex<std::fs::File>,
}

impl Write for TeeLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
        Ok(())
    }
}

fn stderr_is(path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(2, &mut st) } != 0 {
        return false;
    }
    st.st_dev == meta.dev() && st.st_ino == meta.ino()
}

pub(crate) fn init_logging(level: &str) {
    let filter = std::env::var("SKWD_WALL_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| level.to_string());
    let mut builder = env_logger::Builder::new();
    builder.parse_filters(&filter);
    let path = walld_log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let redirected = stderr_is(&path);
    skwd_log::rotate_if_large(&path, skwd_log::ROTATE_BYTES);
    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(&path)
    {
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
        if redirected {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::dup2(file.as_raw_fd(), 2);
            }
        } else {
            builder.target(env_logger::Target::Pipe(Box::new(TeeLog { file: Mutex::new(file) })));
        }
    }
    builder.init();
}

mod tests;
