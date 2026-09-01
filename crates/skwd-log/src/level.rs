#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

pub fn level_from(debug: bool, env: Option<&str>) -> LogLevel {
    if debug {
        return LogLevel::Debug;
    }
    match env {
        Some("trace") => LogLevel::Trace,
        Some("debug") => LogLevel::Debug,
        Some("warn") => LogLevel::Warn,
        Some("error") => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

mod tests;
