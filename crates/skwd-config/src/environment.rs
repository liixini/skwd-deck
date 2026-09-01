use std::path::PathBuf;

pub fn home() -> String {
    env("HOME").unwrap_or_else(|| String::from("/"))
}

pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|val| !val.is_empty())
}

pub fn config_dir() -> String {
    env("SKWD_WALL_V2_CONFIG").unwrap_or_else(|| {
        let base = env("XDG_CONFIG_HOME").unwrap_or_else(|| format!("{}/.config", home()));
        format!("{base}/skwd-wall-v2")
    })
}

pub fn config_path() -> PathBuf {
    PathBuf::from(config_dir()).join("config.json")
}

pub fn cache_dir() -> String {
    env("SKWD_WALL_V2_CACHE").unwrap_or_else(|| {
        let base = env("XDG_CACHE_HOME").unwrap_or_else(|| format!("{}/.cache", home()));
        format!("{base}/skwd-wall-v2")
    })
}

pub fn resolve(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        format!("{}/{}", home(), rest)
    } else if path == "~" {
        home()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;
