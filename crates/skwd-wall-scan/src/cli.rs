use std::path::PathBuf;

pub(crate) enum Command {
    Version,
    Ansi16 { image: String, dark: bool, auto: bool, variant: String },
    Tone { image: String },
    Semantic { image: String, dark: bool, auto: bool },
    ThemePreview { image: String, dark: bool },
    Recolor,
    Theme { image: String, dark: bool },
    RemoteThumb { source: String },
    Preview { key: String, video: String },
    Stream { video: String },
    StreamPersist,
    Paths { changed: Vec<PathBuf>, request_id: Option<String> },
    SceneAudit { dir: Option<String> },
    SceneProbe { dir: String },
    FullScan { request_id: Option<String> },
}

pub(crate) fn parse(arguments: &[String]) -> Command {
    let dark = !has(arguments, "--light");
    let auto = has(arguments, "--auto");

    if has(arguments, "--version") || has(arguments, "-V") {
        return Command::Version;
    }
    if let Some(position) = position(arguments, "--ansi16") {
        return Command::Ansi16 {
            image: arguments.get(position + 1).cloned().unwrap_or_default(),
            dark,
            auto,
            variant: value_after(arguments, "--variant").unwrap_or("dark16").to_string(),
        };
    }
    if let Some(position) = position(arguments, "--tone") {
        return Command::Tone { image: arguments.get(position + 1).cloned().unwrap_or_default() };
    }
    if let Some(position) = position(arguments, "--semantic") {
        return Command::Semantic {
            image: arguments.get(position + 1).cloned().unwrap_or_default(),
            dark,
            auto,
        };
    }
    if let Some(position) = position(arguments, "--theme-preview") {
        return Command::ThemePreview {
            image: arguments.get(position + 1).cloned().unwrap_or_default(),
            dark,
        };
    }
    if has(arguments, "--recolor") {
        return Command::Recolor;
    }
    if let Some(position) = position(arguments, "--theme") {
        return Command::Theme {
            image: arguments.get(position + 1).cloned().unwrap_or_default(),
            dark,
        };
    }
    if let Some(position) = position(arguments, "--remote-thumb") {
        return Command::RemoteThumb {
            source: arguments.get(position + 1).cloned().unwrap_or_default(),
        };
    }
    if let Some(position) = position(arguments, "--preview") {
        return Command::Preview {
            key: arguments.get(position + 1).cloned().unwrap_or_default(),
            video: arguments.get(position + 2).cloned().unwrap_or_default(),
        };
    }
    if let Some(position) = position(arguments, "--stream") {
        return Command::Stream { video: arguments.get(position + 1).cloned().unwrap_or_default() };
    }
    if has(arguments, "--stream-persist") {
        return Command::StreamPersist;
    }
    if let Some(position) = position(arguments, "--paths") {
        return Command::Paths {
            changed: path_values(arguments, position + 1),
            request_id: value_after(arguments, "--scan-request-id").map(String::from),
        };
    }
    if let Some(position) = position(arguments, "--scene-audit") {
        return Command::SceneAudit {
            dir: arguments.get(position + 1).filter(|arg| !arg.starts_with('-')).cloned(),
        };
    }
    if let Some(position) = position(arguments, "--scene-probe") {
        return Command::SceneProbe {
            dir: arguments.get(position + 1).cloned().unwrap_or_default(),
        };
    }
    Command::FullScan { request_id: value_after(arguments, "--scan-request-id").map(String::from) }
}

fn path_values(arguments: &[String], start: usize) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut index = start;
    while let Some(value) = arguments.get(index) {
        if value == "--scan-request-id" {
            index += 2;
        } else {
            paths.push(PathBuf::from(value));
            index += 1;
        }
    }
    paths
}

fn has(arguments: &[String], flag: &str) -> bool {
    arguments.iter().any(|argument| argument == flag)
}

fn position(arguments: &[String], flag: &str) -> Option<usize> {
    arguments.iter().position(|argument| argument == flag)
}

fn value_after<'a>(arguments: &'a [String], flag: &str) -> Option<&'a str> {
    position(arguments, flag).and_then(|position| arguments.get(position + 1)).map(String::as_str)
}

mod tests;
