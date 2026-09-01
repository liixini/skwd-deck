use std::fs::{File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, LevelFilter, Log, Metadata, Record};

struct Logger {
    file: Mutex<Option<File>>,
}

static LOGGER: Logger = Logger { file: Mutex::new(None) };

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if metadata.target().starts_with("skwd") { true } else { metadata.level() <= Level::Warn }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let line = format!(
            "{} {} {}\n",
            local_timestamp(now.as_secs() as i64, now.subsec_millis()),
            level_tag(record.level()),
            record.args()
        );
        eprint!("{line}");
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
        {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {}
}

fn local_timestamp(secs: i64, millis: u32) -> String {
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let time = secs as libc::time_t;
        libc::localtime_r(&time, &mut tm);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{millis:03}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec,
        )
    }
}

fn level_tag(level: Level) -> &'static str {
    match level {
        Level::Error => "ERROR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    }
}

fn level_filter(level: crate::LogLevel) -> LevelFilter {
    match level {
        crate::LogLevel::Trace => LevelFilter::Trace,
        crate::LogLevel::Debug => LevelFilter::Debug,
        crate::LogLevel::Info => LevelFilter::Info,
        crate::LogLevel::Warn => LevelFilter::Warn,
        crate::LogLevel::Error => LevelFilter::Error,
    }
}

#[cfg(test)]
mod tests;

pub fn init_facade(app: &str, debug: bool) {
    let level =
        level_filter(crate::level_from(debug, std::env::var("SKWD_WALL_LOG").ok().as_deref()));
    if let Some(path) = super::prepare(app)
        && let Ok(file) = {
            let mut options = OpenOptions::new();
            options.create(true).append(true);
            #[cfg(unix)]
            options.mode(0o600);
            options.open(&path)
        }
        && let Ok(mut guard) = LOGGER.file.lock()
    {
        super::files::secure_mode(&path, 0o600);
        *guard = Some(file);
    }
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(level);
}
