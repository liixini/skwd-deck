use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::sync::Mutex;

fn walld_log_path() -> std::path::PathBuf {
    skwd_wall_core::paths::cache_dir().join("skwd-walld.log")
}

struct TeeLog {
    file: Mutex<skwd_log::RotatingWriter>,
    mirror_stderr: bool,
}

impl Write for TeeLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.mirror_stderr {
            let _ = std::io::stderr().write_all(buf);
        }
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
    if let Ok(file) = skwd_log::RotatingWriter::new(path) {
        builder.target(env_logger::Target::Pipe(Box::new(TeeLog {
            file: Mutex::new(file),
            mirror_stderr: !redirected,
        })));
    }
    builder.init();
}

mod tests;
