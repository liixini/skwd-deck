use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;

pub(crate) fn normalized(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_string()
}

pub(crate) fn running() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let uid = unsafe { libc::getuid() };
    let mut names = BTreeSet::new();
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().parse::<u32>().is_err()
            || !entry.metadata().is_ok_and(|meta| meta.uid() == uid)
        {
            continue;
        }
        let path = entry.path();
        if let Ok(bytes) = std::fs::read(path.join("cmdline")) {
            let name =
                String::from_utf8_lossy(bytes.split(|byte| *byte == 0).next().unwrap_or_default());
            let name = name.rsplit(['/', '\\']).next().unwrap_or(&name).trim();
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
        if let Ok(name) = std::fs::read_to_string(path.join("comm")) {
            let name = name.trim();
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
    }
    names.into_iter().collect()
}

pub(crate) fn matches(rules: &str, running: &[String]) -> Vec<String> {
    rules
        .split([',', '\n'])
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .filter(|rule| running.iter().any(|name| normalized(name) == normalized(rule)))
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests;
